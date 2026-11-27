<script lang="ts">
  let {
    size = 32,
    class: className = "",
  }: {
    size?: number;
    class?: string;
  } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 100 100"
  fill="none"
  xmlns="http://www.w3.org/2000/svg"
  role="img"
  aria-label="RtMaga"
  class="rtmaga-logo {className}"
>
  <!-- Theme-adaptive squircle container -->
  <rect
    class="rtmaga-logo-bg"
    x="3"
    y="3"
    width="94"
    height="94"
    rx="24"
  />
  <!-- Subtle top specular highlight rim -->
  <rect
    class="rtmaga-logo-rim"
    x="4.25"
    y="4.25"
    width="91.5"
    height="91.5"
    rx="22.75"
    stroke-width="1.5"
  />

  <!-- Left & Right open-book paper shell (iconic RtMaga mark) -->
  <path
    class="rtmaga-logo-paper"
    d="M21 31.8C21 28.4 24.7 26.4 27.6 28.2L50 41.8L72.4 28.2C75.3 26.4 79 28.4 79 31.8V61.2C79 61.9 78.5 62.5 77.8 62.7C69.1 64.9 60.4 68.7 52.8 73.7V53.6L50 51.9L47.2 53.6V73.7C39.6 68.7 30.9 64.9 22.2 62.7C21.5 62.5 21 61.9 21 61.2V31.8Z"
  />

  <!-- Central vertical spine ink cut + upper-right inner page crease -->
  <path
    class="rtmaga-logo-ink"
    d="M49.5 52.7L75.6 37.6V41.5L53.4 54.4V73.7H49.5V52.7Z"
  />

  <!-- Inner-right dynamic accent page panel -->
  <path
    class="rtmaga-logo-accent"
    d="M53.4 54.4L75.6 41.5V60.2C68.1 62.5 60.6 66.2 53.4 70.9V54.4Z"
  />
</svg>

<style>
  .rtmaga-logo {
    display: inline-block;
    flex-shrink: 0;
    border-radius: 24%;
    overflow: visible;
    filter: drop-shadow(0 2px 4px rgba(22, 22, 26, 0.14));
  }

  .rtmaga-logo-bg {
    fill: var(--text, #16161a);
  }

  .rtmaga-logo-rim {
    stroke: rgba(255, 255, 255, 0.14);
  }

  .rtmaga-logo-paper {
    fill: var(--surface, #ffffff);
  }

  .rtmaga-logo-ink {
    fill: var(--text, #16161a);
  }

  .rtmaga-logo-accent {
    fill: var(--accent, #e11d48);
  }

  :global(:root[data-theme="dark"]) .rtmaga-logo {
    filter: drop-shadow(0 3px 8px rgba(0, 0, 0, 0.45));
  }

  :global(:root[data-theme="dark"]) .rtmaga-logo-bg {
    fill: var(--surface-elevated, #1a1a22);
  }

  :global(:root[data-theme="dark"]) .rtmaga-logo-rim {
    stroke: var(--border-strong, #353546);
  }

  :global(:root[data-theme="dark"]) .rtmaga-logo-paper {
    fill: var(--text, #f3f3f6);
  }

  :global(:root[data-theme="dark"]) .rtmaga-logo-ink {
    fill: var(--bg, #0b0b0f);
  }

  :global(:root[data-theme="dark"]) .rtmaga-logo-accent {
    fill: var(--accent, #f43f5e);
  }
</style>
