<script lang="ts">
  let {
    checked = false,
    disabled = false,
    ariaLabel = "",
    size = "md",
    onchange,
  }: {
    checked: boolean;
    disabled?: boolean;
    ariaLabel?: string;
    size?: "sm" | "md";
    onchange: (checked: boolean) => void;
  } = $props();

  let squishDir = $state<"none" | "on" | "off">("none");
  let squishTimer: number | null = null;

  function triggerToggle() {
    if (disabled) return;
    const next = !checked;
    squishDir = next ? "on" : "off";
    if (squishTimer !== null) window.clearTimeout(squishTimer);
    squishTimer = window.setTimeout(() => {
      squishDir = "none";
      squishTimer = null;
    }, 340);
    onchange(next);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (disabled) return;
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      triggerToggle();
    }
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel || undefined}
  {disabled}
  onclick={triggerToggle}
  onkeydown={handleKeyDown}
  class="squish-track {size === 'sm' ? 'squish-sm' : 'squish-md'}"
  class:squish-checked={checked}
  class:squish-anim-on={squishDir === "on"}
  class:squish-anim-off={squishDir === "off"}
>
  <span class="squish-thumb" aria-hidden="true"></span>
</button>

<style>
  .squish-track {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    border-radius: 9999px;
    border: 1px solid var(--border-strong);
    background-color: var(--surface-subtle);
    padding: 2px;
    cursor: pointer;
    user-select: none;
    -webkit-tap-highlight-color: transparent;
    transition:
      background-color 200ms var(--ease-out, cubic-bezier(0.22, 1, 0.36, 1)),
      border-color 200ms var(--ease-out, cubic-bezier(0.22, 1, 0.36, 1)),
      transform 150ms var(--ease-spring, cubic-bezier(0.34, 1.4, 0.64, 1));
  }

  .squish-track:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .squish-track:active:not(:disabled) {
    transform: scale(0.95, 1.03);
  }

  .squish-md {
    --track-w: 42px;
    --track-h: 24px;
    --thumb-size: 18px;
    --thumb-travel: 18px;
    width: var(--track-w);
    height: var(--track-h);
  }

  .squish-sm {
    --track-w: 36px;
    --track-h: 20px;
    --thumb-size: 14px;
    --thumb-travel: 16px;
    width: var(--track-w);
    height: var(--track-h);
  }

  .squish-track.squish-checked {
    background-color: var(--accent);
    border-color: var(--accent);
  }

  .squish-track:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .squish-thumb {
    display: block;
    width: var(--thumb-size);
    height: var(--thumb-size);
    border-radius: 9999px;
    background-color: #ffffff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.24);
    transform: translate3d(0, 0, 0) scale(1, 1);
    transform-origin: left center;
    will-change: transform;
    transition:
      transform 280ms var(--ease-spring, cubic-bezier(0.34, 1.4, 0.64, 1)),
      opacity 160ms ease;
  }

  .squish-track.squish-checked .squish-thumb {
    background-color: var(--accent-fg);
    transform-origin: right center;
    transform: translate3d(var(--thumb-travel), 0, 0) scale(1, 1);
  }

  /* Pure GPU tactile press-down squish (transform only — zero layout/width changes) */
  .squish-track:active:not(:disabled):not(.squish-checked) .squish-thumb {
    transform: translate3d(0, 0, 0) scale(1.26, 0.88);
  }

  .squish-track:active:not(:disabled).squish-checked .squish-thumb {
    transform: translate3d(var(--thumb-travel), 0, 0) scale(1.26, 0.88);
  }

  /* Elastic jelly-squish travel keyframes on state flip (GPU transform only) */
  .squish-anim-on .squish-thumb {
    transform-origin: center center;
    animation: thumb-squish-to-on 300ms cubic-bezier(0.34, 1.42, 0.64, 1);
  }

  .squish-anim-off .squish-thumb {
    transform-origin: center center;
    animation: thumb-squish-to-off 300ms cubic-bezier(0.34, 1.42, 0.64, 1);
  }

  @keyframes thumb-squish-to-on {
    0% {
      transform: translate3d(0, 0, 0) scale(1, 1);
    }
    35% {
      transform: translate3d(calc(var(--thumb-travel) * 0.55), 0, 0) scale(1.26, 0.84);
    }
    70% {
      transform: translate3d(calc(var(--thumb-travel) * 1.04), 0, 0) scale(0.92, 1.08);
    }
    100% {
      transform: translate3d(var(--thumb-travel), 0, 0) scale(1, 1);
    }
  }

  @keyframes thumb-squish-to-off {
    0% {
      transform: translate3d(var(--thumb-travel), 0, 0) scale(1, 1);
    }
    35% {
      transform: translate3d(calc(var(--thumb-travel) * 0.45), 0, 0) scale(1.26, 0.84);
    }
    70% {
      transform: translate3d(calc(var(--thumb-travel) * -0.04), 0, 0) scale(0.92, 1.08);
    }
    100% {
      transform: translate3d(0, 0, 0) scale(1, 1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .squish-track,
    .squish-thumb {
      transition: opacity 80ms ease;
    }
    .squish-anim-on .squish-thumb,
    .squish-anim-off .squish-thumb {
      animation: none;
    }
  }
</style>
