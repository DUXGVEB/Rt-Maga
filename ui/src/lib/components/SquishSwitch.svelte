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
    border: 1px solid var(--border);
    background-color: var(--surface-hover);
    padding: 2px;
    cursor: pointer;
    user-select: none;
    -webkit-tap-highlight-color: transparent;
    transition:
      background-color 220ms cubic-bezier(0.22, 1, 0.36, 1),
      border-color 220ms cubic-bezier(0.22, 1, 0.36, 1),
      transform 160ms cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  .squish-track:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .squish-track:active:not(:disabled) {
    transform: scale(0.95, 1.03);
  }

  .squish-md {
    --track-w: 40px;
    --track-h: 22px;
    --thumb-size: 16px;
    --thumb-squish-w: 21px;
    --thumb-travel: 18px;
    width: var(--track-w);
    height: var(--track-h);
  }

  .squish-sm {
    --track-w: 34px;
    --track-h: 18px;
    --thumb-size: 12px;
    --thumb-squish-w: 16px;
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
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.24);
    transform: translateX(0) scale(1, 1);
    transform-origin: center center;
    will-change: transform, width;
    transition:
      transform 300ms cubic-bezier(0.34, 1.56, 0.64, 1),
      width 150ms cubic-bezier(0.22, 1, 0.36, 1),
      background-color 180ms ease;
  }

  .squish-track.squish-checked .squish-thumb {
    background-color: var(--accent-fg);
    transform: translateX(var(--thumb-travel)) scale(1, 1);
  }

  /* Tactile press-down squish before release */
  .squish-track:active:not(:disabled):not(.squish-checked) .squish-thumb {
    width: var(--thumb-squish-w);
    transform: translateX(0) scaleY(0.92);
  }

  .squish-track:active:not(:disabled).squish-checked .squish-thumb {
    width: var(--thumb-squish-w);
    transform: translateX(calc(var(--thumb-travel) - (var(--thumb-squish-w) - var(--thumb-size))))
      scaleY(0.92);
  }

  /* Elastic jelly-squish travel keyframes on state flip */
  .squish-anim-on .squish-thumb {
    animation: thumb-squish-to-on 320ms cubic-bezier(0.34, 1.48, 0.64, 1);
  }

  .squish-anim-off .squish-thumb {
    animation: thumb-squish-to-off 320ms cubic-bezier(0.34, 1.48, 0.64, 1);
  }

  @keyframes thumb-squish-to-on {
    0% {
      transform: translateX(0) scale(1, 1);
    }
    35% {
      transform: translateX(calc(var(--thumb-travel) * 0.55)) scale(1.26, 0.84);
    }
    70% {
      transform: translateX(calc(var(--thumb-travel) * 1.04)) scale(0.92, 1.08);
    }
    100% {
      transform: translateX(var(--thumb-travel)) scale(1, 1);
    }
  }

  @keyframes thumb-squish-to-off {
    0% {
      transform: translateX(var(--thumb-travel)) scale(1, 1);
    }
    35% {
      transform: translateX(calc(var(--thumb-travel) * 0.45)) scale(1.26, 0.84);
    }
    70% {
      transform: translateX(calc(var(--thumb-travel) * -0.04)) scale(0.92, 1.08);
    }
    100% {
      transform: translateX(0) scale(1, 1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .squish-track,
    .squish-thumb {
      transition: background-color 120ms ease, transform 120ms ease;
    }
    .squish-anim-on .squish-thumb,
    .squish-anim-off .squish-thumb {
      animation: none;
    }
  }
</style>
