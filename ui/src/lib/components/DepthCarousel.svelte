<script lang="ts">
  import { BookOpen, CheckCircle2, ChevronLeft, ChevronRight, Sparkles } from "lucide-svelte";
  import { onDestroy, onMount } from "svelte";
  import { imgProxyUrl } from "../api";
  import {
    computeCard3DState,
    computeSpinPlan,
    computeStageScale,
    DEFAULT_DEPTH_CAROUSEL_CONFIG,
    easeSpinOut,
    stepCriticallyDampedSpring,
    wrapIndex,
    type Card3DState,
    type DepthCarouselConfig,
    type RandomMangaItem,
    type SpinPlan,
    type TiltDirection,
  } from "../depthCarouselMath";
  import { prefersReducedMotion, pulseHighlight } from "../motion";
  import { settingsStore } from "../stores/settings.svelte";

  let {
    items,
    depth = DEFAULT_DEPTH_CAROUSEL_CONFIG.depth,
    spread = DEFAULT_DEPTH_CAROUSEL_CONFIG.spread,
    tilt = DEFAULT_DEPTH_CAROUSEL_CONFIG.tilt,
    tiltDirection = DEFAULT_DEPTH_CAROUSEL_CONFIG.tiltDirection,
    perspective = DEFAULT_DEPTH_CAROUSEL_CONFIG.perspective,
    visibleCards = DEFAULT_DEPTH_CAROUSEL_CONFIG.visibleCards,
    falloff = DEFAULT_DEPTH_CAROUSEL_CONFIG.falloff,
    loop = DEFAULT_DEPTH_CAROUSEL_CONFIG.loop,
    autoplay = DEFAULT_DEPTH_CAROUSEL_CONFIG.autoplay,
    keyboardEnabled = true,
    onActiveChange,
    onSelectStory,
    onSpinStart,
    onSpinEnd,
  }: {
    items: RandomMangaItem[];
    depth?: number;
    spread?: number;
    tilt?: number;
    tiltDirection?: TiltDirection;
    perspective?: number;
    visibleCards?: number;
    falloff?: number;
    loop?: boolean;
    autoplay?: boolean;
    keyboardEnabled?: boolean;
    onActiveChange?: (item: RandomMangaItem | null, index: number) => void;
    onSelectStory?: (item: RandomMangaItem) => void;
    onSpinStart?: () => void;
    onSpinEnd?: (item: RandomMangaItem, index: number) => void;
  } = $props();

  const t = $derived(settingsStore.t);

  // Discrete UI state ONLY — never mutated inside rAF frames!
  let isSpinning = $state(false);
  let activeIndex = $state(0);
  let failedCovers = $state<Record<number, boolean>>({});

  // Non-reactive hot-path engine state (zero Svelte reactivity per frame)
  let stageEl: HTMLDivElement | null = null;
  let cardEls: (HTMLDivElement | null)[] = [];
  let frameEls: (HTMLDivElement | null)[] = [];
  let shadeEls: (HTMLSpanElement | null)[] = [];
  let resizeObserver: ResizeObserver | null = null;

  let virtualPos = 0;
  let targetPos = 0;
  let velocity = 0;
  let mode: "idle" | "drag" | "settle" | "spin" = "idle";
  let stageScale = 1;
  let rafId: number | null = null;
  let lastFrameTs = 0;
  let willChangeActive = false;
  let lastReportedIdx = -1;
  let lastDeckSignature = "";

  // Spin parameters
  let activeSpin: SpinPlan | null = null;
  let spinElapsedSec = 0;

  // Pointer & touch gesture state
  let activePointerId: number | null = null;
  let dragIntent: "none" | "pending" | "dragging" = "none";
  let dragStartX = 0;
  let dragStartY = 0;
  let dragStartPos = 0;
  let dragLastX = 0;
  let dragLastTs = 0;
  let dragVel = 0;
  let suppressClickUntil = 0;

  // Wheel accumulation state
  let wheelAccum = 0;
  let lastWheelStepAt = 0;

  // WAAPI highlight cleanup handle
  let cancelHighlightPulse: () => void = () => {};

  // Single reusable scratch struct — zero GC allocations inside rAF!
  const scratch: Card3DState = {
    tx: 0,
    tz: 0,
    rotY: 0,
    scale: 1,
    opacity: 1,
    shadeOpacity: 0,
    zIndex: 100,
    visible: true,
    isCenter: true,
    absRel: 0,
  };

  function currentConfig(): DepthCarouselConfig {
    return {
      depth,
      spread,
      tilt,
      tiltDirection,
      perspective,
      visibleCards,
      falloff,
      loop,
      autoplay,
    };
  }

  function setCardsWillChange(active: boolean) {
    if (active === willChangeActive) return;
    willChangeActive = active;
    for (let i = 0; i < cardEls.length; i += 1) {
      const el = cardEls[i];
      if (!el) continue;
      if (active) {
        if (el.style.visibility !== "hidden") {
          el.style.willChange = "transform, opacity";
        }
      } else {
        el.style.willChange = "";
      }
    }
  }

  function applyCardTransform(i: number, pos: number, count: number, cfg: DepthCarouselConfig) {
    const el = cardEls[i];
    if (!el) return;
    const state = computeCard3DState(i, pos, count, cfg, stageScale, scratch);

    if (!state.visible || state.opacity <= 0.01) {
      if (el.style.visibility !== "hidden") {
        el.style.visibility = "hidden";
        el.style.opacity = "0";
        el.style.pointerEvents = "none";
        el.style.willChange = "";
        el.dataset.active = "false";
      }
      return;
    }

    if (el.style.visibility !== "visible") {
      el.style.visibility = "visible";
      if (willChangeActive) {
        el.style.willChange = "transform, opacity";
      }
    }

    el.style.transform = `translate3d(${state.tx.toFixed(2)}px, 0px, ${state.tz.toFixed(2)}px) rotateY(${state.rotY.toFixed(2)}deg) scale(${state.scale.toFixed(4)})`;
    el.style.opacity = state.opacity.toFixed(3);
    el.style.zIndex = String(state.zIndex);
    el.style.pointerEvents = mode === "spin" ? "none" : "auto";

    const nextActiveAttr = state.isCenter ? "true" : "false";
    if (el.dataset.active !== nextActiveAttr) {
      el.dataset.active = nextActiveAttr;
    }

    const shade = shadeEls[i];
    if (shade) {
      shade.style.opacity = state.shadeOpacity.toFixed(3);
    }
  }

  function applyAllTransforms(pos: number) {
    const count = items.length;
    if (count === 0) return;
    const cfg = currentConfig();
    for (let i = 0; i < count; i += 1) {
      applyCardTransform(i, pos, count, cfg);
    }
  }

  function reportActiveIfChanged(index: number, force = false) {
    const count = items.length;
    if (count === 0) {
      lastReportedIdx = -1;
      activeIndex = 0;
      onActiveChange?.(null, -1);
      return;
    }
    const wrapped = wrapIndex(index, count);
    if (!force && wrapped === lastReportedIdx) return;
    lastReportedIdx = wrapped;
    activeIndex = wrapped;
    onActiveChange?.(items[wrapped] ?? null, wrapped);
  }

  function stopLoop() {
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
  }

  function startLoop() {
    if (rafId !== null) return;
    lastFrameTs = performance.now();
    rafId = requestAnimationFrame(tick);
  }

  function finishSpin(targetIdx: number) {
    const count = items.length;
    stopLoop();
    activeSpin = null;
    spinElapsedSec = 0;
    mode = "idle";
    virtualPos = count > 0 ? wrapIndex(targetIdx, count) : 0;
    targetPos = virtualPos;
    velocity = 0;
    applyAllTransforms(virtualPos);
    setCardsWillChange(false);
    isSpinning = false;

    if (count > 0) {
      const winnerIdx = wrapIndex(targetIdx, count);
      reportActiveIfChanged(winnerIdx, true);
      cancelHighlightPulse();
      cancelHighlightPulse = pulseHighlight(frameEls[winnerIdx] ?? null);
      const winner = items[winnerIdx];
      if (winner) {
        onSpinEnd?.(winner, winnerIdx);
      }
    }
  }

  function tick(now: number) {
    rafId = null;
    const count = items.length;
    if (count === 0) {
      mode = "idle";
      setCardsWillChange(false);
      isSpinning = false;
      return;
    }

    const dt = Math.min(0.05, Math.max(0.001, (now - lastFrameTs) / 1000));
    lastFrameTs = now;

    if (mode === "spin" && activeSpin) {
      spinElapsedSec += dt;
      const progress =
        activeSpin.durationSec > 0 ? Math.min(1, spinElapsedSec / activeSpin.durationSec) : 1;
      const eased = easeSpinOut(progress);
      virtualPos = activeSpin.startPos + (activeSpin.endPos - activeSpin.startPos) * eased;
      applyAllTransforms(virtualPos);

      if (progress >= 1) {
        finishSpin(activeSpin.targetIndex);
        return;
      }
      rafId = requestAnimationFrame(tick);
      return;
    }

    if (mode === "settle") {
      // Exact analytical critically damped spring (omega = 15 rad/s -> crisp ~240ms settle)
      const next = stepCriticallyDampedSpring(virtualPos, targetPos, velocity, 15, dt);
      virtualPos = next.pos;
      velocity = next.vel;

      const settled = Math.abs(virtualPos - targetPos) < 0.0015 && Math.abs(velocity) < 0.015;
      if (settled) {
        virtualPos = loop ? wrapIndex(Math.round(targetPos), count) : Math.max(0, Math.min(count - 1, Math.round(targetPos)));
        targetPos = virtualPos;
        velocity = 0;
        mode = "idle";
        applyAllTransforms(virtualPos);
        setCardsWillChange(false);
        reportActiveIfChanged(virtualPos);
        return;
      }

      applyAllTransforms(virtualPos);
      // Discrete center update while settling (only when nearest index changes)
      const nearest = loop
        ? wrapIndex(Math.round(virtualPos), count)
        : Math.max(0, Math.min(count - 1, Math.round(virtualPos)));
      if (nearest !== lastReportedIdx) {
        reportActiveIfChanged(nearest);
      }
      rafId = requestAnimationFrame(tick);
      return;
    }

    if (mode === "drag") {
      applyAllTransforms(virtualPos);
      return;
    }

    setCardsWillChange(false);
  }

  function updateStageMetrics() {
    if (!stageEl) return;
    const rect = stageEl.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;
    stageScale = computeStageScale(rect.width, rect.height);
    const cardW = Math.round(260 * stageScale);
    const cardH = Math.round(390 * stageScale);
    stageEl.style.setProperty("--dc-card-w", `${cardW}px`);
    stageEl.style.setProperty("--dc-card-h", `${cardH}px`);
    applyAllTransforms(virtualPos);
  }

  function clampNonLoopTarget(pos: number): number {
    if (loop) return pos;
    return Math.max(0, Math.min(Math.max(0, items.length - 1), pos));
  }

  /**
   * Exposed method: spins through 2–3 full revolutions with `expo.out`/`power4.out` easing
   * onto a uniformly random manga (excluding the immediately previous active manga).
   */
  export function spinToRandom(excludeId?: number | null): RandomMangaItem | null {
    const count = items.length;
    if (isSpinning || count === 0) return null;

    cancelHighlightPulse();
    stopLoop();

    const itemIds = items.map((item) => item.id);
    const plan = computeSpinPlan({
      currentPos: virtualPos,
      count,
      excludeId: excludeId ?? null,
      itemIds,
    });
    if (!plan) return null;

    const chosen = items[plan.targetIndex] ?? null;
    if (!chosen) return null;

    // Single card or reduced-motion: skip multi-loop spinning and snap directly.
    if (count === 1 || prefersReducedMotion()) {
      virtualPos = plan.targetIndex;
      targetPos = plan.targetIndex;
      velocity = 0;
      mode = "idle";
      setCardsWillChange(false);
      applyAllTransforms(virtualPos);
      reportActiveIfChanged(plan.targetIndex, true);
      if (!prefersReducedMotion()) {
        cancelHighlightPulse = pulseHighlight(frameEls[plan.targetIndex] ?? null);
      }
      onSpinEnd?.(chosen, plan.targetIndex);
      return chosen;
    }

    isSpinning = true;
    activeSpin = plan;
    spinElapsedSec = 0;
    virtualPos = plan.startPos;
    targetPos = plan.endPos;
    velocity = 0;
    mode = "spin";
    activePointerId = null;
    dragIntent = "none";

    onSpinStart?.();
    setCardsWillChange(true);
    applyAllTransforms(virtualPos);
    startLoop();
    return chosen;
  }

  /**
   * Smoothly focuses a specific card index to the center using shortest circular path.
   */
  export function focusIndex(index: number): void {
    const count = items.length;
    if (isSpinning || count === 0) return;
    const clampedIdx = wrapIndex(index, count);
    const snappedBase = Math.round(virtualPos);
    let delta: number;
    if (loop && count > 1) {
      const currentIdx = wrapIndex(snappedBase, count);
      delta = clampedIdx - currentIdx;
      const half = count / 2;
      if (delta > half) delta -= count;
      else if (delta < -half) delta += count;
    } else {
      delta = clampedIdx - snappedBase;
    }

    if (delta === 0 && Math.abs(virtualPos - snappedBase) < 0.01) return;

    if (prefersReducedMotion()) {
      stopLoop();
      virtualPos = clampedIdx;
      targetPos = clampedIdx;
      velocity = 0;
      mode = "idle";
      setCardsWillChange(false);
      applyAllTransforms(virtualPos);
      reportActiveIfChanged(clampedIdx);
      return;
    }

    targetPos = clampNonLoopTarget(snappedBase + delta);
    mode = "settle";
    setCardsWillChange(true);
    startLoop();
  }

  /**
   * Steps the carousel forward (+1) or backward (-1) by one card.
   */
  export function step(delta: number): void {
    const count = items.length;
    if (isSpinning || count <= 1 || delta === 0) return;
    const base = mode === "settle" ? targetPos : Math.round(virtualPos);
    const next = clampNonLoopTarget(base + Math.sign(delta));
    if (next === base && !loop) return;

    if (prefersReducedMotion()) {
      stopLoop();
      const idx = loop ? wrapIndex(next, count) : next;
      virtualPos = idx;
      targetPos = idx;
      velocity = 0;
      mode = "idle";
      setCardsWillChange(false);
      applyAllTransforms(virtualPos);
      reportActiveIfChanged(idx);
      return;
    }

    targetPos = next;
    mode = "settle";
    setCardsWillChange(true);
    startLoop();
  }

  function handlePointerDown(e: PointerEvent) {
    if (isSpinning || items.length <= 1) return;
    if (e.pointerType === "mouse" && e.button !== 0) return;
    if (activePointerId !== null) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest("[data-carousel-nav]")) return;

    activePointerId = e.pointerId;
    dragIntent = "pending";
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    dragStartPos = virtualPos;
    dragLastX = e.clientX;
    dragLastTs = performance.now();
    dragVel = 0;
  }

  function handlePointerMove(e: PointerEvent) {
    if (isSpinning || activePointerId !== e.pointerId || dragIntent === "none") return;

    const dx = e.clientX - dragStartX;
    const dy = e.clientY - dragStartY;

    if (dragIntent === "pending") {
      if (Math.hypot(dx, dy) < 7) return;
      // Preserve native vertical scrolling on Android touch devices when gesture is mostly vertical.
      if (e.pointerType !== "mouse" && Math.abs(dy) > Math.abs(dx) * 1.1) {
        activePointerId = null;
        dragIntent = "none";
        return;
      }
      dragIntent = "dragging";
      mode = "drag";
      stopLoop();
      setCardsWillChange(true);
      try {
        stageEl?.setPointerCapture(e.pointerId);
      } catch {
        // Ignore synthetic pointer events without capture support.
      }
    }

    const now = performance.now();
    const dt = Math.max(0.004, (now - dragLastTs) / 1000);
    const pxPerCard = Math.max(85, spread * 1.55 * stageScale);
    const stepDelta = -(e.clientX - dragLastX) / pxPerCard;
    const instantVel = stepDelta / dt;
    // Exponential velocity smoothing (refresh-rate invariant via dt)
    const blend = 1 - Math.exp(-18 * dt);
    dragVel = dragVel + (instantVel - dragVel) * blend;

    let nextPos = dragStartPos - dx / pxPerCard;
    if (!loop) {
      const maxIdx = Math.max(0, items.length - 1);
      if (nextPos < 0) nextPos *= 0.35;
      else if (nextPos > maxIdx) nextPos = maxIdx + (nextPos - maxIdx) * 0.35;
    }

    virtualPos = nextPos;
    dragLastX = e.clientX;
    dragLastTs = now;
    applyAllTransforms(virtualPos);

    const nearest = loop
      ? wrapIndex(Math.round(virtualPos), items.length)
      : Math.max(0, Math.min(items.length - 1, Math.round(virtualPos)));
    if (nearest !== lastReportedIdx) {
      reportActiveIfChanged(nearest);
    }
  }

  function endPointerGesture(e: PointerEvent) {
    if (activePointerId !== e.pointerId) return;
    const wasDragging = dragIntent === "dragging";
    activePointerId = null;
    dragIntent = "none";

    try {
      if (stageEl?.hasPointerCapture(e.pointerId)) {
        stageEl.releasePointerCapture(e.pointerId);
      }
    } catch {
      // Ignore release errors on detached pointers.
    }

    if (!wasDragging) return;
    suppressClickUntil = performance.now() + 90;

    if (isSpinning || items.length === 0) {
      setCardsWillChange(false);
      return;
    }

    const flingOffset = Math.max(-2.6, Math.min(2.6, dragVel * 0.18));
    targetPos = clampNonLoopTarget(Math.round(virtualPos + flingOffset));
    velocity = Math.max(-14, Math.min(14, dragVel));

    if (prefersReducedMotion()) {
      const idx = loop ? wrapIndex(targetPos, items.length) : targetPos;
      virtualPos = idx;
      targetPos = idx;
      velocity = 0;
      mode = "idle";
      setCardsWillChange(false);
      applyAllTransforms(virtualPos);
      reportActiveIfChanged(idx);
      return;
    }

    mode = "settle";
    setCardsWillChange(true);
    startLoop();
  }

  function handleCardClick(index: number) {
    if (isSpinning || performance.now() < suppressClickUntil) return;
    const count = items.length;
    if (count === 0) return;
    const currentIdx = wrapIndex(Math.round(virtualPos), count);
    if (index === currentIdx) {
      const selected = items[index];
      if (selected) onSelectStory?.(selected);
    } else {
      focusIndex(index);
    }
  }

  // Svelte actions for zero-leak direct DOM ref binding (no reactive array churn)
  function bindCardNode(node: HTMLDivElement, idx: number) {
    let cur = idx;
    cardEls[cur] = node;
    applyCardTransform(cur, virtualPos, items.length, currentConfig());
    return {
      update(nextIdx: number) {
        if (cardEls[cur] === node) cardEls[cur] = null;
        cur = nextIdx;
        cardEls[cur] = node;
        applyCardTransform(cur, virtualPos, items.length, currentConfig());
      },
      destroy() {
        if (cardEls[cur] === node) cardEls[cur] = null;
        node.style.willChange = "";
      },
    };
  }

  function bindFrameNode(node: HTMLDivElement, idx: number) {
    let cur = idx;
    frameEls[cur] = node;
    return {
      update(nextIdx: number) {
        if (frameEls[cur] === node) frameEls[cur] = null;
        cur = nextIdx;
        frameEls[cur] = node;
      },
      destroy() {
        if (frameEls[cur] === node) frameEls[cur] = null;
      },
    };
  }

  function bindShadeNode(node: HTMLSpanElement, idx: number) {
    let cur = idx;
    shadeEls[cur] = node;
    return {
      update(nextIdx: number) {
        if (shadeEls[cur] === node) shadeEls[cur] = null;
        cur = nextIdx;
        shadeEls[cur] = node;
      },
      destroy() {
        if (shadeEls[cur] === node) shadeEls[cur] = null;
      },
    };
  }

  // Sync only when the sampled deck membership/order actually changes — NOT on metadata hydration!
  $effect(() => {
    const signature = items.map((it) => it.id).join(",");
    if (signature === lastDeckSignature) {
      // Metadata hydrated on existing cards: refresh parent callback cleanly without moving deck.
      if (items.length > 0 && lastReportedIdx >= 0 && lastReportedIdx < items.length) {
        onActiveChange?.(items[lastReportedIdx], lastReportedIdx);
      }
      return;
    }
    lastDeckSignature = signature;
    stopLoop();
    cancelHighlightPulse();
    activeSpin = null;
    isSpinning = false;
    mode = "idle";
    virtualPos = 0;
    targetPos = 0;
    velocity = 0;
    setCardsWillChange(false);
    cardEls.length = items.length;
    frameEls.length = items.length;
    shadeEls.length = items.length;
    applyAllTransforms(0);
    reportActiveIfChanged(0, true);
  });

  onMount(() => {
    updateStageMetrics();

    if (stageEl && typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(() => {
        updateStageMetrics();
      });
      resizeObserver.observe(stageEl);
    }

    const onWheel = (e: WheelEvent) => {
      if (isSpinning || items.length <= 1) return;
      const dominant = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      if (Math.abs(dominant) < 2) return;
      e.preventDefault();
      wheelAccum += dominant;
      const now = performance.now();
      if (Math.abs(wheelAccum) >= 28 && now - lastWheelStepAt >= 85) {
        const dir = wheelAccum > 0 ? 1 : -1;
        wheelAccum = 0;
        lastWheelStepAt = now;
        step(dir);
      }
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (!keyboardEnabled || isSpinning || items.length === 0) return;
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "SELECT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      ) {
        return;
      }
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        step(-1);
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        step(1);
      }
    };

    stageEl?.addEventListener("wheel", onWheel, { passive: false });
    window.addEventListener("keydown", onKeyDown);

    return () => {
      stageEl?.removeEventListener("wheel", onWheel);
      window.removeEventListener("keydown", onKeyDown);
    };
  });

  onDestroy(() => {
    stopLoop();
    cancelHighlightPulse();
    resizeObserver?.disconnect();
    resizeObserver = null;
    setCardsWillChange(false);
    for (let i = 0; i < cardEls.length; i += 1) {
      const el = cardEls[i];
      if (el) el.style.willChange = "";
    }
    cardEls = [];
    frameEls = [];
    shadeEls = [];
  });
</script>

<div class="relative flex w-full flex-col items-center select-none">
  <!-- 3D Perspective Stage -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={stageEl}
    role="region"
    aria-roledescription="carousel"
    aria-label={t.randomMangaTitle}
    tabindex="0"
    onpointerdown={handlePointerDown}
    onpointermove={handlePointerMove}
    onpointerup={endPointerGesture}
    onpointercancel={endPointerGesture}
    ondragstart={(e) => e.preventDefault()}
    class="depth-stage relative flex w-full items-center justify-center overflow-hidden rounded-2xl outline-none"
    style="perspective: {perspective}px;"
  >
    <!-- Ambient radial spotlight (static CSS gradient, zero blur filter) -->
    <div class="depth-stage-glow pointer-events-none absolute inset-0" aria-hidden="true"></div>

    <!-- 3D Track -->
    <div class="depth-track relative flex h-full w-full items-center justify-center">
      {#each items as item, idx (item.id)}
        {@const statusCue = item.hasNew
          ? t.badgeNew
          : item.isCompleted
            ? t.badgeCompleted
            : item.status}
        <div
          use:bindCardNode={idx}
          class="depth-card"
          data-active={idx === activeIndex ? "true" : "false"}
        >
          <div
            use:bindFrameNode={idx}
            role="button"
            tabindex={idx === activeIndex ? 0 : -1}
            aria-label={item.title}
            onclick={() => handleCardClick(idx)}
            onkeydown={(e) => {
              if ((e.key === "Enter" || e.key === " ") && !isSpinning) {
                e.preventDefault();
                handleCardClick(idx);
              }
            }}
            class="depth-card-frame card-contain relative h-full w-full cursor-pointer overflow-hidden rounded-2xl border text-left"
          >
            {#if item.cover && !failedCovers[item.id]}
              <img
                src={imgProxyUrl(item.cover)}
                alt={item.title}
                decoding="async"
                loading="eager"
                draggable="false"
                onerror={() => {
                  failedCovers = { ...failedCovers, [item.id]: true };
                }}
                class="h-full w-full object-cover"
              />
            {:else}
              <div
                class="flex h-full w-full flex-col items-center justify-center gap-2 px-4 text-center"
                style="background-color: var(--surface-subtle); color: var(--text-muted);"
              >
                <BookOpen size={34} strokeWidth={1.5} />
                <span class="line-clamp-3 text-xs font-semibold" style="color: var(--text);">
                  {item.title}
                </span>
              </div>
            {/if}

            <!-- Bottom legibility gradient + active card title bar -->
            <div
              class="pointer-events-none absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/85 via-black/45 to-transparent px-3 pt-10 pb-3"
            >
              <div class="line-clamp-2 text-xs font-bold leading-snug text-white drop-shadow-sm">
                {item.title}
              </div>
              {#if item.chapters > 0}
                <div class="mt-0.5 text-[10px] font-medium text-white/75">
                  {t.chapterCount(item.chapters)}
                </div>
              {/if}
            </div>

            <!-- Subtle status / progress badge (visible primarily on the active center card) -->
            <div class="depth-active-badges pointer-events-none absolute top-2.5 right-2.5 left-2.5 flex items-start justify-between gap-1.5">
              {#if statusCue}
                <span
                  class="inline-flex max-w-[70%] items-center gap-1 truncate rounded-full px-2 py-0.5 text-[10px] font-semibold shadow-sm"
                  style="background-color: {item.hasNew
                    ? 'var(--accent)'
                    : item.isCompleted
                      ? 'rgba(16, 185, 129, 0.92)'
                      : 'rgba(11, 11, 15, 0.82)'}; color: {item.hasNew ? 'var(--accent-fg)' : '#ffffff'};"
                >
                  {#if item.hasNew}
                    <Sparkles size={10} />
                  {:else if item.isCompleted}
                    <CheckCircle2 size={10} />
                  {/if}
                  <span class="truncate capitalize">{statusCue}</span>
                </span>
              {:else}
                <span></span>
              {/if}

              {#if item.rating}
                <span
                  class="shrink-0 rounded-full px-2 py-0.5 text-[10px] font-bold tabular-nums text-white shadow-sm"
                  style="background-color: rgba(11, 11, 15, 0.82);"
                >
                  {item.rating}
                </span>
              {/if}
            </div>

            <!-- GPU-cheap depth darkening veil (opacity-only, zero CSS filter cost) -->
            <span
              use:bindShadeNode={idx}
              class="pointer-events-none absolute inset-0 bg-black"
              style="opacity: 0;"
              aria-hidden="true"
            ></span>
          </div>
        </div>
      {/each}
    </div>

    <!-- Left / Right Desktop & Touch Step Controls -->
    {#if items.length > 1}
      <button
        type="button"
        data-carousel-nav
        disabled={isSpinning}
        onclick={() => step(-1)}
        class="gpu-press absolute top-1/2 left-2.5 z-20 flex h-10 w-10 -translate-y-1/2 items-center justify-center rounded-full border shadow-md disabled:pointer-events-none disabled:opacity-35 sm:left-3.5"
        style="background-color: var(--surface-glass-solid); border-color: var(--border); color: var(--text);"
        aria-label={t.browsePrevPage}
      >
        <ChevronLeft size={18} />
      </button>

      <button
        type="button"
        data-carousel-nav
        disabled={isSpinning}
        onclick={() => step(1)}
        class="gpu-press absolute top-1/2 right-2.5 z-20 flex h-10 w-10 -translate-y-1/2 items-center justify-center rounded-full border shadow-md disabled:pointer-events-none disabled:opacity-35 sm:right-3.5"
        style="background-color: var(--surface-glass-solid); border-color: var(--border); color: var(--text);"
        aria-label={t.browseNextPage}
      >
        <ChevronRight size={18} />
      </button>
    {/if}
  </div>

  <!-- Subtle deck counter & interaction hint -->
  {#if items.length > 0}
    <div
      class="mt-2.5 flex flex-wrap items-center justify-center gap-x-3 gap-y-1 text-[11px]"
      style="color: var(--text-muted);"
    >
      <span class="font-semibold tabular-nums" style="color: var(--text);">
        {isSpinning ? "•••" : `${activeIndex + 1} / ${items.length}`}
      </span>
      <span aria-hidden="true">·</span>
      <span>{t.randomMangaGestureHint}</span>
    </div>
  {/if}
</div>

<style>
  .depth-stage {
    --dc-card-w: 260px;
    --dc-card-h: 390px;
    height: clamp(300px, 46dvh, 450px);
    touch-action: pan-y;
    cursor: grab;
    background-color: var(--surface-subtle);
    border: 1px solid var(--border);
  }

  .depth-stage:active {
    cursor: grabbing;
  }

  .depth-stage-glow {
    background: radial-gradient(
      60% 62% at 50% 50%,
      color-mix(in srgb, var(--accent) 12%, transparent),
      transparent 72%
    );
  }

  .depth-track {
    transform-style: preserve-3d;
  }

  .depth-card {
    position: absolute;
    width: var(--dc-card-w);
    height: var(--dc-card-h);
    transform-origin: center center;
    backface-visibility: hidden;
    -webkit-backface-visibility: hidden;
  }

  .depth-card-frame {
    border-color: rgba(255, 255, 255, 0.14);
    box-shadow: var(--shadow-md);
    background-color: var(--surface);
    transition:
      border-color var(--dur-fast, 140ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1)),
      box-shadow var(--dur-fast, 140ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1));
  }

  .depth-card[data-active="true"] .depth-card-frame {
    border-color: var(--accent);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--accent) 55%, transparent),
      var(--shadow-lg);
  }

  .depth-active-badges {
    opacity: 0;
    transition: opacity var(--dur-fast, 140ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1));
  }

  .depth-card[data-active="true"] .depth-active-badges {
    opacity: 1;
  }

  @media (min-width: 1024px) {
    .depth-stage {
      height: clamp(410px, 56dvh, 470px);
    }
  }
</style>
