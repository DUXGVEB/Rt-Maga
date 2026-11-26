<script lang="ts">
  import {
    Activity,
    CheckCircle2,
    Eye,
    EyeOff,
    Globe,
    Plus,
    ShieldAlert,
    Trash2,
    XCircle,
  } from "lucide-svelte";
  import { api, formatAppError } from "../api";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import ToggleSwitch from "./ToggleSwitch.svelte";
  import type {
    DomainProxyRule,
    DomainRuleAction,
    ProxyEndpoint,
    ProxyProtocol,
    ProxyTestResult,
    Settings,
  } from "../types";

  const s = $derived(settingsStore.value);
  const t = $derived(settingsStore.t);
  const proxy = $derived(s.proxy);

  let showPassword = $state(false);
  let testUrl = $state("https://example.com");
  let testing = $state(false);
  let testResult = $state<ProxyTestResult | null>(null);

  // New per-domain rule form state
  let newDomain = $state("");
  let newAction = $state<DomainRuleAction>("force_proxy");
  let customProtocol = $state<ProxyProtocol>("socks5");
  let customHost = $state("");
  let customPort = $state(1080);
  let customUsername = $state("");
  let customPassword = $state("");

  const hasForceProxyRule = $derived(
    proxy.domainRules.some((r) => r.action === "force_proxy")
  );

  function patch(mutator: (draft: Settings) => void, debounceMs = 0) {
    settingsStore.update(mutator, debounceMs);
  }

  function updateDefaultProxy(mutator: (ep: ProxyEndpoint) => void, debounceMs = 250) {
    testResult = null;
    patch((d) => mutator(d.proxy.defaultProxy), debounceMs);
  }

  async function runConnectionTest() {
    testing = true;
    testResult = null;
    try {
      testResult = await api.testProxy(proxy.defaultProxy, testUrl.trim() || null);
    } catch (err) {
      const f = formatAppError(err, s.language);
      testResult = {
        ok: false,
        latencyMs: 0,
        statusCode: null,
        targetUrl: testUrl.trim() || "https://example.com",
        viaProxy: "",
        error: f.detail,
      };
    } finally {
      testing = false;
    }
  }

  function normalizeDomainInput(raw: string): string {
    return raw
      .trim()
      .toLowerCase()
      .replace(/^https?:\/\//, "")
      .replace(/\/.*$/, "")
      .replace(/^\*\./, "")
      .replace(/^\.+|\.+$/g, "");
  }

  function addDomainRule() {
    const domain = normalizeDomainInput(newDomain);
    if (!domain) return;

    let customProxy: ProxyEndpoint | null = null;
    if (newAction === "custom_proxy") {
      const h = customHost.trim();
      if (!h) {
        toasts.push(t.proxyHostLabel, "error");
        return;
      }
      customProxy = {
        protocol: customProtocol,
        host: h,
        port: Math.max(1, Math.min(65535, Math.round(customPort || 1080))),
        username: customUsername.trim() || null,
        password: customPassword || null,
      };
    }

    const rule: DomainProxyRule = {
      domain,
      action: newAction,
      customProxy,
    };

    patch((d) => {
      const filtered = d.proxy.domainRules.filter(
        (r) => r.domain.toLowerCase() !== domain
      );
      d.proxy.domainRules = [...filtered, rule];
    });

    newDomain = "";
    customHost = "";
    customUsername = "";
    customPassword = "";
  }

  function removeDomainRule(domain: string) {
    patch((d) => {
      d.proxy.domainRules = d.proxy.domainRules.filter((r) => r.domain !== domain);
    });
  }

  function actionLabel(action: DomainRuleAction): string {
    switch (action) {
      case "force_proxy":
        return t.proxyRuleForceProxy;
      case "force_direct":
        return t.proxyRuleForceDirect;
      case "custom_proxy":
        return t.proxyRuleCustomProxy;
      default:
        return t.proxyDirect;
    }
  }
</script>

<section class="settings-card">
  <div class="flex items-start justify-between gap-3">
    <div>
      <h2 class="section-title flex items-center gap-2">
        <Globe size={15} />
        <span>{t.proxySection}</span>
      </h2>
    </div>

    <div class="flex shrink-0 gap-1.5">
      <button
        type="button"
        onclick={() => {
          testResult = null;
          patch((d) => (d.proxy.enabled = false));
        }}
        class="pill"
        class:pill-active={!proxy.enabled}
      >
        {t.proxyDirect}
      </button>
      <button
        type="button"
        onclick={() => {
          testResult = null;
          patch((d) => (d.proxy.enabled = true));
        }}
        class="pill"
        class:pill-active={proxy.enabled}
      >
        {t.proxyEnabled}
      </button>
    </div>
  </div>

  {#if proxy.enabled || hasForceProxyRule}
    <div
      class="flex flex-col gap-3 rounded-xl border p-3.5"
      style="border-color: var(--border); background-color: var(--surface-subtle);"
    >
      <!-- Protocol, Host, Port -->
      <div class="grid grid-cols-1 gap-2.5 sm:grid-cols-12">
        <label class="flex flex-col gap-1 sm:col-span-3">
          <span class="text-xs font-medium">{t.proxyProtocolLabel}</span>
          <select
            value={proxy.defaultProxy.protocol}
            onchange={(e) => {
              const proto = e.currentTarget.value as ProxyProtocol;
              updateDefaultProxy((ep) => {
                ep.protocol = proto;
                if (proto === "socks5" && ep.port === 8080) ep.port = 1080;
                if ((proto === "http" || proto === "https") && ep.port === 1080) ep.port = 8080;
              }, 0);
            }}
            class="field"
          >
            <option value="socks5">SOCKS5</option>
            <option value="http">HTTP</option>
            <option value="https">HTTPS</option>
          </select>
        </label>

        <label class="flex flex-col gap-1 sm:col-span-6">
          <span class="text-xs font-medium">{t.proxyHostLabel}</span>
          <input
            type="text"
            placeholder="127.0.0.1"
            value={proxy.defaultProxy.host}
            oninput={(e) => {
              const host = e.currentTarget.value;
              updateDefaultProxy((ep) => (ep.host = host), 300);
            }}
            class="field"
          />
        </label>

        <label class="flex flex-col gap-1 sm:col-span-3">
          <span class="text-xs font-medium">{t.proxyPortLabel}</span>
          <input
            type="number"
            min="1"
            max="65535"
            value={proxy.defaultProxy.port}
            oninput={(e) => {
              const port = Number(e.currentTarget.value);
              if (Number.isFinite(port) && port >= 1 && port <= 65535) {
                updateDefaultProxy((ep) => (ep.port = Math.round(port)), 250);
              }
            }}
            class="field"
          />
        </label>
      </div>

      <!-- Optional Auth -->
      <div class="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
        <label class="flex flex-col gap-1">
          <span class="text-xs font-medium">{t.proxyUsernameLabel}</span>
          <input
            type="text"
            autocomplete="off"
            value={proxy.defaultProxy.username ?? ""}
            oninput={(e) => {
              const val = e.currentTarget.value.trim();
              updateDefaultProxy((ep) => (ep.username = val || null), 300);
            }}
            class="field"
          />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-xs font-medium">{t.proxyPasswordLabel}</span>
          <div class="relative flex items-center">
            <input
              type={showPassword ? "text" : "password"}
              autocomplete="off"
              value={proxy.defaultProxy.password ?? ""}
              oninput={(e) => {
                const val = e.currentTarget.value;
                updateDefaultProxy((ep) => (ep.password = val || null), 300);
              }}
              class="field w-full pr-8"
            />
            <button
              type="button"
              onclick={() => (showPassword = !showPassword)}
              class="absolute right-2 opacity-70 hover:opacity-100"
              aria-label="Toggle password visibility"
            >
              {#if showPassword}
                <EyeOff size={14} />
              {:else}
                <Eye size={14} />
              {/if}
            </button>
          </div>
        </label>
      </div>

      <!-- Fail-closed toggle -->
      <div class="flex items-start justify-between gap-3 pt-1">
        <div class="flex items-center gap-2">
          <ShieldAlert size={15} class="shrink-0 opacity-75" />
          <div class="text-xs font-medium">{t.proxyFailClosedLabel}</div>
        </div>
        <ToggleSwitch
          size="sm"
          checked={proxy.allowDirectFallback}
          ariaLabel={t.proxyFailClosedLabel}
          onchange={(checked) => {
            patch((d) => (d.proxy.allowDirectFallback = checked));
          }}
        />
      </div>

      <!-- Connection Test -->
      <div class="flex flex-col gap-2 border-t pt-3" style="border-color: var(--border);">
        <div class="flex flex-wrap items-center gap-2">
          <input
            type="url"
            bind:value={testUrl}
            placeholder="https://example.com"
            aria-label={t.proxyTestUrlLabel}
            class="field min-w-44 flex-1"
          />
          <button
            type="button"
            onclick={runConnectionTest}
            disabled={testing || !proxy.defaultProxy.host.trim()}
            class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold disabled:opacity-45"
            style="background-color: var(--accent); color: var(--accent-fg);"
          >
            <Activity size={13} class={testing ? "animate-pulse" : ""} />
            <span>{testing ? t.proxyTesting : t.proxyTestBtn}</span>
          </button>
        </div>

        {#if testResult}
          <div
            class="flex items-start gap-2 rounded-lg border px-3 py-2 text-xs"
            style="border-color: {testResult.ok
              ? 'var(--success, #10b981)'
              : 'var(--danger)'}; color: {testResult.ok
              ? 'var(--success, #10b981)'
              : 'var(--danger)'};"
          >
            {#if testResult.ok}
              <CheckCircle2 size={14} class="mt-0.5 shrink-0" />
              <span>{t.proxyTestOk(testResult.latencyMs, testResult.statusCode)}</span>
            {:else}
              <XCircle size={14} class="mt-0.5 shrink-0" />
              <span>{t.proxyTestFailed(testResult.error ?? "Unknown error")}</span>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Per-Domain Rules -->
  <div class="flex flex-col gap-2.5 border-t pt-3" style="border-color: var(--border);">
    <div class="label">{t.proxyDomainRulesTitle}</div>

    {#if proxy.domainRules.length === 0}
      <div class="text-xs italic" style="color: var(--text-muted);">{t.proxyNoRules}</div>
    {:else}
      <div class="flex flex-col gap-1.5">
        {#each proxy.domainRules as rule (rule.domain)}
          <div
            class="flex flex-wrap items-center justify-between gap-2 rounded-lg border px-3 py-2 text-xs"
            style="border-color: var(--border); background-color: var(--surface-subtle);"
          >
            <div class="flex flex-wrap items-center gap-2">
              <span class="font-mono font-semibold">{rule.domain}</span>
              <span
                class="rounded-md px-2 py-0.5 text-[11px] font-medium"
                style="background-color: var(--accent-soft); color: var(--accent);"
              >
                {actionLabel(rule.action)}
              </span>
              {#if rule.action === "custom_proxy" && rule.customProxy}
                <span class="font-mono text-[11px]" style="color: var(--text-muted);">
                  {rule.customProxy.protocol}://{rule.customProxy.host}:{rule.customProxy.port}
                </span>
              {/if}
            </div>

            <button
              type="button"
              onclick={() => removeDomainRule(rule.domain)}
              class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] hover:opacity-80"
              style="color: var(--danger);"
              title={t.proxyRemoveRule}
            >
              <Trash2 size={12} />
              <span>{t.proxyRemoveRule}</span>
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Add Rule Form -->
    <div class="flex flex-col gap-2">
      <div class="flex flex-wrap items-center gap-2">
        <input
          type="text"
          bind:value={newDomain}
          placeholder={t.proxyDomainPlaceholder}
          class="field min-w-40 flex-1"
        />
        <select bind:value={newAction} class="field">
          <option value="force_proxy">{t.proxyRuleForceProxy}</option>
          <option value="force_direct">{t.proxyRuleForceDirect}</option>
          <option value="custom_proxy">{t.proxyRuleCustomProxy}</option>
        </select>
        <button
          type="button"
          onclick={addDomainRule}
          disabled={!newDomain.trim()}
          class="inline-flex items-center gap-1 rounded-lg border px-3 py-1.5 text-xs font-medium disabled:opacity-45"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <Plus size={13} />
          <span>{t.proxyAddRule}</span>
        </button>
      </div>

      {#if newAction === "custom_proxy"}
        <div
          class="grid grid-cols-1 gap-2 rounded-lg border p-2.5 sm:grid-cols-12"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <select bind:value={customProtocol} class="field sm:col-span-3">
            <option value="socks5">SOCKS5</option>
            <option value="http">HTTP</option>
            <option value="https">HTTPS</option>
          </select>
          <input
            type="text"
            bind:value={customHost}
            placeholder={t.proxyHostLabel}
            class="field sm:col-span-6"
          />
          <input
            type="number"
            min="1"
            max="65535"
            bind:value={customPort}
            placeholder={t.proxyPortLabel}
            class="field sm:col-span-3"
          />
          <input
            type="text"
            bind:value={customUsername}
            placeholder={t.proxyUsernameLabel}
            class="field sm:col-span-6"
          />
          <input
            type="password"
            bind:value={customPassword}
            placeholder={t.proxyPasswordLabel}
            class="field sm:col-span-6"
          />
        </div>
      {/if}
    </div>
  </div>
</section>

<style>
  .settings-card {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    padding: 1rem 1.15rem;
    border-radius: 1rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
  }

  .section-title {
    font-size: 0.9rem;
    font-weight: 600;
    margin: 0;
  }

  .label {
    font-size: 0.875rem;
    font-weight: 500;
  }

  .pill {
    padding: 0.35rem 0.75rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
  }

  .pill-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .field {
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
    color: var(--text);
    padding: 0.4rem 0.65rem;
    font-size: 0.75rem;
    outline: none;
  }

  .field:focus {
    border-color: var(--accent);
  }
</style>
