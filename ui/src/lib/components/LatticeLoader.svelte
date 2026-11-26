<script lang="ts">
  import { AlertCircle, Check } from "lucide-svelte";

  export type LatticeStatus = "loading" | "success" | "error";
  export type LatticeSize = "xs" | "sm" | "md" | "lg";

  let {
    status = "loading",
    size = "md",
    label = "",
    showTimer = false,
    inline = false,
    tone = "accent",
  }: {
    status?: LatticeStatus;
    size?: LatticeSize;
    label?: string;
    showTimer?: boolean;
    inline?: boolean;
    tone?: "accent" | "inherit";
  } = $props();

  let elapsedMs = $state(0);

  $effect(() => {
    if (!showTimer || status !== "loading") {
      if (status === "loading") elapsedMs = 0;
      return;
    }
    const started = performance.now();
    elapsedMs = 0;
    const id = window.setInterval(() => {
      elapsedMs = performance.now() - started;
    }, 200);
    return () => window.clearInterval(id);
  });

  const formattedTime = $derived(() => {
    const secs = elapsedMs / 1000;
    return secs < 10 ? `${secs.toFixed(1)}s` : `${Math.floor(secs)}s`;
  });

  const pxSize = $derived(
    size === "xs" ? 13 : size === "sm" ? 16 : size === "lg" ? 30 : 22
  );

  // 3x3 orbit order around the perimeter (clockwise); center cell (index 4) gently breathes.
  const ORBIT_STEP: readonly (number | null)[] = [
    0, 1, 2,
    7, null, 3,
    6, 5, 4,
  ];
</script>

<span
  class="lattice-wrap"
  class:lattice-inline={inline}
  class:lattice-stack={!inline && (Boolean(label) || showTimer)}
  role="status"
  aria-live="polite"
  aria-busy={status === "loading"}
>
  {#if status === "loading"}
    <span
      class="lattice-grid lattice-{size}"
      class:lattice-tone-accent={tone === "accent"}
      class:lattice-tone-inherit={tone === "inherit"}
      aria-hidden="true"
    >
      {#each ORBIT_STEP as step, idx (idx)}
        {#if step === null}
          <span class="lattice-cell lattice-center"></span>
        {:else}
          <span class="lattice-cell lattice-orbit" style="--step: {step};"></span>
        {/if}
      {/each}
    </span>
  {:else if status === "success"}
    <span class="lattice-state lattice-state-ok" aria-hidden="true">
      <Check size={pxSize} strokeWidth={2.5} />
    </span>
  {:else}
    <span class="lattice-state lattice-state-err" aria-hidden="true">
      <AlertCircle size={pxSize} strokeWidth={2.2} />
    </span>
  {/if}

  {#if label || (showTimer && status === "loading" && elapsedMs >= 400)}
    <span class="lattice-meta" class:lattice-meta-inline={inline}>
      {#if label}
        <span class="lattice-label">{label}</span>
      {/if}
      {#if showTimer && status === "loading" && elapsedMs >= 400}
        <span class="lattice-timer">{formattedTime()}</span>
      {/if}
    </span>
  {/if}
</span>

<style>
  .lattice-wrap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.45rem;
  }

  .lattice-stack {
    flex-direction: column;
    gap: 0.55rem;
  }

  .lattice-inline {
    flex-direction: row;
    gap: 0.4rem;
  }

  .lattice-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    grid-template-rows: repeat(3, 1fr);
    flex-shrink: 0;
  }

  .lattice-tone-accent {
    color: var(--accent);
  }

  .lattice-tone-inherit {
    color: currentColor;
  }

  .lattice-xs {
    width: 13px;
    height: 13px;
    gap: 1.5px;
  }

  .lattice-sm {
    width: 16px;
    height: 16px;
    gap: 2px;
  }

  .lattice-md {
    width: 22px;
    height: 22px;
    gap: 2.5px;
  }

  .lattice-lg {
    width: 30px;
    height: 30px;
    gap: 3px;
  }

  .lattice-cell {
    display: block;
    width: 100%;
    height: 100%;
    border-radius: 1.5px;
    background-color: currentColor;
    will-change: transform, opacity;
  }

  .lattice-orbit {
    animation: lattice-orbit-wave 960ms cubic-bezier(0.4, 0, 0.2, 1) infinite;
    animation-delay: calc(var(--step) * -120ms);
  }

  .lattice-center {
    animation: lattice-center-breathe 1920ms ease-in-out infinite;
  }

  @keyframes lattice-orbit-wave {
    0%,
    100% {
      opacity: 0.96;
      transform: scale(1);
    }
    40% {
      opacity: 0.22;
      transform: scale(0.68);
    }
    72% {
      opacity: 0.34;
      transform: scale(0.76);
    }
  }

  @keyframes lattice-center-breathe {
    0%,
    100% {
      opacity: 0.32;
      transform: scale(0.72);
    }
    50% {
      opacity: 0.56;
      transform: scale(0.86);
    }
  }

  .lattice-state {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    animation: lattice-pop 200ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
  }

  .lattice-state-ok {
    color: #10b981;
  }

  .lattice-state-err {
    color: var(--danger);
  }

  @keyframes lattice-pop {
    0% {
      opacity: 0;
      transform: scale(0.72);
    }
    100% {
      opacity: 1;
      transform: scale(1);
    }
  }

  .lattice-meta {
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    gap: 0.1rem;
    line-height: 1.25;
  }

  .lattice-meta-inline {
    flex-direction: row;
    gap: 0.35rem;
  }

  .lattice-label {
    font-size: inherit;
    font-weight: 500;
  }

  .lattice-timer {
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    opacity: 0.72;
  }

  @media (prefers-reduced-motion: reduce) {
    .lattice-orbit,
    .lattice-center {
      animation: none;
    }
    .lattice-orbit {
      opacity: 0.78;
      transform: scale(0.9);
    }
    .lattice-center {
      opacity: 0.4;
      transform: scale(0.8);
    }
    .lattice-state {
      animation: none;
    }
  }
</style>
