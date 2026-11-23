<script lang="ts">
  import {
    Activity,
    AlertTriangle,
    CheckCircle2,
    ChevronDown,
    ChevronUp,
    Info,
    Loader2,
    RotateCcw,
    Shield,
    XCircle,
    Zap,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api } from "../api";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { DohTestResult, NetworkProviderInfo, NetworkServiceType } from "../types";
  import ProxySettingsPanel from "./ProxySettingsPanel.svelte";

  let providers = $state<NetworkProviderInfo[]>([]);
  let testing = $state(false);
  let resetting = $state(false);
  let testResult = $state<DohTestResult | null>(null);
  let showAdvancedProxy = $state(false);

  const s = $derived(settingsStore.value);
  const doh = $derived(s.doh);
  const t = $derived(settingsStore.t);

  const selectedProvider = $derived(
    providers.find((p) => p.id === doh.providerId) ?? providers[0] ?? null
  );
  const missingRequiredAccountId = $derived(
    doh.enabled &&
      Boolean(selectedProvider?.requiresAccountId) &&
      !(doh.customProfileId && doh.customProfileId.trim().length > 0)
  );

  onMount(async () => {
    try {
      providers = await api.getNetworkProviders();
    } catch {
      providers = [];
    }
    if (s.proxy.enabled) {
      showAdvancedProxy = true;
    }
  });

  async function resetNetworkSection() {
    if (resetting) return;
    resetting = true;
    testResult = null;
    try {
      await settingsStore.resetSection("network");
      showAdvancedProxy = false;
    } finally {
      resetting = false;
    }
  }

  function serviceTypeLabel(kind: NetworkServiceType): string {
    if (kind === "dns_filtering") return t.netTypeDnsFiltering;
    if (kind === "local_tunnel") return t.netTypeLocalTunnel;
    return t.netTypeEncryptedDns;
  }

  function selectProvider(provider: NetworkProviderInfo) {
    testResult = null;
    settingsStore.update((d) => {
      d.doh.providerId = provider.id;
      d.doh.profileId = provider.profiles[0]?.id ?? "default";
    });
  }

  function selectProfile(profileId: string) {
    testResult = null;
    settingsStore.update((d) => {
      d.doh.profileId = profileId;
    });
  }

  async function runDohTest() {
    if (testing) return;
    testing = true;
    testResult = null;
    try {
      testResult = await api.testDohProvider($state.snapshot(doh));
    } catch (err) {
      testResult = {
        ok: false,
        latencyMs: 0,
        providerName: selectedProvider?.name ?? doh.providerId,
        profileLabel: doh.profileId,
        resolvedIps: 0,
        error: String(err),
      };
    } finally {
      testing = false;
    }
  }

  function applyLocalWarpSocksPreset() {
    settingsStore.update((d) => {
      d.proxy.enabled = true;
      d.proxy.defaultProxy.protocol = "socks5";
      d.proxy.defaultProxy.host = "127.0.0.1";
      d.proxy.defaultProxy.port = 40000;
      d.proxy.defaultProxy.username = null;
      d.proxy.defaultProxy.password = null;
    });
    showAdvancedProxy = true;
    toasts.push(t.netLocalTunnelPresetApplied, "info", 3200);
  }
</script>

<section class="net-card">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div class="flex items-start gap-2.5">
      <div
        class="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-xl"
        style="background-color: var(--accent-soft); color: var(--accent);"
      >
        <Shield size={16} />
      </div>
      <div>
        <h2 class="text-sm font-semibold">{t.netProtectionSection}</h2>
        <p class="mt-0.5 text-xs" style="color: var(--text-muted);">
          {t.netProtectionSubtitle}
        </p>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-1.5">
      <button
        type="button"
        onclick={() => {
          testResult = null;
          settingsStore.update((d) => (d.doh.enabled = false));
        }}
        class="net-pill"
        class:net-pill-active={!doh.enabled}
      >
        {t.netSystemDns}
      </button>
      <button
        type="button"
        onclick={() => {
          testResult = null;
          settingsStore.update((d) => (d.doh.enabled = true));
        }}
        class="net-pill"
        class:net-pill-active={doh.enabled}
      >
        {t.netEncryptedDnsOn}
      </button>
      <button
        type="button"
        onclick={resetNetworkSection}
        disabled={resetting}
        class="net-reset-btn"
        title={t.resetSectionBtn}
      >
        <RotateCcw size={12} />
        <span>{t.resetSectionBtn}</span>
      </button>
    </div>
  </div>

  <!-- Honest DNS vs Tunnel explanation banner -->
  <div
    class="flex items-start gap-2 rounded-xl border px-3 py-2.5 text-[11px] leading-relaxed"
    style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
  >
    <Info size={14} class="mt-0.5 shrink-0" style="color: var(--accent);" />
    <span>{doh.enabled ? t.netHonestDisclaimer : t.netSystemDnsActiveNote}</span>
  </div>

  {#if doh.enabled}
    <!-- Zero-Config Provider Catalog -->
    <div class="flex flex-col gap-2">
      <div class="text-xs font-medium">{t.netSelectProvider}</div>
      <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {#each providers as provider (provider.id)}
          {@const active = doh.providerId === provider.id}
          <button
            type="button"
            onclick={() => selectProvider(provider)}
            class="flex flex-col gap-1 rounded-xl border p-3 text-left transition-all"
            style="border-color: {active
              ? 'var(--accent)'
              : 'var(--border)'}; background-color: {active
              ? 'color-mix(in srgb, var(--surface) 88%, var(--accent-soft))'
              : 'var(--surface-subtle)'};"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="text-xs font-semibold">{provider.name}</span>
              <span
                class="rounded-full px-2 py-0.5 text-[10px] font-medium"
                style="background-color: {active
                  ? 'var(--accent-soft)'
                  : 'var(--surface-hover)'}; color: {active ? 'var(--accent)' : 'var(--text-muted)'};"
              >
                {serviceTypeLabel(provider.serviceType)}
              </span>
            </div>
            <p class="text-[11px] leading-snug" style="color: var(--text-muted);">
              {provider.summary}
            </p>
          </button>
        {/each}
      </div>
    </div>

    <!-- Active Provider Profile & Account ID -->
    {#if selectedProvider}
      <div
        class="flex flex-col gap-3 rounded-xl border p-3.5"
        style="background-color: var(--surface-subtle); border-color: var(--border);"
      >
        <div class="flex flex-col gap-1.5">
          <div class="text-xs font-medium">{t.netProfileLabel}</div>
          <div class="flex flex-wrap gap-1.5">
            {#each selectedProvider.profiles as profile (profile.id)}
              <button
                type="button"
                onclick={() => selectProfile(profile.id)}
                class="net-pill"
                class:net-pill-active={doh.profileId === profile.id}
                title={profile.description}
              >
                {profile.label}
              </button>
            {/each}
          </div>
          {#each selectedProvider.profiles as profile (profile.id)}
            {#if profile.id === doh.profileId}
              <p class="text-[11px]" style="color: var(--text-muted);">{profile.description}</p>
            {/if}
          {/each}
        </div>

        {#if selectedProvider.requiresAccountId}
          <div class="flex flex-col gap-1">
            <label class="text-xs font-medium" for="doh-account-id">
              {selectedProvider.accountIdLabel ?? t.netAccountIdLabel}
            </label>
            <input
              id="doh-account-id"
              type="text"
              value={doh.customProfileId ?? ""}
              oninput={(e) => {
                const val = e.currentTarget.value.trim();
                settingsStore.update((d) => {
                  d.doh.customProfileId = val || null;
                }, 250);
              }}
              placeholder="e.g. a1b2c3"
              class="rounded-lg border px-3 py-1.5 text-xs outline-none"
              style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
            />
            {#if missingRequiredAccountId}
              <div
                class="mt-1 flex items-center gap-1.5 text-[11px] font-medium"
                style="color: var(--danger);"
              >
                <AlertTriangle size={12} class="shrink-0" />
                <span>{t.netMissingAccountIdWarn}</span>
              </div>
            {/if}
          </div>
        {/if}

        <div class="flex items-center justify-between gap-2 text-xs">
          <div>
            <div class="font-medium">{t.netSystemFallbackLabel}</div>
            <div class="text-[11px]" style="color: var(--text-muted);">
              {t.netSystemFallbackDesc}
            </div>
          </div>
          <input
            type="checkbox"
            checked={doh.allowSystemFallback}
            onchange={(e) => {
              const checked = e.currentTarget.checked;
              settingsStore.update((d) => (d.doh.allowSystemFallback = checked));
            }}
            class="h-4 w-4 accent-rose-500"
          />
        </div>

        <p class="text-[11px]" style="color: var(--text-muted);">{selectedProvider.limitations}</p>

        <!-- Live Connectivity & Latency Test -->
        <div class="flex flex-wrap items-center justify-between gap-2 pt-1">
          <button
            type="button"
            onclick={runDohTest}
            disabled={testing || missingRequiredAccountId}
            class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium disabled:opacity-50"
            style="background-color: var(--surface); border-color: var(--border);"
          >
            {#if testing}
              <Loader2 size={13} class="animate-spin" />
              <span>{t.proxyTesting}</span>
            {:else}
              <Activity size={13} />
              <span>{t.netTestDnsBtn}</span>
            {/if}
          </button>

          {#if testResult}
            <div
              class="inline-flex items-center gap-1.5 text-xs font-medium"
              style="color: {testResult.ok ? 'var(--success, #16a34a)' : 'var(--danger)'};"
            >
              {#if testResult.ok}
                <CheckCircle2 size={14} />
                <span>{t.netTestDnsOk(testResult.latencyMs, testResult.resolvedIps)}</span>
              {:else}
                <XCircle size={14} />
                <span>{testResult.error ?? t.netTestDnsFailed}</span>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  {/if}

  <!-- Local Tunnel Quick Preset + Collapsible Advanced Proxy -->
  <div
    class="flex flex-wrap items-center justify-between gap-2 border-t pt-3"
    style="border-color: var(--border);"
  >
    <button
      type="button"
      onclick={applyLocalWarpSocksPreset}
      class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium"
      style="background-color: var(--surface-subtle); border-color: var(--border);"
      title={t.netLocalTunnelPresetDesc}
    >
      <Zap size={13} style="color: var(--accent);" />
      <span>{t.netLocalTunnelPresetBtn}</span>
    </button>

    <button
      type="button"
      onclick={() => {
        showAdvancedProxy = !showAdvancedProxy;
      }}
      class="inline-flex items-center gap-1 text-xs font-medium"
      style="color: var(--text-muted);"
    >
      <span>{t.netAdvancedProxyToggle}</span>
      {#if showAdvancedProxy}
        <ChevronUp size={14} />
      {:else}
        <ChevronDown size={14} />
      {/if}
    </button>
  </div>
</section>

{#if showAdvancedProxy}
  <ProxySettingsPanel />
{/if}

<style>
  .net-card {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    padding: 1.25rem;
    border-radius: 1rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
  }

  .net-pill {
    padding: 0.3rem 0.7rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text-muted);
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 140ms ease;
  }

  .net-pill-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .net-reset-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.3rem 0.6rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text-muted);
    font-size: 0.72rem;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 120ms ease;
  }

  .net-reset-btn:hover:not(:disabled) {
    color: var(--text);
  }

  .net-reset-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
