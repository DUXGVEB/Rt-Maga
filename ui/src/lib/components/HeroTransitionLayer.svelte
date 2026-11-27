<script lang="ts">
  import { onDestroy } from "svelte";
  import { imgProxyUrl } from "../api";
  import { MOTION_DURATION, MOTION_EASING, prefersReducedMotion } from "../motion";
  import { heroTransitionStore, type HeroFlightState } from "../stores/heroTransition.svelte";

  let ghostEl = $state<HTMLDivElement | null>(null);
  let activeAnim: Animation | null = null;
  let searchRaf: number | null = null;
  let handledFlightId = -1;

  function cancelActiveMotion() {
    if (searchRaf !== null) {
      cancelAnimationFrame(searchRaf);
      searchRaf = null;
    }
    if (activeAnim) {
      try {
        activeAnim.cancel();
      } catch {
        // Ignore already finished animations.
      }
      activeAnim = null;
    }
  }

  function settledRect(el: HTMLElement): DOMRect {
    const rect = el.getBoundingClientRect();
    let offsetX = 0;
    let offsetY = 0;
    let cur: HTMLElement | null = el;
    while (cur && cur !== document.body) {
      const tf = window.getComputedStyle(cur).transform;
      if (tf && tf !== "none") {
        try {
          const m = new DOMMatrixReadOnly(tf);
          offsetX += m.m41;
          offsetY += m.m42;
        } catch {
          // Ignore unsupported transform matrices.
        }
      }
      cur = cur.parentElement;
    }
    return new DOMRect(rect.left - offsetX, rect.top - offsetY, rect.width, rect.height);
  }

  function startFlight(flight: HeroFlightState, node: HTMLDivElement) {
    if (prefersReducedMotion() || typeof node.animate !== "function") {
      heroTransitionStore.finish(flight.id);
      return;
    }

    const selector = `[data-hero-cover-target="manga-${flight.mangaId}"]`;
    let attempts = 0;
    const maxAttempts = 16;

    const locateAndFly = () => {
      searchRaf = null;
      if (heroTransitionStore.active?.id !== flight.id) return;

      const target = document.querySelector<HTMLElement>(selector);
      const targetRect = target ? settledRect(target) : null;

      if (target && targetRect && targetRect.width >= 12 && targetRect.height >= 12) {
        heroTransitionStore.markFlying(flight.id);

        const targetRadius = window.getComputedStyle(target).borderRadius || "12px";
        const dx = flight.from.left - targetRect.left;
        const dy = flight.from.top - targetRect.top;
        const sx = Math.max(0.1, flight.from.width / targetRect.width);
        const sy = Math.max(0.1, flight.from.height / targetRect.height);

        // Lock layout box to exact destination rect and animate ONLY GPU-composited transform + opacity.
        node.style.left = `${targetRect.left.toFixed(2)}px`;
        node.style.top = `${targetRect.top.toFixed(2)}px`;
        node.style.width = `${targetRect.width.toFixed(2)}px`;
        node.style.height = `${targetRect.height.toFixed(2)}px`;
        node.style.borderRadius = targetRadius;
        node.style.transformOrigin = "top left";

        const anim = node.animate(
          [
            {
              transform: `translate3d(${dx.toFixed(2)}px, ${dy.toFixed(2)}px, 0) scale(${sx.toFixed(4)}, ${sy.toFixed(4)})`,
              opacity: 1,
            },
            {
              transform: "translate3d(0px, 0px, 0) scale(1, 1)",
              opacity: 1,
            },
          ],
          {
            duration: MOTION_DURATION.heroFlight,
            easing: MOTION_EASING.heroCss,
            fill: "forwards",
          }
        );

        activeAnim = anim;
        const done = () => {
          if (activeAnim === anim) activeAnim = null;
          heroTransitionStore.finish(flight.id);
        };
        anim.onfinish = done;
        anim.oncancel = done;
        return;
      }

      attempts += 1;
      if (attempts < maxAttempts) {
        searchRaf = requestAnimationFrame(locateAndFly);
        return;
      }

      // Target did not mount in time (e.g. navigation aborted or error state): fade out cleanly.
      const fadeAnim = node.animate(
        [
          { opacity: 1, transform: "scale(1)" },
          { opacity: 0, transform: "scale(0.96)" },
        ],
        { duration: 110, easing: MOTION_EASING.snappyCss, fill: "forwards" }
      );
      activeAnim = fadeAnim;
      const done = () => {
        if (activeAnim === fadeAnim) activeAnim = null;
        heroTransitionStore.finish(flight.id);
      };
      fadeAnim.onfinish = done;
      fadeAnim.oncancel = done;
    };

    searchRaf = requestAnimationFrame(locateAndFly);
  }

  $effect(() => {
    const flight = heroTransitionStore.active;
    const node = ghostEl;
    if (!flight || !node) {
      cancelActiveMotion();
      if (!flight) handledFlightId = -1;
      return;
    }
    if (flight.id === handledFlightId) return;
    handledFlightId = flight.id;
    cancelActiveMotion();
    startFlight(flight, node);
  });

  onDestroy(() => {
    cancelActiveMotion();
  });
</script>

{#if heroTransitionStore.active}
  {@const flight = heroTransitionStore.active}
  <div class="hero-flight-layer" aria-hidden="true">
    <div
      bind:this={ghostEl}
      class="hero-flight-card"
      style="left: {flight.from.left}px; top: {flight.from.top}px; width: {flight.from.width}px; height: {flight.from.height}px; border-radius: {flight.from.borderRadius};"
    >
      <img
        src={imgProxyUrl(flight.coverToken)}
        alt=""
        decoding="sync"
        draggable="false"
        class="h-full w-full object-cover"
      />
    </div>
  </div>
{/if}

<style>
  .hero-flight-layer {
    position: fixed;
    inset: 0;
    z-index: 70;
    pointer-events: none;
    overflow: hidden;
  }

  .hero-flight-card {
    position: fixed;
    overflow: hidden;
    background-color: var(--surface-subtle);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-lg);
    will-change: transform, opacity;
  }
</style>
