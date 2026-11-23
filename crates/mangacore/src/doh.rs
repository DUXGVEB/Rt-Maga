//! Provider-based RFC 8484 DNS-over-HTTPS (`DoH`) resolver with bootstrap IPs, TTL caching,
//! DNS-rebinding SSRF filtering, and a modular built-in provider catalog.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use reqwest::Client;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use url::Url;

use crate::error::ScrapeError;
use crate::http::{DEFAULT_USER_AGENT, is_disallowed_ip};

/// Timeout for individual RFC 8484 DNS-over-HTTPS lookups.
const DOH_QUERY_TIMEOUT: Duration = Duration::from_secs(6);

/// Minimum and maximum DNS cache entry TTL bounds.
const MIN_CACHE_TTL_SECS: u32 = 30;
const MAX_CACHE_TTL_SECS: u32 = 3600;

/// Maximum number of cached hostnames kept in RAM.
const MAX_DNS_CACHE_ENTRIES: usize = 512;

/// DNS query record type constants (RFC 1035 / RFC 3596).
const QTYPE_A: u16 = 1;
const QTYPE_AAAA: u16 = 28;
const QCLASS_IN: u16 = 1;

/// High-level category distinguishing DNS encryption, DNS filtering, and tunneling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NetworkServiceType {
    /// Encrypts DNS lookups via HTTPS (bypasses ISP DNS blocking; does not hide IP).
    #[default]
    EncryptedDns,
    /// Encrypted DNS plus server-side blocking of ad, tracker, malware, or adult domains.
    DnsFiltering,
    /// Full traffic forwarding via a local SOCKS5/HTTP tunnel or external VPN client.
    LocalTunnel,
}

/// Selectable filtering/protection profile offered by a DNS provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DohProfileInfo {
    /// Stable profile identifier (e.g. `"default"`, `"adblock"`, `"malware"`, `"family"`).
    pub id: String,
    /// Human-readable short profile label.
    pub label: String,
    /// Brief description of what this profile filters or resolves.
    pub description: String,
    /// RFC 8484 HTTPS endpoint URL (or template containing `{id}` for account-based providers).
    pub endpoint_url: String,
    /// Service category badge for this specific profile.
    pub service_type: NetworkServiceType,
}

/// Built-in provider entry presented in the Settings Network Protection catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProviderInfo {
    /// Stable provider identifier (e.g. `"cloudflare"`, `"adguard"`, `"mullvad"`).
    pub id: String,
    /// Display name of the provider.
    pub name: String,
    /// Primary service category.
    pub service_type: NetworkServiceType,
    /// Concise summary of capabilities.
    pub summary: String,
    /// Honest note describing limitations (e.g., DNS filtering is not a VPN).
    pub limitations: String,
    /// Whether this provider requires a user-supplied profile/account ID (e.g. `NextDNS`).
    pub requires_account_id: bool,
    /// Label for the account/profile ID input when `requires_account_id` is `true`.
    pub account_id_label: Option<String>,
    /// Available profiles under this provider.
    pub profiles: Vec<DohProfileInfo>,
}

/// Persisted in-app DNS-over-HTTPS (`DoH`) provider configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DohConfig {
    /// `false` (default) = OS system DNS; `true` = In-app RFC 8484 `DoH` resolver.
    pub enabled: bool,
    /// Selected provider ID from [`network_provider_catalog`].
    pub provider_id: String,
    /// Selected profile ID under the chosen provider.
    pub profile_id: String,
    /// Optional user profile ID for account-based providers (e.g. `NextDNS`).
    pub custom_profile_id: Option<String>,
    /// Fall back to system DNS if the `DoH` server is unreachable.
    pub allow_system_fallback: bool,
}

impl Default for DohConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider_id: "cloudflare".to_owned(),
            profile_id: "default".to_owned(),
            custom_profile_id: None,
            allow_system_fallback: true,
        }
    }
}

impl DohConfig {
    /// Normalizes whitespace and ensures `provider_id` / `profile_id` refer to valid catalog entries.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.provider_id = self.provider_id.trim().to_ascii_lowercase();
        self.profile_id = self.profile_id.trim().to_ascii_lowercase();
        if let Some(id) = self.custom_profile_id.as_mut() {
            let clean: String = id
                .trim()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                .take(48)
                .collect();
            if clean.is_empty() {
                self.custom_profile_id = None;
            } else {
                *id = clean;
            }
        }

        let catalog = network_provider_catalog();
        if let Some(provider) = catalog.iter().find(|p| p.id == self.provider_id) {
            if !provider.profiles.iter().any(|pr| pr.id == self.profile_id)
                && let Some(first) = provider.profiles.first()
            {
                self.profile_id.clone_from(&first.id);
            }
        } else {
            "cloudflare".clone_into(&mut self.provider_id);
            "default".clone_into(&mut self.profile_id);
        }
        self
    }

    /// Resolves the concrete `DoH` HTTPS endpoint URL and official bootstrap IPs for this configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Proxy`] if a required account/profile ID (such as `NextDNS`) is missing.
    pub fn resolve_endpoint(&self) -> Result<ResolvedDohEndpoint, ScrapeError> {
        let catalog = network_provider_catalog();
        let provider = catalog
            .iter()
            .find(|p| p.id == self.provider_id)
            .ok_or_else(|| {
                ScrapeError::Proxy(format!("unknown DNS provider '{}'", self.provider_id))
            })?;
        let profile = provider
            .profiles
            .iter()
            .find(|pr| pr.id == self.profile_id)
            .or_else(|| provider.profiles.first())
            .ok_or_else(|| {
                ScrapeError::Proxy(format!(
                    "no profile available for provider '{}'",
                    provider.name
                ))
            })?;

        let url_str = if provider.requires_account_id {
            let pid = self
                .custom_profile_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ScrapeError::Proxy(format!(
                        "{} requires your Profile ID before enabling",
                        provider.name
                    ))
                })?;
            profile.endpoint_url.replace("{id}", pid)
        } else {
            profile.endpoint_url.clone()
        };

        let url = Url::parse(&url_str)
            .map_err(|e| ScrapeError::Proxy(format!("invalid DoH endpoint URL: {e}")))?;
        let host = url
            .host_str()
            .ok_or_else(|| ScrapeError::Proxy("DoH endpoint missing host".to_owned()))?
            .to_owned();
        let bootstrap_ips = bootstrap_ips_for_host(&host);

        Ok(ResolvedDohEndpoint {
            provider_name: provider.name.clone(),
            profile_label: profile.label.clone(),
            url,
            host,
            bootstrap_ips,
        })
    }
}

/// Resolved concrete `DoH` endpoint metadata ready for [`DohResolver`].
#[derive(Debug, Clone)]
pub struct ResolvedDohEndpoint {
    /// Human-readable provider name.
    pub provider_name: String,
    /// Human-readable profile label.
    pub profile_label: String,
    /// Full RFC 8484 HTTPS endpoint URL.
    pub url: Url,
    /// Hostname used for TLS SNI and bootstrap pinning.
    pub host: String,
    /// Official anycast bootstrap IP addresses preventing ISP DNS blocking of the `DoH` server.
    pub bootstrap_ips: Vec<SocketAddr>,
}

/// Connectivity and latency test result returned by [`test_doh_config`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DohTestResult {
    /// `true` when the provider answered a valid RFC 8484 DNS query.
    pub ok: bool,
    /// Round-trip query latency in milliseconds.
    pub latency_ms: u64,
    /// Provider display name.
    pub provider_name: String,
    /// Profile display label.
    pub profile_label: String,
    /// Number of valid public IP addresses returned in the test query.
    pub resolved_ips: usize,
    /// Sanitized error message when `ok` is `false`.
    pub error: Option<String>,
}

/// Returns the built-in catalog of reputable, free-to-use DNS-over-HTTPS providers and profiles.
pub fn network_provider_catalog() -> Vec<NetworkProviderInfo> {
    vec![
        cloudflare_provider(),
        adguard_provider(),
        mullvad_provider(),
        quad9_provider(),
        controld_provider(),
        google_provider(),
        nextdns_provider(),
    ]
}

fn cloudflare_provider() -> NetworkProviderInfo {
    NetworkProviderInfo {
        id: "cloudflare".to_owned(),
        name: "Cloudflare DNS (1.1.1.1)".to_owned(),
        service_type: NetworkServiceType::EncryptedDns,
        summary: "Fast global encrypted DNS with optional malware and family filtering.".to_owned(),
        limitations: "Encrypted DNS only — bypasses ISP DNS blocks inside RtMaga, does not hide your IP address. Full WARP tunneling requires the official WARP client (or its local SOCKS5 proxy mode).".to_owned(),
        requires_account_id: false,
        account_id_label: None,
        profiles: vec![
            DohProfileInfo {
                id: "default".to_owned(),
                label: "Default (1.1.1.1)".to_owned(),
                description: "Fast unfiltered encrypted DNS.".to_owned(),
                endpoint_url: "https://cloudflare-dns.com/dns-query".to_owned(),
                service_type: NetworkServiceType::EncryptedDns,
            },
            DohProfileInfo {
                id: "malware".to_owned(),
                label: "Malware Protection".to_owned(),
                description: "Blocks known malware and phishing domains.".to_owned(),
                endpoint_url: "https://security.cloudflare-dns.com/dns-query".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
            DohProfileInfo {
                id: "family".to_owned(),
                label: "Family Protection".to_owned(),
                description: "Blocks malware and adult domains.".to_owned(),
                endpoint_url: "https://family.cloudflare-dns.com/dns-query".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
        ],
    }
}

fn adguard_provider() -> NetworkProviderInfo {
    NetworkProviderInfo {
        id: "adguard".to_owned(),
        name: "AdGuard DNS".to_owned(),
        service_type: NetworkServiceType::DnsFiltering,
        summary: "Encrypted DNS with built-in ad, tracker, and phishing domain blocking.".to_owned(),
        limitations: "DNS-level filtering only — blocks third-party ad/tracker domains, not a VPN and cannot hide your IP or block same-domain inline ads.".to_owned(),
        requires_account_id: false,
        account_id_label: None,
        profiles: vec![
            DohProfileInfo {
                id: "adblock".to_owned(),
                label: "Ad & Tracker Blocking".to_owned(),
                description: "Default AdGuard DNS filtering for ads and trackers.".to_owned(),
                endpoint_url: "https://dns.adguard-dns.com/dns-query".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
            DohProfileInfo {
                id: "unfiltered".to_owned(),
                label: "Unfiltered".to_owned(),
                description: "Encrypted DNS without domain filtering.".to_owned(),
                endpoint_url: "https://unfiltered.adguard-dns.com/dns-query".to_owned(),
                service_type: NetworkServiceType::EncryptedDns,
            },
            DohProfileInfo {
                id: "family".to_owned(),
                label: "Family Protection".to_owned(),
                description: "Blocks ads, trackers, and adult content.".to_owned(),
                endpoint_url: "https://family.adguard-dns.com/dns-query".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
        ],
    }
}

fn mullvad_provider() -> NetworkProviderInfo {
    NetworkProviderInfo {
        id: "mullvad".to_owned(),
        name: "Mullvad DNS".to_owned(),
        service_type: NetworkServiceType::DnsFiltering,
        summary: "Privacy-focused no-logging encrypted DNS with tiered filtering profiles.".to_owned(),
        limitations: "DNS-level protection only — does not route traffic through Mullvad VPN tunnels or mask your IP address.".to_owned(),
        requires_account_id: false,
        account_id_label: None,
        profiles: vec![
            DohProfileInfo {
                id: "adblock".to_owned(),
                label: "Ad & Tracker Blocking".to_owned(),
                description: "Blocks advertising and tracking domains.".to_owned(),
                endpoint_url: "https://adblock.dns.mullvad.net/dns-query".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
            DohProfileInfo {
                id: "base".to_owned(),
                label: "Ads + Trackers + Malware".to_owned(),
                description: "Combines ad/tracker blocking with malware protection.".to_owned(),
                endpoint_url: "https://base.dns.mullvad.net/dns-query".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
            DohProfileInfo {
                id: "default".to_owned(),
                label: "Default (Unfiltered)".to_owned(),
                description: "No-logging encrypted DNS with no blocking.".to_owned(),
                endpoint_url: "https://dns.mullvad.net/dns-query".to_owned(),
                service_type: NetworkServiceType::EncryptedDns,
            },
            DohProfileInfo {
                id: "family".to_owned(),
                label: "Family Protection".to_owned(),
                description: "Blocks ads, trackers, malware, adult, and gambling domains.".to_owned(),
                endpoint_url: "https://family.dns.mullvad.net/dns-query".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
        ],
    }
}

fn quad9_provider() -> NetworkProviderInfo {
    NetworkProviderInfo {
        id: "quad9".to_owned(),
        name: "Quad9".to_owned(),
        service_type: NetworkServiceType::DnsFiltering,
        summary: "Swiss non-profit encrypted DNS blocking malicious and phishing hosts.".to_owned(),
        limitations:
            "DNS-level security filtering only — not a VPN and does not hide your IP address."
                .to_owned(),
        requires_account_id: false,
        account_id_label: None,
        profiles: vec![
            DohProfileInfo {
                id: "malware".to_owned(),
                label: "Malware Blocking (Recommended)".to_owned(),
                description: "Threat-intelligence blocking with DNSSEC validation.".to_owned(),
                endpoint_url: "https://dns.quad9.net/dns-query".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
            DohProfileInfo {
                id: "unfiltered".to_owned(),
                label: "Unfiltered".to_owned(),
                description: "Encrypted DNS without threat blocking.".to_owned(),
                endpoint_url: "https://dns10.quad9.net/dns-query".to_owned(),
                service_type: NetworkServiceType::EncryptedDns,
            },
        ],
    }
}

fn controld_provider() -> NetworkProviderInfo {
    NetworkProviderInfo {
        id: "controld".to_owned(),
        name: "Control D".to_owned(),
        service_type: NetworkServiceType::DnsFiltering,
        summary: "Free public encrypted DNS with malware, ad/tracker, and family filters."
            .to_owned(),
        limitations: "Free DNS profiles do not perform geo-unblocking or full VPN tunneling."
            .to_owned(),
        requires_account_id: false,
        account_id_label: None,
        profiles: vec![
            DohProfileInfo {
                id: "p2".to_owned(),
                label: "Ads & Tracking".to_owned(),
                description: "Blocks malware, ads, and trackers.".to_owned(),
                endpoint_url: "https://freedns.controld.com/p2".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
            DohProfileInfo {
                id: "p1".to_owned(),
                label: "Malware Protection".to_owned(),
                description: "Blocks malware and phishing domains.".to_owned(),
                endpoint_url: "https://freedns.controld.com/p1".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
            DohProfileInfo {
                id: "p0".to_owned(),
                label: "Unfiltered".to_owned(),
                description: "Fast encrypted DNS with no filtering.".to_owned(),
                endpoint_url: "https://freedns.controld.com/p0".to_owned(),
                service_type: NetworkServiceType::EncryptedDns,
            },
            DohProfileInfo {
                id: "family".to_owned(),
                label: "Family Protection".to_owned(),
                description: "Blocks malware, ads, trackers, and adult content.".to_owned(),
                endpoint_url: "https://freedns.controld.com/family".to_owned(),
                service_type: NetworkServiceType::DnsFiltering,
            },
        ],
    }
}

fn google_provider() -> NetworkProviderInfo {
    NetworkProviderInfo {
        id: "google".to_owned(),
        name: "Google Public DNS".to_owned(),
        service_type: NetworkServiceType::EncryptedDns,
        summary: "Reliable global encrypted DNS (8.8.8.8) with DNSSEC support.".to_owned(),
        limitations: "Encrypted DNS only — no ad blocking and does not mask your IP address."
            .to_owned(),
        requires_account_id: false,
        account_id_label: None,
        profiles: vec![DohProfileInfo {
            id: "default".to_owned(),
            label: "Default (8.8.8.8)".to_owned(),
            description: "Unfiltered encrypted DNS.".to_owned(),
            endpoint_url: "https://dns.google/dns-query".to_owned(),
            service_type: NetworkServiceType::EncryptedDns,
        }],
    }
}

fn nextdns_provider() -> NetworkProviderInfo {
    NetworkProviderInfo {
        id: "nextdns".to_owned(),
        name: "NextDNS".to_owned(),
        service_type: NetworkServiceType::DnsFiltering,
        summary: "Custom cloud DNS filtering using your personal NextDNS Profile ID.".to_owned(),
        limitations: "Requires a free NextDNS account/Profile ID configured at my.nextdns.io. DNS filtering only (not a VPN).".to_owned(),
        requires_account_id: true,
        account_id_label: Some("NextDNS Profile ID".to_owned()),
        profiles: vec![DohProfileInfo {
            id: "custom".to_owned(),
            label: "Custom Profile".to_owned(),
            description: "Uses the blocklists and rules from your NextDNS profile.".to_owned(),
            endpoint_url: "https://dns.nextdns.io/{id}".to_owned(),
            service_type: NetworkServiceType::DnsFiltering,
        }],
    }
}

fn bootstrap_ips_for_host(host: &str) -> Vec<SocketAddr> {
    let ips: &[Ipv4Addr] = match host {
        "cloudflare-dns.com" => &[Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(1, 0, 0, 1)],
        "security.cloudflare-dns.com" => &[Ipv4Addr::new(1, 1, 1, 2), Ipv4Addr::new(1, 0, 0, 2)],
        "family.cloudflare-dns.com" => &[Ipv4Addr::new(1, 1, 1, 3), Ipv4Addr::new(1, 0, 0, 3)],
        "dns.adguard-dns.com" => &[
            Ipv4Addr::new(94, 140, 14, 14),
            Ipv4Addr::new(94, 140, 15, 15),
        ],
        "unfiltered.adguard-dns.com" => &[
            Ipv4Addr::new(94, 140, 14, 140),
            Ipv4Addr::new(94, 140, 14, 141),
        ],
        "family.adguard-dns.com" => &[
            Ipv4Addr::new(94, 140, 14, 15),
            Ipv4Addr::new(94, 140, 15, 16),
        ],
        "dns.mullvad.net" => &[Ipv4Addr::new(194, 242, 2, 2)],
        "adblock.dns.mullvad.net" => &[Ipv4Addr::new(194, 242, 2, 3)],
        "base.dns.mullvad.net" => &[Ipv4Addr::new(194, 242, 2, 4)],
        "family.dns.mullvad.net" => &[Ipv4Addr::new(194, 242, 2, 6)],
        "dns.quad9.net" => &[Ipv4Addr::new(9, 9, 9, 9), Ipv4Addr::new(149, 112, 112, 112)],
        "dns10.quad9.net" => &[Ipv4Addr::new(9, 9, 9, 10), Ipv4Addr::new(149, 112, 112, 10)],
        "freedns.controld.com" => &[Ipv4Addr::new(76, 76, 2, 0), Ipv4Addr::new(76, 76, 10, 0)],
        "dns.google" => &[Ipv4Addr::new(8, 8, 8, 8), Ipv4Addr::new(8, 8, 4, 4)],
        "dns.nextdns.io" => &[Ipv4Addr::new(45, 90, 28, 0), Ipv4Addr::new(45, 90, 30, 0)],
        _ => &[],
    };
    ips.iter()
        .map(|ip| SocketAddr::new(IpAddr::V4(*ip), 443))
        .collect()
}

#[derive(Debug, Clone)]
struct CachedDnsEntry {
    addrs: Vec<IpAddr>,
    expires_at: Instant,
}

/// Custom `reqwest` DNS resolver backed by RFC 8484 DNS-over-HTTPS with TTL caching.
#[derive(Debug)]
pub struct DohResolver {
    endpoint: ResolvedDohEndpoint,
    client: Client,
    cache: Arc<DashMap<String, CachedDnsEntry>>,
    allow_system_fallback: bool,
}

/// Controls fallback behavior when a `DoH` lookup cannot reach the configured provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DohFallbackPolicy {
    /// Fall back to OS system DNS resolution.
    SystemDns,
    /// Fail closed without falling back to unencrypted OS DNS.
    FailClosed,
}

impl DohResolver {
    /// Builds a new [`DohResolver`] pinned to the provider's official bootstrap IPs.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Proxy`] if the `DoH` HTTP client cannot be constructed.
    pub fn new(
        endpoint: ResolvedDohEndpoint,
        fallback: DohFallbackPolicy,
    ) -> Result<Self, ScrapeError> {
        let mut builder = Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .timeout(DOH_QUERY_TIMEOUT)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none());

        if !endpoint.bootstrap_ips.is_empty() {
            builder = builder.resolve_to_addrs(&endpoint.host, &endpoint.bootstrap_ips);
        }

        let client = builder
            .build()
            .map_err(|e| ScrapeError::Proxy(format!("failed to initialize DoH client: {e}")))?;

        Ok(Self {
            endpoint,
            client,
            cache: Arc::new(DashMap::new()),
            allow_system_fallback: fallback == DohFallbackPolicy::SystemDns,
        })
    }

    async fn resolve_host(&self, host: &str) -> Result<Vec<IpAddr>, String> {
        let lower = host.trim().to_ascii_lowercase();
        if let Ok(ip) = lower.trim_matches(['[', ']']).parse::<IpAddr>() {
            return Ok(vec![ip]);
        }

        if let Some(entry) = self.cache.get(&lower)
            && Instant::now() < entry.expires_at
        {
            return Ok(entry.addrs.clone());
        }

        match self.query_doh_records(&lower).await {
            Ok((ips, ttl)) if !ips.is_empty() => {
                if self.cache.len() >= MAX_DNS_CACHE_ENTRIES {
                    self.cache.clear();
                }
                let clamped_ttl = ttl.clamp(MIN_CACHE_TTL_SECS, MAX_CACHE_TTL_SECS);
                self.cache.insert(
                    lower,
                    CachedDnsEntry {
                        addrs: ips.clone(),
                        expires_at: Instant::now() + Duration::from_secs(u64::from(clamped_ttl)),
                    },
                );
                Ok(ips)
            }
            Ok(_) => Err(format!(
                "DoH provider returned no public addresses for {lower}"
            )),
            Err(err) => {
                if self.allow_system_fallback {
                    warn!(%err, host = %lower, "DoH lookup failed; falling back to system DNS");
                    resolve_system_dns(&lower).await
                } else {
                    Err(err)
                }
            }
        }
    }

    async fn query_doh_records(&self, host: &str) -> Result<(Vec<IpAddr>, u32), String> {
        let (mut ips, mut min_ttl) = self.send_single_query(host, QTYPE_A).await?;
        if ips.is_empty()
            && let Ok((v6_ips, v6_ttl)) = self.send_single_query(host, QTYPE_AAAA).await
        {
            ips.extend(v6_ips);
            min_ttl = min_ttl.min(v6_ttl);
        }
        // Reject private/loopback/link-local answers (defense against DNS rebinding SSRF).
        ips.retain(|ip| !is_disallowed_ip(*ip));
        Ok((ips, min_ttl))
    }

    async fn send_single_query(
        &self,
        host: &str,
        qtype: u16,
    ) -> Result<(Vec<IpAddr>, u32), String> {
        let wire_query = encode_dns_query(host, qtype)?;
        let resp = self
            .client
            .post(self.endpoint.url.as_str())
            .header(CONTENT_TYPE, "application/dns-message")
            .header(ACCEPT, "application/dns-message")
            .body(wire_query)
            .send()
            .await
            .map_err(|e| format!("DoH transport error: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!(
                "DoH server returned HTTP {}",
                resp.status().as_u16()
            ));
        }
        let body = resp
            .bytes()
            .await
            .map_err(|e| format!("failed to read DoH response: {e}"))?;
        decode_dns_response(&body)
    }
}

impl Resolve for DohResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let endpoint = self.endpoint.clone();
        let client = self.client.clone();
        let cache = Arc::clone(&self.cache);
        let allow_fallback = self.allow_system_fallback;

        Box::pin(async move {
            let helper = DohResolver {
                endpoint,
                client,
                cache,
                allow_system_fallback: allow_fallback,
            };
            match helper.resolve_host(&host).await {
                Ok(ips) => {
                    debug!(%host, count = ips.len(), "resolved host via DoH");
                    let addrs: Vec<SocketAddr> =
                        ips.into_iter().map(|ip| SocketAddr::new(ip, 0)).collect();
                    let iter: Addrs = Box::new(addrs.into_iter());
                    Ok(iter)
                }
                Err(msg) => {
                    let err: Box<dyn std::error::Error + Send + Sync> =
                        Box::new(std::io::Error::other(msg));
                    Err(err)
                }
            }
        })
    }
}

/// Tests connectivity and round-trip latency of a [`DohConfig`] using a real RFC 8484 query.
///
/// # Errors
///
/// Returns [`ScrapeError::Proxy`] only if the configuration itself is syntactically invalid.
pub async fn test_doh_config(config: &DohConfig) -> Result<DohTestResult, ScrapeError> {
    let clean = config.clone().sanitized();
    let endpoint = clean.resolve_endpoint()?;
    let provider_name = endpoint.provider_name.clone();
    let profile_label = endpoint.profile_label.clone();

    let resolver = DohResolver::new(endpoint, DohFallbackPolicy::FailClosed)?;
    let started = Instant::now();
    let outcome = resolver.query_doh_records("example.com").await;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    match outcome {
        Ok((ips, _ttl)) if !ips.is_empty() => Ok(DohTestResult {
            ok: true,
            latency_ms,
            provider_name,
            profile_label,
            resolved_ips: ips.len(),
            error: None,
        }),
        Ok(_) => Ok(DohTestResult {
            ok: false,
            latency_ms,
            provider_name,
            profile_label,
            resolved_ips: 0,
            error: Some("Provider returned no valid IP addresses".to_owned()),
        }),
        Err(err) => Ok(DohTestResult {
            ok: false,
            latency_ms,
            provider_name,
            profile_label,
            resolved_ips: 0,
            error: Some(err),
        }),
    }
}

async fn resolve_system_dns(host: &str) -> Result<Vec<IpAddr>, String> {
    let addrs = tokio::net::lookup_host((host, 443))
        .await
        .map_err(|e| format!("system DNS fallback failed: {e}"))?;
    let ips: Vec<IpAddr> = addrs
        .map(|s| s.ip())
        .filter(|ip| !is_disallowed_ip(*ip))
        .collect();
    if ips.is_empty() {
        Err(format!(
            "system DNS returned no public addresses for {host}"
        ))
    } else {
        Ok(ips)
    }
}

/// Encodes a standard RFC 1035 DNS query packet with RD=1 for a single hostname and record type.
fn encode_dns_query(host: &str, qtype: u16) -> Result<Vec<u8>, String> {
    let trimmed = host.trim().trim_end_matches('.');
    if trimmed.is_empty() || trimmed.len() > 253 {
        return Err("invalid hostname length for DNS query".to_owned());
    }

    let mut packet = Vec::with_capacity(64);
    // Transaction ID = 0 (recommended by RFC 8484 §4.1 for HTTP cacheability)
    packet.extend_from_slice(&[0x00, 0x00]);
    // Flags: standard query with Recursion Desired (RD = 0x0100)
    packet.extend_from_slice(&[0x01, 0x00]);
    // QDCOUNT = 1, ANCOUNT = 0, NSCOUNT = 0, ARCOUNT = 0
    packet.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    for label in trimmed.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err("invalid DNS label length".to_owned());
        }
        let len_u8 = u8::try_from(label.len()).map_err(|_| "label too long".to_owned())?;
        packet.push(len_u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0x00); // root label
    packet.extend_from_slice(&qtype.to_be_bytes());
    packet.extend_from_slice(&QCLASS_IN.to_be_bytes());
    Ok(packet)
}

/// Decodes A/AAAA records and minimum TTL from an RFC 1035 / RFC 8484 wire-format response.
fn decode_dns_response(pkt: &[u8]) -> Result<(Vec<IpAddr>, u32), String> {
    if pkt.len() < 12 {
        return Err("truncated DNS response header".to_owned());
    }
    let flags = u16::from_be_bytes([pkt[2], pkt[3]]);
    let rcode = flags & 0x000f;
    if rcode != 0 {
        // RCODE 3 (NXDOMAIN) or refused/filtered returns empty list cleanly.
        if rcode == 3 {
            return Ok((Vec::new(), MIN_CACHE_TTL_SECS));
        }
        return Err(format!("DNS server returned RCODE {rcode}"));
    }

    let qdcount = u16::from_be_bytes([pkt[4], pkt[5]]);
    let ancount = u16::from_be_bytes([pkt[6], pkt[7]]);

    let mut offset: usize = 12;
    for _ in 0..qdcount {
        offset = skip_dns_name(pkt, offset)?;
        offset = offset
            .checked_add(4)
            .filter(|&o| o <= pkt.len())
            .ok_or_else(|| "truncated DNS question section".to_owned())?;
    }

    let mut ips = Vec::new();
    let mut min_ttl = MAX_CACHE_TTL_SECS;

    for _ in 0..ancount {
        offset = skip_dns_name(pkt, offset)?;
        if offset.checked_add(10).is_none_or(|o| o > pkt.len()) {
            return Err("truncated DNS answer header".to_owned());
        }
        let rtype = u16::from_be_bytes([pkt[offset], pkt[offset + 1]]);
        let ttl = u32::from_be_bytes([
            pkt[offset + 4],
            pkt[offset + 5],
            pkt[offset + 6],
            pkt[offset + 7],
        ]);
        let rdlen = usize::from(u16::from_be_bytes([pkt[offset + 8], pkt[offset + 9]]));
        offset += 10;

        let Some(rdata_end) = offset.checked_add(rdlen).filter(|&e| e <= pkt.len()) else {
            return Err("truncated DNS answer RDATA".to_owned());
        };
        let rdata = &pkt[offset..rdata_end];
        offset = rdata_end;

        if rtype == QTYPE_A && rdata.len() == 4 {
            let ip = IpAddr::V4(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]));
            ips.push(ip);
            min_ttl = min_ttl.min(ttl);
        } else if rtype == QTYPE_AAAA && rdata.len() == 16 {
            let mut octets = [0_u8; 16];
            octets.copy_from_slice(rdata);
            ips.push(IpAddr::V6(Ipv6Addr::from(octets)));
            min_ttl = min_ttl.min(ttl);
        }
    }

    Ok((ips, min_ttl))
}

fn skip_dns_name(pkt: &[u8], mut offset: usize) -> Result<usize, String> {
    let mut steps = 0_u8;
    while offset < pkt.len() {
        steps = steps.saturating_add(1);
        if steps > 128 {
            return Err("malformed DNS name label chain".to_owned());
        }
        let len = pkt[offset];
        if len == 0 {
            return Ok(offset + 1);
        }
        // Compression pointer (top two bits set: 0xC0) occupies 2 bytes and terminates the name.
        if (len & 0xc0) == 0xc0 {
            return offset
                .checked_add(2)
                .filter(|&o| o <= pkt.len())
                .ok_or_else(|| "truncated DNS compression pointer".to_owned());
        }
        if (len & 0xc0) != 0 {
            return Err("invalid DNS label header bits".to_owned());
        }
        offset = offset
            .checked_add(1 + usize::from(len))
            .filter(|&o| o <= pkt.len())
            .ok_or_else(|| "DNS label exceeds packet bounds".to_owned())?;
    }
    Err("unterminated DNS name".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_rfc8484_dns_wireformat() {
        let query = encode_dns_query("example.com", QTYPE_A).expect("valid query");
        assert!(query.len() > 16, "query must include header + QNAME + tail");
        assert_eq!(query[12], 7, "first label 'example' has length 7");

        // Synthetic valid DNS response for example.com -> 93.184.216.34 (TTL 300)
        // plus a loopback 127.0.0.1 record to verify filtering.
        let mut resp = query;
        resp[2] = 0x81; // QR=1, RD=1
        resp[3] = 0x80; // RA=1, RCODE=0
        resp[6] = 0x00;
        resp[7] = 0x02; // ANCOUNT = 2
        // Answer 1: pointer to offset 12 (0xC00C), type A, class IN, TTL 300, rdlen 4, 93.184.216.34
        resp.extend_from_slice(&[
            0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0x2c, 0x00, 0x04, 93, 184, 216,
            34,
        ]);
        // Answer 2: 127.0.0.1 (disallowed loopback)
        resp.extend_from_slice(&[
            0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04, 127, 0, 0, 1,
        ]);

        let (mut ips, ttl) = decode_dns_response(&resp).expect("valid response");
        assert_eq!(ttl, 60, "minimum TTL across answers should be 60");
        assert_eq!(
            ips.len(),
            2,
            "decoder returns both records before SSRF filter"
        );
        ips.retain(|ip| !is_disallowed_ip(*ip));
        assert_eq!(
            ips,
            vec![IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))],
            "loopback IP must be filtered out by SSRF guard"
        );
    }

    #[test]
    fn provider_catalog_and_nextdns_validation() {
        let catalog = network_provider_catalog();
        assert!(catalog.len() >= 7, "must include all core DNS providers");
        for provider in &catalog {
            assert!(
                !provider.profiles.is_empty(),
                "provider {} must define at least one profile",
                provider.id
            );
        }

        // NextDNS requires a non-empty custom_profile_id.
        let missing_id = DohConfig {
            enabled: true,
            provider_id: "nextdns".to_owned(),
            profile_id: "custom".to_owned(),
            custom_profile_id: None,
            allow_system_fallback: true,
        };
        assert!(
            missing_id.resolve_endpoint().is_err(),
            "NextDNS without Profile ID must fail validation"
        );

        let valid_nextdns = DohConfig {
            custom_profile_id: Some("abc123".to_owned()),
            ..missing_id
        };
        let ep = valid_nextdns
            .resolve_endpoint()
            .expect("valid NextDNS config");
        assert_eq!(ep.url.as_str(), "https://dns.nextdns.io/abc123");
        assert!(
            !ep.bootstrap_ips.is_empty(),
            "NextDNS must include bootstrap IPs"
        );
    }
}
