<script lang="ts">
  import { onMount, tick } from "svelte";
  import { X, Play, RotateCcw, Check, Sparkles } from "@lucide/svelte";
  import { t, type Key } from "$lib/i18n/index.svelte";

  export type BezierCurve = [number, number, number, number];

  interface Props {
    open: boolean;
    curve?: BezierCurve;
    title?: string;
    onapply: (curve: BezierCurve) => void;
    onclose: () => void;
  }

  let {
    open = $bindable(false),
    curve = [0.4, 0.0, 0.2, 1.0],
    title,
    onapply,
    onclose,
  }: Props = $props();

  // Local state for handles P1(x1, y1) and P2(x2, y2)
  let p1x = $state(0.4);
  let p1y = $state(0.0);
  let p2x = $state(0.2);
  let p2y = $state(1.0);

  $effect(() => {
    if (open) {
      p1x = curve[0];
      p1y = curve[1];
      p2x = curve[2];
      p2y = curve[3];
    }
  });

  const PRESET_ENTRIES: { id: string; curve: BezierCurve; labelKey: Key; descKey: Key }[] = [
    { id: "smooth", curve: [0.4, 0.0, 0.2, 1.0], labelKey: "curve.preset.smooth", descKey: "curve.preset.smoothDesc" },
    { id: "linear", curve: [0.0, 0.0, 1.0, 1.0], labelKey: "curve.preset.linear", descKey: "curve.preset.linearDesc" },
    { id: "slowIn", curve: [0.42, 0.0, 1.0, 1.0], labelKey: "curve.preset.slowIn", descKey: "curve.preset.slowInDesc" },
    { id: "slowOut", curve: [0.0, 0.0, 0.58, 1.0], labelKey: "curve.preset.slowOut", descKey: "curve.preset.slowOutDesc" },
    { id: "smoothInOut", curve: [0.42, 0.0, 0.58, 1.0], labelKey: "curve.preset.smoothInOut", descKey: "curve.preset.smoothInOutDesc" },
    { id: "snappy", curve: [0.15, 0.85, 0.35, 1.0], labelKey: "curve.preset.snappy", descKey: "curve.preset.snappyDesc" },
    { id: "dramatic", curve: [0.75, 0.0, 0.25, 1.0], labelKey: "curve.preset.dramatic", descKey: "curve.preset.dramaticDesc" },
    { id: "overshoot", curve: [0.34, 1.25, 0.64, 1.0], labelKey: "curve.preset.overshoot", descKey: "curve.preset.overshootDesc" },
  ];

  // SVG Canvas dimensions
  const SVG_SIZE = 260;
  const PAD = 25;
  const PLOT_W = SVG_SIZE - PAD * 2;
  const PLOT_H = SVG_SIZE - PAD * 2;

  let activeHandle = $state<"p1" | "p2" | null>(null);
  let svgEl = $state<SVGSVGElement | null>(null);

  // Conversion math between curve space [0..1] and SVG space
  function curveToSvg(x: number, y: number): { cx: number; cy: number } {
    const cx = PAD + Math.max(0, Math.min(1, x)) * PLOT_W;
    const cy = PAD + (1 - Math.max(-0.4, Math.min(1.4, y))) * PLOT_H;
    return { cx, cy };
  }

  function svgToCurve(clientX: number, clientY: number): { x: number; y: number } {
    if (!svgEl) return { x: 0, y: 0 };
    const rect = svgEl.getBoundingClientRect();
    const rawX = clientX - rect.left - PAD;
    const rawY = clientY - rect.top - PAD;
    const x = Math.max(0, Math.min(1, rawX / PLOT_W));
    const y = 1 - rawY / PLOT_H;
    return {
      x: Math.round(x * 100) / 100,
      y: Math.round(Math.max(-0.4, Math.min(1.4, y)) * 100) / 100,
    };
  }

  const p0Svg = $derived(curveToSvg(0, 0));
  const p1Svg = $derived(curveToSvg(p1x, p1y));
  const p2Svg = $derived(curveToSvg(p2x, p2y));
  const p3Svg = $derived(curveToSvg(1, 1));

  const bezierPathD = $derived(
    `M ${p0Svg.cx},${p0Svg.cy} C ${p1Svg.cx},${p1Svg.cy} ${p2Svg.cx},${p2Svg.cy} ${p3Svg.cx},${p3Svg.cy}`
  );

  function handlePointerDown(handle: "p1" | "p2", e: PointerEvent) {
    e.preventDefault();
    activeHandle = handle;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function handlePointerMove(e: PointerEvent) {
    if (!activeHandle) return;
    const coords = svgToCurve(e.clientX, e.clientY);
    if (activeHandle === "p1") {
      p1x = coords.x;
      p1y = coords.y;
    } else if (activeHandle === "p2") {
      p2x = coords.x;
      p2y = coords.y;
    }
  }

  function handlePointerUp() {
    activeHandle = null;
  }

  function applyPreset(p: BezierCurve) {
    p1x = p[0];
    p1y = p[1];
    p2x = p[2];
    p2y = p[3];
  }

  // Live simulation ball moving across track
  let simTime = $state(0);
  let animId: number;

  function sampleCubicBezier(tVal: number, x1: number, y1: number, x2: number, y2: number): number {
    // Simple iterative solver for cubic bezier progression
    let u = tVal;
    for (let i = 0; i < 5; i++) {
      const currentSlope = 3 * (1 - u) * (1 - u) * x1 + 6 * (1 - u) * u * (x2 - x1) + 3 * u * u * (1 - x2);
      if (Math.abs(currentSlope) < 1e-4) break;
      const currentX = 3 * (1 - u) * (1 - u) * u * x1 + 3 * (1 - u) * u * u * x2 + u * u * u;
      const diff = currentX - tVal;
      u -= diff / currentSlope;
      u = Math.max(0, Math.min(1, u));
    }
    const y = 3 * (1 - u) * (1 - u) * u * y1 + 3 * (1 - u) * u * u * y2 + u * u * u;
    return y;
  }

  onMount(() => {
    let start = performance.now();
    function loop(now: number) {
      const elapsed = (now - start) / 1000;
      const period = 2.0;
      const tNorm = (elapsed % period) / period;
      simTime = sampleCubicBezier(tNorm, p1x, p1y, p2x, p2y);
      animId = requestAnimationFrame(loop);
    }
    animId = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(animId);
  });

  function handleSave() {
    onapply([p1x, p1y, p2x, p2y]);
    onclose();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="curve-modal-backdrop" onclick={onclose}>
    <div
      class="curve-modal card"
      onclick={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label={title ?? t("curve.title")}
    >
      <div class="cm-head">
        <div class="cm-title-wrap">
          <Sparkles size={16} class="accent-icon" />
          <h3>{title ?? t("curve.title")}</h3>
        </div>
        <button type="button" class="ghost icon-btn" onclick={onclose} aria-label={t("modal.close")}>
          <X size={16} />
        </button>
      </div>

      <div class="cm-content">
        <!-- Panel Izquierdo: SVG interactivo y simulación -->
        <div class="cm-visual">
          <svg
            bind:this={svgEl}
            viewBox="0 0 {SVG_SIZE} {SVG_SIZE}"
            class="curve-svg"
            onpointermove={handlePointerMove}
            onpointerup={handlePointerUp}
            onpointercancel={handlePointerUp}
          >
            <!-- Cuadrícula de referencia -->
            <rect x={PAD} y={PAD} width={PLOT_W} height={PLOT_H} fill="rgba(14, 10, 26, 0.45)" rx="8" />
            <line x1={PAD} y1={PAD + PLOT_H} x2={PAD + PLOT_W} y2={PAD} stroke="rgba(184, 169, 255, 0.08)" stroke-dasharray="3 3" />
            <line x1={PAD} y1={PAD + PLOT_H / 2} x2={PAD + PLOT_W} y2={PAD + PLOT_H / 2} stroke="rgba(184, 169, 255, 0.05)" />
            <line x1={PAD + PLOT_W / 2} y1={PAD} x2={PAD + PLOT_W / 2} y2={PAD + PLOT_H} stroke="rgba(184, 169, 255, 0.05)" />

            <!-- Líneas de las manijas de control P1 y P2 -->
            <line x1={p0Svg.cx} y1={p0Svg.cy} x2={p1Svg.cx} y2={p1Svg.cy} stroke="#3d5afe" stroke-width="1.75" stroke-dasharray="2 2" />
            <line x1={p3Svg.cx} y1={p3Svg.cy} x2={p2Svg.cx} y2={p2Svg.cy} stroke="#b8a9ff" stroke-width="1.75" stroke-dasharray="2 2" />

            <!-- Curva Bézier -->
            <path d={bezierPathD} fill="none" stroke="var(--accent, #3d5afe)" stroke-width="3.5" stroke-linecap="round" />

            <!-- Anclas inicio y fin fijas -->
            <circle cx={p0Svg.cx} cy={p0Svg.cy} r="4.5" fill="#ffffff" />
            <circle cx={p3Svg.cx} cy={p3Svg.cy} r="4.5" fill="#ffffff" />

            <!-- Manija interactiva P1 (Azul) -->
            <g
              class="cm-handle handle-p1"
              transform="translate({p1Svg.cx}, {p1Svg.cy})"
              onpointerdown={(e) => handlePointerDown("p1", e)}
            >
              <circle r="9" fill="#3d5afe" stroke="#ffffff" stroke-width="2.5" />
            </g>

            <!-- Manija interactiva P2 (Lavanda) -->
            <g
              class="cm-handle handle-p2"
              transform="translate({p2Svg.cx}, {p2Svg.cy})"
              onpointerdown={(e) => handlePointerDown("p2", e)}
            >
              <circle r="9" fill="#b8a9ff" stroke="#ffffff" stroke-width="2.5" />
            </g>
          </svg>

          <!-- Previsualizador dinámico animado -->
          <div class="sim-rail-wrap">
            <span class="sim-label">{t("curve.preview")}</span>
            <div class="sim-rail">
              <div
                class="sim-ball"
                style="left: {Math.max(0, Math.min(100, simTime * 100))}%"
              ></div>
            </div>
          </div>
        </div>

        <!-- Panel Derecho: Coordenadas y Presets Rápidos -->
        <div class="cm-controls">
          <div class="cm-coords-grid">
            <label class="coord-field">
              <span>P1.x</span>
              <input type="number" min="0" max="1" step="0.05" bind:value={p1x} />
            </label>
            <label class="coord-field">
              <span>P1.y</span>
              <input type="number" min="-0.4" max="1.4" step="0.05" bind:value={p1y} />
            </label>
            <label class="coord-field">
              <span>P2.x</span>
              <input type="number" min="0" max="1" step="0.05" bind:value={p2x} />
            </label>
            <label class="coord-field">
              <span>P2.y</span>
              <input type="number" min="-0.4" max="1.4" step="0.05" bind:value={p2y} />
            </label>
          </div>

          <div class="presets-section">
            <span class="presets-title">{t("curve.presetsTitle")}</span>
            <div class="presets-list">
              {#each PRESET_ENTRIES as p (p.id)}
                {@const isMatch =
                  Math.abs(p1x - p.curve[0]) < 0.05 &&
                  Math.abs(p1y - p.curve[1]) < 0.05 &&
                  Math.abs(p2x - p.curve[2]) < 0.05 &&
                  Math.abs(p2y - p.curve[3]) < 0.05}
                <button
                  type="button"
                  class="preset-item"
                  class:active={isMatch}
                  onclick={() => applyPreset(p.curve)}
                >
                  <div class="preset-info">
                    <strong>{t(p.labelKey)}</strong>
                    <small>{t(p.descKey)}</small>
                  </div>
                  {#if isMatch}
                    <Check size={14} class="check-icon" />
                  {/if}
                </button>
              {/each}
            </div>
          </div>
        </div>
      </div>

      <div class="cm-footer">
        <button type="button" class="ghost sm" onclick={() => applyPreset([0.4, 0.0, 0.2, 1.0])}>
          <RotateCcw size={14} /> {t("curve.reset")}
        </button>
        <div class="cm-footer-right">
          <button type="button" class="ghost sm" onclick={onclose}>{t("curve.cancel")}</button>
          <button type="button" class="primary sm" onclick={handleSave}>
            <Check size={14} /> {t("curve.apply")}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .curve-modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 10001;
    background: rgba(14, 10, 26, 0.75);
    backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    animation: fadeIn 150ms ease-out;
  }

  .curve-modal {
    width: 100%;
    max-width: 620px;
    background: var(--surface, #1a1230);
    border: 1px solid var(--select-border, rgba(184, 169, 255, 0.2));
    border-radius: 18px;
    box-shadow:
      0 0 0 1px rgba(255, 255, 255, 0.08),
      0 20px 45px -10px rgba(0, 0, 0, 0.7);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: scaleIn 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes scaleIn {
    from { opacity: 0; transform: scale(0.95); }
    to { opacity: 1; transform: scale(1); }
  }

  .cm-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid rgba(184, 169, 255, 0.1);
  }

  .cm-title-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .cm-title-wrap h3 {
    margin: 0;
    font-size: 0.95rem;
    color: var(--text, #ece8ff);
    font-weight: 600;
  }

  :global(.accent-icon) {
    color: var(--accent, #3d5afe);
  }

  .icon-btn {
    padding: 6px;
    border-radius: 8px;
  }

  .cm-content {
    display: grid;
    grid-template-columns: 280px 1fr;
    gap: 16px;
    padding: 16px 18px;
  }

  @media (max-width: 600px) {
    .cm-content {
      grid-template-columns: 1fr;
    }
  }

  .cm-visual {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .curve-svg {
    width: 100%;
    height: auto;
    background: rgba(14, 10, 26, 0.6);
    border: 1px solid rgba(184, 169, 255, 0.14);
    border-radius: 12px;
    user-select: none;
    touch-action: none;
  }

  .cm-handle {
    cursor: grab;
    transition: transform 100ms ease;
  }

  .cm-handle:active {
    cursor: grabbing;
    transform: scale(1.15);
  }

  .sim-rail-wrap {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .sim-label {
    font-size: 0.72rem;
    color: var(--text-2, #a59cc7);
    font-weight: 500;
  }

  .sim-rail {
    position: relative;
    height: 14px;
    background: rgba(184, 169, 255, 0.12);
    border-radius: 999px;
    padding: 2px;
  }

  .sim-ball {
    position: absolute;
    top: 2px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--accent, #3d5afe);
    box-shadow: 0 0 8px var(--accent-glow, rgba(61, 90, 254, 0.45));
    transform: translateX(-50%);
  }

  .cm-controls {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }

  .cm-coords-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
  }

  .coord-field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    background: rgba(14, 10, 26, 0.5);
    border: 1px solid rgba(184, 169, 255, 0.14);
    border-radius: 8px;
    padding: 4px 8px;
    font-size: 0.76rem;
    color: var(--text-2, #a59cc7);
  }

  .coord-field input {
    width: 60px;
    background: transparent;
    border: 0;
    color: var(--text, #ece8ff);
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 0.78rem;
    text-align: right;
    outline: none;
  }

  .presets-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .presets-title {
    font-size: 0.74rem;
    color: var(--lavender, #b8a9ff);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .presets-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 200px;
    overflow-y: auto;
    padding-right: 2px;
  }

  .preset-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    background: rgba(14, 10, 26, 0.45);
    border: 1px solid rgba(184, 169, 255, 0.08);
    border-radius: 8px;
    cursor: pointer;
    text-align: left;
    transition: background 120ms ease, border-color 120ms ease;
  }

  .preset-item:hover {
    background: rgba(184, 169, 255, 0.08);
    border-color: rgba(184, 169, 255, 0.2);
  }

  .preset-item.active {
    background: var(--velvet, #2a1446);
    border-color: var(--accent, #3d5afe);
  }

  .preset-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .preset-info strong {
    font-size: 0.78rem;
    color: var(--text, #ece8ff);
  }

  .preset-info small {
    font-size: 0.68rem;
    color: var(--text-2, #a59cc7);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  :global(.check-icon) {
    color: var(--accent, #3d5afe);
    flex-shrink: 0;
  }

  .cm-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 18px;
    border-top: 1px solid rgba(184, 169, 255, 0.1);
    background: rgba(14, 10, 26, 0.25);
  }

  .cm-footer-right {
    display: flex;
    gap: 8px;
  }
</style>
