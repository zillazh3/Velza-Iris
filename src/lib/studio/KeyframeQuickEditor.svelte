<script lang="ts">
  import { onMount, tick } from "svelte";
  import { X, Camera, Copy, Trash2, Sliders, Activity, Sparkles } from "@lucide/svelte";
  import type { Keyframe, Pose } from "$lib/api";
  import IrisSelect, { type SelectOption } from "$lib/components/ui/IrisSelect.svelte";
  import IrisCheckbox from "$lib/components/ui/IrisCheckbox.svelte";
  import CurveEditorModal, { type BezierCurve } from "$lib/components/ui/CurveEditorModal.svelte";
  import { t, formatKeyframeLabel } from "$lib/i18n/index.svelte";

  interface Props {
    open: boolean;
    keyframe: Keyframe;
    maxDuration: number;
    canUpdateFromCamera: boolean;
    x: number;
    y: number;
    onupdate: () => void;
    onduplicate: () => void;
    ondelete: () => void;
    onchange: () => void;
    onclose: () => void;
  }

  let {
    open = $bindable(false),
    keyframe = $bindable(),
    maxDuration,
    canUpdateFromCamera,
    x,
    y,
    onupdate,
    onduplicate,
    ondelete,
    onchange,
    onclose,
  }: Props = $props();

  let popoverEl = $state<HTMLDivElement | null>(null);
  let adjustedX = $state(0);
  let adjustedY = $state(0);
  let showCurveEditor = $state(false);

  const transitionOptions = $derived<SelectOption[]>([
    { value: "linear", label: t("curve.preset.linear"), description: t("curve.preset.linearDesc") },
    { value: "smooth", label: t("curve.preset.smooth"), description: t("quickEdit.smoothDesc") },
    { value: "slowIn", label: t("curve.preset.slowIn"), description: t("quickEdit.slowInDesc") },
    { value: "slowOut", label: t("curve.preset.slowOut"), description: t("curve.preset.slowOutDesc") },
    { value: "smoothInOut", label: t("curve.preset.smoothInOut"), description: t("curve.preset.smoothInOutDesc") },
  ]);

  $effect(() => {
    if (open) {
      adjustedX = x;
      adjustedY = y;
      void tick().then(reposition);
    }
  });

  function reposition() {
    if (!popoverEl) return;
    const rect = popoverEl.getBoundingClientRect();
    const pad = 12;
    let nextX = x - rect.width / 2;
    let nextY = y - rect.height - 14;

    if (nextY < pad) {
      nextY = y + 24; // Desplegar hacia abajo si no cabe arriba
    }
    if (nextX < pad) {
      nextX = pad;
    } else if (nextX + rect.width > window.innerWidth - pad) {
      nextX = window.innerWidth - rect.width - pad;
    }

    adjustedX = nextX;
    adjustedY = nextY;
  }

  function handleCustomCurveApply(curve: BezierCurve) {
    (keyframe as any).customCurve = curve;
    onchange();
  }

  onMount(() => {
    function handlePointerDown(e: PointerEvent) {
      if (!open || !popoverEl || showCurveEditor) return;
      if (!popoverEl.contains(e.target as Node)) {
        onclose();
      }
    }
    window.addEventListener("pointerdown", handlePointerDown, { capture: true });
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, { capture: true });
    };
  });
</script>

{#if open}
  <div
    bind:this={popoverEl}
    class="kf-popover card"
    style="left: {adjustedX}px; top: {adjustedY}px;"
    role="dialog"
    tabindex="-1"
    aria-label={t("quickEdit.titleAria")}
  >
    <div class="pop-head">
      <div class="pop-title">
        <Activity size={14} class="pop-icon" />
        <input
          type="text"
          class="title-input"
          bind:value={keyframe.label}
          oninput={onchange}
          title={t("quickEdit.editLabel")}
        />
      </div>
      <button type="button" class="ghost icon-btn" onclick={onclose} aria-label="Cerrar">
        <X size={14} />
      </button>
    </div>

    <div class="pop-body">
      <div class="pop-row-two">
        <label class="pop-field">
          <span class="pop-label">Tiempo (s)</span>
          <input
            type="number"
            min="0"
            max={maxDuration}
            step="0.05"
            bind:value={keyframe.time}
            onchange={onchange}
          />
        </label>

        <label class="pop-field">
          <span class="pop-label">Zoom (mm)</span>
          <input
            type="number"
            min="20"
            max="300"
            step="1"
            placeholder="Sin cambio"
            value={keyframe.look?.zoom ?? ""}
            oninput={(e) => {
              const val = e.currentTarget.value.trim();
              if (!keyframe.look) {
                keyframe.look = { zoom: null, exposure: null, focalDistance: null, aperture: null };
              }
              keyframe.look.zoom = val === "" ? null : Number(val);
              onchange();
            }}
          />
        </label>
      </div>

      <div class="pop-field">
        <div class="pop-field-head">
          <span class="pop-label">{t("quickEdit.curveLabel")}</span>
          <button
            type="button"
            class="curve-tool-btn"
            title={t("quickEdit.curveBtn")}
            onclick={() => (showCurveEditor = true)}
          >
            <Sparkles size={12} /> Curva Custom
          </button>
        </div>
        <IrisSelect
          items={transitionOptions}
          value={keyframe.transition}
          size="sm"
          onchange={(val) => {
            keyframe.transition = val as any;
            onchange();
          }}
        />
      </div>

      <div class="pop-field">
        <IrisCheckbox
          variant="switch"
          size="sm"
          checked={keyframe.knotMode === "stop"}
          label={t("quickEdit.stopLabel")}
          description={keyframe.knotMode === "stop" ? t("quickEdit.stopDesc") : t("quickEdit.passDesc")}
          onchange={(isStop) => {
            keyframe.knotMode = isStop ? "stop" : "passThrough";
            onchange();
          }}
        />
      </div>

      <div class="pop-coords-readout">
        <span>XYZ: {keyframe.rawPose.pos.map((v) => v.toFixed(2)).join(", ")} m</span>
        <span>PYR: {keyframe.rawPose.rot.map((v) => v.toFixed(0)).join("°, ")}°</span>
      </div>

      <div class="pop-actions">
        <button
          type="button"
          class="ghost sm"
          disabled={!canUpdateFromCamera}
          title={t("quickEdit.captureTooltip")}
          onclick={onupdate}
        >
          <Camera size={13} /> {t("studio.updatePoint")}
        </button>
        <button type="button" class="ghost sm" title={t("quickEdit.dupTooltip")} onclick={onduplicate}>
          <Copy size={13} /> {t("studio.ctx.duplicate")}
        </button>
        <button type="button" class="ghost sm danger" title={t("quickEdit.delTooltip")} onclick={ondelete}>
          <Trash2 size={13} />
        </button>
      </div>
    </div>
  </div>

  {#if showCurveEditor}
    <CurveEditorModal
      open={showCurveEditor}
      curve={(keyframe as any).customCurve ?? [0.4, 0.0, 0.2, 1.0]}
      title={t("quickEdit.bezierTitle", { label: formatKeyframeLabel(keyframe.label) })}
      onapply={handleCustomCurveApply}
      onclose={() => (showCurveEditor = false)}
    />
  {/if}
{/if}

<style>
  .kf-popover {
    position: fixed;
    z-index: 9999;
    width: 290px;
    padding: 12px;
    background: var(--select-surface, rgba(26, 18, 44, 0.94));
    backdrop-filter: blur(20px) saturate(180%);
    -webkit-backdrop-filter: blur(20px) saturate(180%);
    border-radius: 14px;
    box-shadow:
      0 0 0 1px rgba(255, 255, 255, 0.08),
      0 12px 30px -4px rgba(0, 0, 0, 0.65),
      0 24px 50px -12px rgba(0, 0, 0, 0.85);
    display: flex;
    flex-direction: column;
    gap: 10px;
    animation: popIn 150ms cubic-bezier(0.16, 1, 0.3, 1);
    user-select: none;
  }

  @keyframes popIn {
    from {
      opacity: 0;
      transform: scale(0.94) translateY(4px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .pop-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .pop-title {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }

  :global(.pop-icon) {
    color: var(--accent);
    flex-shrink: 0;
  }

  .title-input {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text);
    font-weight: 600;
    font-size: 0.84rem;
    padding: 2px 4px;
    border-radius: 6px;
    width: 100%;
    outline: none;
    transition: border-color 140ms ease, background 140ms ease;
  }

  .title-input:hover,
  .title-input:focus {
    background: var(--surface-raised);
  }

  .icon-btn {
    padding: 4px;
    border-radius: 6px;
  }

  .pop-body {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  .pop-row-two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .pop-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .pop-field-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .pop-label {
    font-size: 0.72rem;
    color: var(--text-2);
    font-weight: 500;
  }

  .curve-tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 6px;
    font-size: 0.68rem;
    border-radius: 999px;
    background: rgba(var(--accent-color-rgb), 0.15);
    color: var(--text-2);
    border: 1px solid rgba(var(--accent-color-rgb), 0.25);
    cursor: pointer;
  }

  .curve-tool-btn:hover {
    background: rgba(var(--accent-color-rgb), 0.3);
  }

  .pop-field input[type="number"] {
    width: 100%;
    background: var(--surface-raised);
    border-radius: 8px;
    padding: 5px 8px;
    font-size: 0.8rem;
    color: var(--text);
    outline: none;
  }

  .pop-coords-readout {
    display: flex;
    justify-content: space-between;
    font-size: 0.68rem;
    font-family: ui-monospace, SFMono-Regular, monospace;
    color: var(--text-2);
    background: var(--surface-raised);
    padding: 4px 6px;
    border-radius: 6px;
  }

  .pop-actions {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }

  .pop-actions button {
    flex: 1;
    justify-content: center;
    font-size: 0.74rem;
    padding: 5px 8px;
  }

  .pop-actions button.danger {
    flex: 0 0 32px;
    color: #f87171;
  }
</style>
