<script lang="ts" module>
  import type { CinematicProfile, DurationInputMode, DurationUnit, ReferenceSpace } from "$lib/api";

  export type ProjectDraft = {
    name: string;
    seconds: number;
    unit: DurationUnit;
    inputMode: DurationInputMode;
    space: ReferenceSpace;
    trajectoryScale: number;
    profile: CinematicProfile;
    calibrationEnabled: boolean;
    autoCorrect: boolean;
    loop: boolean;
    /** Solo en modo `create`. */
    template: "blank" | "heroic";
  };

  export type DurationConflict = { targetSeconds: number; keyframesCount: number; effectsCount: number };
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { ChevronRight, X } from "@lucide/svelte";
  import type { SequenceProject } from "$lib/api";
  import { t } from "$lib/i18n/index.svelte";
  import IrisSelect, { type SelectOption } from "$lib/components/ui/IrisSelect.svelte";
  import IrisCheckbox from "$lib/components/ui/IrisCheckbox.svelte";
  import {
    DEFAULT_DURATION_SEC,
    formatDurationEquivalence,
    formatSecondsToMmSs,
    MAX_DURATION_SEC,
    MIN_DURATION_SEC,
    parseMmSsInput,
    parseValueUnitInput,
    secondsToUnitValue,
  } from "$lib/studio/duration";
  import { defaultCalibrationConfig, defaultCinematicConfig } from "$lib/studio/catalog";

  interface Props {
    open: boolean;
    mode: "create" | "edit";
    project: SequenceProject;
    conflict: DurationConflict | null;
    onconfirm: (draft: ProjectDraft) => void;
    onresolve: (resolution: "clamp" | "scale" | "cancel") => void;
    onclose: () => void;
  }

  let { open, mode, project, conflict, onconfirm, onresolve, onclose }: Props = $props();

  const ADV_KEY = "velza-project-advanced";
  const QUICK_DURATIONS = [5, 10, 15, 30, 60];

  let dialogEl = $state<HTMLDialogElement | null>(null);
  let nameEl = $state<HTMLInputElement | null>(null);

  let name = $state("");
  let inputMode = $state<DurationInputMode>("valueUnit");
  let unit = $state<DurationUnit>("seconds");
  let valueInput = $state("10");
  let mmSsInput = $state("00:10");
  let space = $state<ReferenceSpace>("anchor");
  let scaleInput = $state("1");
  let profile = $state<CinematicProfile>("cinematic");
  let calibrationEnabled = $state(true);
  let autoCorrect = $state(false);
  let loop = $state(false);
  let template = $state<"blank" | "heroic">("blank");
  let advOpen = $state(false);
  let advSettled = $state(false);

  const parsed = $derived(
    inputMode === "valueUnit" ? parseValueUnitInput(valueInput, unit) : parseMmSsInput(mmSsInput),
  );
  const scale = $derived(Number(scaleInput));
  const scaleOk = $derived(Number.isFinite(scale) && scale > 0 && scale <= 10);
  const canConfirm = $derived(parsed.ok && name.trim().length > 0 && scaleOk && !conflict);
  const equivalence = $derived(formatDurationEquivalence(parsed.ok ? parsed.seconds : project.duration, unit));

  const unitOptions = $derived<SelectOption<DurationUnit>[]>([
    { value: "seconds", label: `${t("studio.config.unitSeconds")} (s)` },
    { value: "minutes", label: `${t("studio.config.unitMinutes")} (min)` },
  ]);
  const spaceOptions = $derived<SelectOption<ReferenceSpace>[]>([
    { value: "anchor", label: t("library.spaceAnchor") },
    { value: "world", label: t("library.spaceWorld") },
  ]);
  const profileOptions = $derived<SelectOption<CinematicProfile>[]>([
    { value: "cinematic", label: t("studio.profile.cinematic"), description: t("studio.profile.cinematicDesc") },
    { value: "precision", label: t("studio.profile.precision"), description: t("studio.profile.precisionDesc") },
    { value: "organic", label: t("studio.profile.organic"), description: t("studio.profile.organicDesc") },
  ]);

  function setSeconds(seconds: number, nextUnit: DurationUnit = unit) {
    unit = nextUnit;
    valueInput = String(secondsToUnitValue(seconds, nextUnit));
    mmSsInput = formatSecondsToMmSs(seconds);
  }

  /** Cambiar de unidad o de modalidad conserva la duración real en segundos. */
  function currentSeconds() {
    return parsed.ok ? parsed.seconds : project.duration;
  }

  function reset() {
    const cin = project.cinematic ?? defaultCinematicConfig();
    const cal = project.calibration ?? defaultCalibrationConfig();
    name = project.name;
    inputMode = project.durationInputMode ?? "valueUnit";
    setSeconds(mode === "create" ? DEFAULT_DURATION_SEC : project.duration, project.durationUnit ?? "seconds");
    space = project.space ?? "anchor";
    scaleInput = String(project.trajectoryScale ?? 1);
    profile = cin.profile;
    calibrationEnabled = cal.enabled;
    autoCorrect = cal.autoCorrect;
    loop = project.loop ?? false;
    template = "blank";
    try {
      advOpen = localStorage.getItem(ADV_KEY) === "1";
    } catch {
      advOpen = false;
    }
    advSettled = advOpen;
  }

  function toggleAdvanced() {
    advOpen = !advOpen;
    advSettled = false;
    try {
      localStorage.setItem(ADV_KEY, advOpen ? "1" : "0");
    } catch {
      /* sin storage */
    }
  }

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) {
      untrack(reset);
      dialogEl.showModal();
      nameEl?.select();
    } else if (!open && dialogEl.open) {
      dialogEl.close();
    }
  });

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!canConfirm || !parsed.ok) return;
    onconfirm({
      name: name.trim(),
      seconds: parsed.seconds,
      unit,
      inputMode,
      space,
      trajectoryScale: scale,
      profile,
      calibrationEnabled,
      autoCorrect,
      loop,
      template,
    });
  }
</script>

<dialog
  bind:this={dialogEl}
  class="pm"
  aria-labelledby="pm-title"
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
  onclick={(e) => {
    if (e.target === dialogEl) onclose();
  }}
>
  <form class="pm-body" onsubmit={submit}>
    <header class="pm-head">
      <h3 id="pm-title">{t(mode === "create" ? "projectModal.createTitle" : "studio.config.title")}</h3>
      <button type="button" class="ghost icon" onclick={onclose} aria-label={t("studio.close")}>
        <X size={16} />
      </button>
    </header>

    <label class="field">
      <span class="label">{t("studio.projectName")}</span>
      <input type="text" bind:this={nameEl} bind:value={name} autocomplete="off" />
    </label>

    <div class="field">
      <div class="label-row">
        <span class="label">{t("studio.config.durationLabel")}</span>
        <div class="seg" role="tablist" aria-label={t("studio.config.inputMode")}>
          <button
            type="button"
            role="tab"
            aria-selected={inputMode === "valueUnit"}
            class:active={inputMode === "valueUnit"}
            onclick={() => {
              const s = currentSeconds();
              inputMode = "valueUnit";
              setSeconds(s);
            }}>{t("studio.config.modeValueUnit")}</button
          >
          <button
            type="button"
            role="tab"
            aria-selected={inputMode === "mmSs"}
            class:active={inputMode === "mmSs"}
            onclick={() => {
              const s = currentSeconds();
              inputMode = "mmSs";
              setSeconds(s);
            }}>{t("studio.config.modeMmSs")}</button
          >
        </div>
      </div>

      <div class="dur-row">
        {#if inputMode === "valueUnit"}
          <input
            type="number"
            step="any"
            aria-label={t("studio.config.durationLabel")}
            min={unit === "minutes" ? secondsToUnitValue(MIN_DURATION_SEC, "minutes") : MIN_DURATION_SEC}
            max={unit === "minutes" ? secondsToUnitValue(MAX_DURATION_SEC, "minutes") : MAX_DURATION_SEC}
            bind:value={valueInput}
          />
        {:else}
          <input
            type="text"
            aria-label={t("studio.config.mmSsLabel")}
            placeholder={t("studio.config.mmSsPlaceholder")}
            bind:value={mmSsInput}
          />
        {/if}
        <div class="unit">
          <IrisSelect
            items={unitOptions}
            value={unit}
            size="sm"
            ariaLabel={t("studio.config.unitLabel")}
            onchange={(next) => setSeconds(currentSeconds(), next as DurationUnit)}
          />
        </div>
      </div>

      <div class="quick" role="group" aria-label={t("studio.config.quickDurations")}>
        {#each QUICK_DURATIONS as sec (sec)}
          <button
            type="button"
            class:active={parsed.ok && Math.abs(parsed.seconds - sec) < 0.05}
            onclick={() => setSeconds(sec)}>{sec}s</button
          >
        {/each}
        <span class="equiv">{equivalence.mmSsText}</span>
      </div>

      {#if !parsed.ok}
        <p class="err" role="alert">
          {#if parsed.code === "secondsRange"}
            {t("studio.config.errSecondsRange")}
          {:else if parsed.code === "range"}
            {t("studio.config.errRange")}
          {:else}
            {t("studio.config.errInvalidNumber")}
          {/if}
        </p>
      {/if}
    </div>

    {#if mode === "create"}
      <div class="field">
        <span class="label">{t("projectModal.startFrom")}</span>
        <div class="seg wide" role="radiogroup" aria-label={t("projectModal.startFrom")}>
          <button
            type="button"
            role="radio"
            aria-checked={template === "blank"}
            class:active={template === "blank"}
            onclick={() => (template = "blank")}>{t("projectModal.blank")}</button
          >
          <button
            type="button"
            role="radio"
            aria-checked={template === "heroic"}
            class:active={template === "heroic"}
            onclick={() => (template = "heroic")}>{t("projectModal.heroic")}</button
          >
        </div>
      </div>
    {/if}

    {#if conflict}
      <div class="conflict" role="alertdialog" aria-label={t("studio.config.conflictTitle")}>
        <strong>{t("studio.config.conflictTitle")}</strong>
        <p>
          {t("studio.config.conflictDesc", {
            dur: `${conflict.targetSeconds} s (${formatSecondsToMmSs(conflict.targetSeconds)})`,
            kf: conflict.keyframesCount,
            fx: conflict.effectsCount,
          })}
        </p>
        <div class="conflict-actions">
          <button type="button" class="primary sm" onclick={() => onresolve("clamp")}>{t("studio.config.resolveClamp")}</button>
          <button type="button" class="ghost sm" onclick={() => onresolve("scale")}>{t("studio.config.resolveScale")}</button>
          <button type="button" class="ghost sm" onclick={() => onresolve("cancel")}>{t("studio.config.resolveCancel")}</button>
        </div>
      </div>
    {/if}

    <button
      type="button"
      class="adv-toggle"
      aria-expanded={advOpen}
      aria-controls="pm-advanced"
      onclick={toggleAdvanced}
    >
      <ChevronRight size={14} class={advOpen ? "chev open" : "chev"} />
      {t("projectModal.advanced")}
    </button>

    <div
      id="pm-advanced"
      class="adv"
      class:open={advOpen}
      class:settled={advSettled}
      ontransitionend={() => (advSettled = advOpen)}
    >
      <div class="adv-inner" inert={!advOpen}>
        <div class="adv-grid">
          <div class="field">
            <span class="label">{t("projectModal.space")}</span>
            <IrisSelect items={spaceOptions} bind:value={space} size="sm" ariaLabel={t("projectModal.space")} />
          </div>
          <label class="field">
            <span class="label">{t("projectModal.scale")}</span>
            <input type="number" step="0.05" min="0.05" max="10" bind:value={scaleInput} aria-invalid={!scaleOk} />
          </label>
          <div class="field span2">
            <span class="label">{t("studio.profileLabel")}</span>
            <IrisSelect items={profileOptions} bind:value={profile} size="sm" ariaLabel={t("studio.profileLabel")} />
          </div>
          <div class="checks span2">
            <IrisCheckbox variant="switch" size="sm" bind:checked={calibrationEnabled} label={t("projectModal.calibration")} />
            <IrisCheckbox variant="switch" size="sm" bind:checked={autoCorrect} label={t("projectModal.autoCorrect")} />
            <IrisCheckbox variant="switch" size="sm" bind:checked={loop} label={t("projectModal.loop")} />
          </div>
          <p class="engine span2"><span class="label">{t("projectModal.engine")}</span> {t("projectModal.engineValue")}</p>
        </div>
      </div>
    </div>

    <footer class="pm-foot">
      <button type="button" class="ghost" onclick={onclose}>{t("projectModal.cancel")}</button>
      <button type="submit" class="primary" disabled={!canConfirm}>
        {t(mode === "create" ? "projectModal.create" : "studio.config.apply")}
      </button>
    </footer>
  </form>
</dialog>

<style>
  .pm {
    width: min(520px, 92vw);
    max-height: 90vh;
    padding: 0;
    border: 0;
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    overflow: visible;
    box-shadow:
      0 0 0 1px var(--border-subtle),
      0 24px 64px -12px rgba(0, 0, 0, 0.7);
  }
  .pm[open] {
    animation: pm-in 160ms var(--ease-spring);
  }
  .pm::backdrop {
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(8px);
  }
  @keyframes pm-in {
    from {
      opacity: 0;
      transform: scale(0.98) translateY(6px);
    }
  }

  .pm-body {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px 22px;
  }
  .pm-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h3 {
    margin: 0;
    font-size: 1.05rem;
  }
  .icon {
    padding: 6px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .label {
    font-size: 0.7rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .label-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  input[type="text"],
  input[type="number"] {
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    font-family: var(--font-mono);
    font-size: 0.9rem;
  }
  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 0;
  }
  input[aria-invalid="true"] {
    outline: 2px solid var(--danger);
  }

  .dur-row {
    display: grid;
    grid-template-columns: 1fr 150px;
    gap: 8px;
  }

  .seg {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-md);
    background: var(--surface-raised);
  }
  .seg.wide {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .seg button,
  .quick button {
    padding: 4px 10px;
    font-size: 0.78rem;
    background: transparent;
    color: var(--text-2);
    border-radius: var(--radius-sm);
    justify-content: center;
  }
  .seg button:hover,
  .quick button:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .seg button.active,
  .quick button.active {
    background: var(--accent-subtle);
    color: var(--text);
  }

  .quick {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .equiv {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 0.8rem;
    color: var(--text-muted);
  }
  .err {
    margin: 0;
    font-size: 0.8rem;
    color: var(--danger);
  }

  .conflict {
    padding: 12px 14px;
    border-radius: var(--radius-md);
    background: var(--surface-raised);
    box-shadow: inset 3px 0 0 var(--warn);
    font-size: 0.84rem;
  }
  .conflict p {
    margin: 6px 0 10px;
    color: var(--text-2);
  }
  .conflict-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .adv-toggle {
    align-self: flex-start;
    padding: 4px 6px;
    margin-left: -6px;
    background: transparent;
    color: var(--text-2);
    font-size: 0.82rem;
    font-weight: 600;
  }
  .adv-toggle:hover {
    color: var(--text);
  }
  .adv-toggle :global(.chev) {
    transition: transform 200ms var(--ease);
  }
  .adv-toggle :global(.chev.open) {
    transform: rotate(90deg);
  }

  .adv {
    display: grid;
    grid-template-rows: 0fr;
    margin-top: -8px;
    transition: grid-template-rows 200ms var(--ease);
  }
  .adv.open {
    grid-template-rows: 1fr;
  }
  .adv-inner {
    min-height: 0;
    overflow: hidden;
  }
  /* Con el acordeón abierto del todo, los desplegables (IrisSelect) no deben quedar recortados. */
  .adv.open.settled .adv-inner {
    overflow: visible;
  }
  .adv-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    padding: 8px 0 2px;
  }
  .span2 {
    grid-column: span 2;
  }
  .checks {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .engine {
    margin: 0;
    font-size: 0.8rem;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }
  .engine .label {
    margin-right: 6px;
    font-family: inherit;
  }

  .pm-foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 4px;
  }
</style>
