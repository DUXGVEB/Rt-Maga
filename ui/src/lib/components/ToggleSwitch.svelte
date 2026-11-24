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
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel || undefined}
  {disabled}
  onclick={() => {
    if (!disabled) onchange(!checked);
  }}
  class="toggle-track {size === 'sm' ? 'toggle-sm' : 'toggle-md'}"
  class:toggle-on={checked}
>
  <span class="toggle-thumb"></span>
</button>

<style>
  .toggle-track {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    border-radius: 9999px;
    border: 1px solid var(--border);
    background-color: var(--surface-hover);
    padding: 2px;
    cursor: pointer;
    transition: background-color 150ms ease, border-color 150ms ease;
  }

  .toggle-md {
    width: 38px;
    height: 22px;
  }

  .toggle-sm {
    width: 32px;
    height: 18px;
  }

  .toggle-track.toggle-on {
    background-color: var(--accent);
    border-color: var(--accent);
  }

  .toggle-track.toggle-on .toggle-thumb {
    background-color: var(--accent-fg);
  }

  .toggle-track:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .toggle-thumb {
    border-radius: 9999px;
    background-color: #ffffff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.28);
    transition: transform 150ms ease;
  }

  .toggle-md .toggle-thumb {
    width: 16px;
    height: 16px;
    transform: translateX(0);
  }

  .toggle-md.toggle-on .toggle-thumb {
    transform: translateX(16px);
  }

  .toggle-sm .toggle-thumb {
    width: 12px;
    height: 12px;
    transform: translateX(0);
  }

  .toggle-sm.toggle-on .toggle-thumb {
    transform: translateX(14px);
  }
</style>
