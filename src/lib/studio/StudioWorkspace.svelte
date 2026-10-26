<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    Activity,
    Camera,
    CircleDot,
    Copy,
    Crosshair,
    Eye,
    Film,
    FolderOpen,
    Layers,
    Magnet,
    Orbit,
    Pause,
    Play,
    Plus,
    Radio,
    Redo2,
    Repeat,
    RotateCcw,
    Save,
    Search,
    Sliders,
    Sparkles,
    Square,
    Trash2,
    Undo2,
    Video,
    Wand2,
    ZoomIn,
    ChevronDown,
    ChevronRight,
    ChevronLeft,
    Clock,
    FastForward,
    Keyboard,
    Rewind,
    SkipBack,
    SkipForward,
  } from "@lucide/svelte";
  import {
    api,
    type CalibrationConfig,
    type CinematicConfig,
    type CinematicProfile,
    type DurationInputMode,
    type DurationUnit,
    type EffectClip,
    type EffectKind,
    type EvaluatedFrame,
    type Keyframe,
    type Motion,
    type Pose,
    type SequenceProject,
    type Status,
    type StudioPlaybackStatus,
    type TargetSourceKind,
    type TelemetryReport,
    type TrajectoryPreview,
    type Transition,
  } from "$lib/api";
  import { t, pointLabel, formatKeyframeLabel, localizeError, localizeMotionName, type Key } from "$lib/i18n/index.svelte";
  import {
    createDefaultSequenceProject,
    createEffectClip,
    defaultCalibrationConfig,
    defaultCinematicConfig,
    EFFECT_CATALOG,
    getEffectDesc,
    getEffectParamALabel,
    getEffectParamBLabel,
    type EffectCategory,
  } from "$lib/studio/catalog";
  import {
    adaptiveSnapStep,
    applyProjectDurationChange,
    DEFAULT_DURATION_SEC,
    detectOutOfRangeElements,
    formatDurationEquivalence,
    formatSecondsToMmSs,
    formatTimeReadout,
    generateTimelineTicks,
    MAX_DURATION_SEC,
    MIN_DURATION_SEC,
    parseMmSsInput,
    parseValueUnitInput,
    secondsToUnitValue,
    type DurationChangeResolution,
  } from "$lib/studio/duration";
  import SpatialViewport from "$lib/studio/SpatialViewport.svelte";
  import IrisSelect, { type SelectOption } from "$lib/components/ui/IrisSelect.svelte";
  import IrisCheckbox from "$lib/components/ui/IrisCheckbox.svelte";
  import IrisContextMenu, { type ContextMenuItem } from "$lib/components/ui/IrisContextMenu.svelte";
  import CurveEditorModal, { type BezierCurve } from "$lib/components/ui/CurveEditorModal.svelte";
  import KeybindingsModal from "$lib/components/ui/KeybindingsModal.svelte";
  import KeyframeQuickEditor from "$lib/studio/KeyframeQuickEditor.svelte";
  import { shortcutManager } from "$lib/shortcuts/shortcuts.svelte";

  let {
    status,
    sequences = $bindable<SequenceProject[]>([]),
    activeProject = $bindable<SequenceProject>(),
    run,
  }: {
    status: Status;
    sequences: SequenceProject[];
    activeProject: SequenceProject;
    run: (action: () => Promise<unknown>, okText?: string) => Promise<void>;
  } = $props();

  type StudioMode = "capture" | "editor";
  let studioMode = $state<StudioMode>("capture");

  // Cursor temporal y selección compartida entre Modo Captura y Modo Editor
  let playheadTime = $state(0);
  let selectedKeyframeId = $state<string | null>(null);
  let selectedEffectId = $state<string | null>(null);
  let inspectorTab = $state<"selection" | "calibration">("selection");

  // Historial Undo / Redo sobre SequenceProject
  let undoStack = $state<string[]>([]);
  let redoStack = $state<string[]>([]);

  // Portapapeles interno del Studio (Keyframes y Efectos)
  let studioClipboard = $state<
    | { type: "keyframe"; data: Keyframe }
    | { type: "effect"; data: EffectClip }
    | null
  >(null);

  // Evaluación matemática y vista previa espacial 3D
  let evaluatedFrame = $state<EvaluatedFrame | null>(null);
  let trajectoryPreview = $state<TrajectoryPreview | null>(null);
  let playbackStatus = $state<StudioPlaybackStatus>({
    state: "stopped",
    currentTime: 0,
    duration: 10,
    looping: false,
    speed: 1,
    liveOsc: false,
    frame: null,
    error: null,
  });

  // Opciones de captura y timeline
  let captureOptics = $state(true);
  let liveScrub = $state(false);
  let snapEnabled = $state(true);
  let timelineZoom = $state(1);
  let quickEffectKind = $state<EffectKind>("smoothZoom");
  let presetMotions = $state<Motion[]>([]);
  let selectedPresetMotion = $state<string>("Órbita 360°");
  let telemetry = $state<TelemetryReport | null>(null);

  const sequenceSelectOptions = $derived<SelectOption[]>([
    ...sequences.map((s) => ({
      value: s.id,
      label: s.name,
      icon: Film,
      description: `${s.keyframes.length} ${t("library.pointsCount", { count: s.keyframes.length })} · ${s.duration}s`,
    })),
    ...(!sequences.some((s) => s.id === activeProject.id)
      ? [
          {
            value: activeProject.id,
            label: `${activeProject.name} *`,
            icon: Film,
            description: t("studio.unsaved"),
          },
        ]
      : []),
  ]);

  const cinematicProfileOptions = $derived<SelectOption[]>([
    { value: "cinematic", label: t("studio.profile.cinematic"), description: t("studio.profile.cinematicDesc") },
    { value: "precision", label: t("studio.profile.precision"), description: t("studio.profile.precisionDesc") },
    { value: "organic", label: t("studio.profile.organic"), description: t("studio.profile.organicDesc") },
  ]);

  const presetMotionOptions = $derived<SelectOption[]>(
    presetMotions.map((m) => ({
      value: m.name,
      label: localizeMotionName(m.name),
      icon: Orbit,
      description: `${m.duration}s · ${t(`motion.loop.${m.loop}` as Key)}`,
    }))
  );

  const configUnitOptions = $derived<SelectOption<DurationUnit>[]>([
    { value: "seconds", label: `${t("studio.config.unitSeconds")} (s)` },
    { value: "minutes", label: `${t("studio.config.unitMinutes")} (min)` },
  ]);

  const transitionOptions = $derived<SelectOption<Transition>[]>([
    { value: "linear", label: `${t("studio.trans.linear")} — Linear` },
    { value: "smooth", label: `${t("studio.trans.smooth")} — Cosine In-Out` },
    { value: "slowIn", label: `${t("studio.trans.slowIn")} — Cubic Ease-In` },
    { value: "slowOut", label: `${t("studio.trans.slowOut")} — Cubic Ease-Out` },
    { value: "smoothInOut", label: `${t("studio.trans.smoothInOut")} — Cubic S-Curve` },
  ]);

  const knotModeOptions = $derived<SelectOption<"passThrough" | "stop">[]>([
    { value: "passThrough", label: t("studio.knotMode.passThrough") },
    { value: "stop", label: t("studio.knotMode.stop") },
  ]);

  const fxTrackOptions = $derived<SelectOption<string>[]>([
    { value: "0", label: t("studio.trackFx1") },
    { value: "1", label: t("studio.trackFx2") },
    { value: "2", label: t("studio.trackFx3") },
  ]);

  const referenceSpaceOptions = $derived<SelectOption<"anchor" | "world">[]>([
    { value: "anchor", label: t("library.spaceAnchor") },
    { value: "world", label: t("library.spaceWorld") },
  ]);

  const targetSourceOptions = $derived<SelectOption<string>[]>([
    { value: "calibratedAnchor", label: "Ojos del jugador (Ancla)" },
    { value: "staticCalibratedPoint", label: "Punto 3D Estático" },
    { value: "manualReference", label: t("studio.targetSource.manual") },
  ]);

  const quickEffectKindOptions = $derived<SelectOption<EffectKind>[]>(
    EFFECT_CATALOG.map((item) => ({
      value: item.kind,
      label: t(item.labelKey),
      description: t(item.descKey),
    }))
  );

  // Estados de Modales y Menús Interactivos Modernos
  let showKeybindingsModal = $state(false);
  let showCurveEditorModal = $state(false);
  let curveEditorTarget = $state<{ type: "keyframe" | "effect"; id: string; curve: BezierCurve } | null>(null);
  let quickEditorState = $state<{ open: boolean; keyframe: Keyframe | null; x: number; y: number }>({
    open: false,
    keyframe: null,
    x: 0,
    y: 0,
  });
  let contextMenu = $state<{
    open: boolean;
    x: number;
    y: number;
    items: ContextMenuItem[];
  }>({
    open: false,
    x: 0,
    y: 0,
    items: [],
  });
  let draggingLibraryEffect = $state<{ kind: EffectKind; x: number; y: number } | null>(null);
  let hoveredFxTrack = $state<number | null>(null);
  let fxLaneEls = $state<(HTMLElement | null)[]>([null, null, null]);
  let inspectorAccordion = $state({
    pos: true,
    rot: true,
    optics: true,
    space: true,
  });

  // Estado de la sección «Configuración del proyecto»
  let showProjectConfig = $state(false);
  let configInputMode = $state<DurationInputMode>("valueUnit");
  let configUnit = $state<DurationUnit>("seconds");
  let configValueInput = $state<string>("10");
  let configMmSsInput = $state<string>("00:10");
  let pendingDurationConflict = $state<{
    targetSeconds: number;
    keyframesCount: number;
    effectsCount: number;
  } | null>(null);
  let lastSyncedDurationKey = "";

  const defaultCinematic: CinematicConfig = defaultCinematicConfig();
  const defaultCalibration: CalibrationConfig = defaultCalibrationConfig();

  function worldToAnchorLocal(
    worldPos: [number, number, number],
    anchor: { eye: [number, number, number]; yaw: number },
  ): [number, number, number] {
    const dx = worldPos[0] - anchor.eye[0];
    const dy = worldPos[1] - anchor.eye[1];
    const dz = worldPos[2] - anchor.eye[2];
    const a = (anchor.yaw * Math.PI) / 180;
    const sin = Math.sin(a);
    const cos = Math.cos(a);
    const localX = dx * cos - dz * sin;
    const localZ = dx * sin + dz * cos;
    return [
      Math.round(localX * 1000) / 1000,
      Math.round(dy * 1000) / 1000,
      Math.round(localZ * 1000) / 1000,
    ];
  }

  function ensureProjectConfigs() {
    if (!Number.isFinite(activeProject.duration) || activeProject.duration <= 0) {
      activeProject.duration = DEFAULT_DURATION_SEC;
    }
    if (!activeProject.durationUnit) {
      activeProject.durationUnit = "seconds";
    }
    if (!activeProject.durationInputMode) {
      activeProject.durationInputMode = "valueUnit";
    }
    if (!activeProject.cinematic) {
      activeProject.cinematic = structuredClone(defaultCinematic);
    } else if (!Number.isFinite(activeProject.cinematic.organicVariation)) {
      activeProject.cinematic.organicVariation = 0.0;
    }
    if (!activeProject.calibration) {
      activeProject.calibration = structuredClone(defaultCalibration);
    }
    if (!Array.isArray(activeProject.keyframes)) {
      activeProject.keyframes = [];
    }
    for (const kf of activeProject.keyframes) {
      if (!kf.look) {
        kf.look = { zoom: 45, exposure: null, focalDistance: null, aperture: null };
      }
    }
    if (!Array.isArray(activeProject.effects)) {
      activeProject.effects = [];
    }
  }

  function syncActiveProjectToLibrary(proj: SequenceProject) {
    const idx = sequences.findIndex((s) => s.id === proj.id);
    if (idx >= 0) {
      sequences[idx] = $state.snapshot(proj);
    }
  }

  /** Abre `proj` como proyecto activo. `proj` suele ser un `$state` (Proxy): `structuredClone` lanza DataCloneError con él. */
  function openProject(proj: SequenceProject) {
    // Guardar en memoria las ediciones del proyecto que se abandona (el effect solo sincroniza cambios estructurales).
    if (proj.id !== activeProject.id) syncActiveProjectToLibrary(activeProject);
    activeProject = $state.snapshot(proj) as SequenceProject;
    selectedKeyframeId = activeProject.keyframes[0]?.id ?? null;
    selectedEffectId = null;
    playheadTime = 0;
    undoStack = [];
    redoStack = [];
    evaluatedFrame = null;
    trajectoryPreview = null;
  }

  function applyCinematicProfile(profile: CinematicProfile) {
    pushUndoSnapshot();
    ensureProjectConfigs();
    if (profile === "precision") {
      activeProject.cinematic = {
        profile: "precision",
        inertiaTau: 0.0,
        maxLinearSpeed: 12.0,
        maxLinearAccel: 25.0,
        maxAngularSpeed: 360.0,
        organicVariation: 0.0,
        organicSeed: 42,
      };
    } else if (profile === "organic") {
      activeProject.cinematic = {
        profile: "organic",
        inertiaTau: 0.24,
        maxLinearSpeed: 3.5,
        maxLinearAccel: 6.0,
        maxAngularSpeed: 140.0,
        organicVariation: 0.35,
        organicSeed: 42,
      };
    } else {
      activeProject.cinematic = structuredClone(defaultCinematic);
    }
    void refreshMathPreview(false);
  }

  async function loadPresetMotionIntoStudio(name: string) {
    await run(async () => {
      const seq = await api.loadMotionAsSequence(name);
      pushUndoSnapshot();
      activeProject = seq;
      selectedKeyframeId = seq.keyframes[0]?.id ?? null;
      selectedEffectId = null;
      playheadTime = 0;
      await refreshMathPreview(false);
    }, t("studio.loadedMotion", { name }));
  }

  async function restoreCalibrationDefaults() {
    pushUndoSnapshot();
    activeProject.calibration = structuredClone(defaultCalibration);
    telemetry = await api.resetCalibration().catch(() => null);
    await refreshMathPreview(false);
  }

  // Modo Captura Armada (cuando la cámara está abierta pero VRChat aún no ha emitido Pose fresca)
  let captureArmed = $state(false);
  let armedBaselineSeq = $state(0);

  // Grabación continua de recorrido (Modalidad B)
  let recordingActive = $state(false);
  let recordingElapsed = $state(0);
  let recordingSamples = $state(0);
  let posTolM = $state(0.03);
  let rotTolDeg = $state(3.5);
  let smoothingPasses = $state(2);

  // Biblioteca izquierda del Modo Editor
  let libraryCategory = $state<EffectCategory | "presets">("movement");
  let librarySearch = $state("");

  // Arrastre interactivo en el timeline
  let timelineTrackEl = $state<HTMLElement | null>(null);
  let dragState = $state<
    | { kind: "playhead" }
    | { kind: "keyframe"; id: string }
    | { kind: "keyframe-stretch"; id: string; nextId: string; origNextTime: number }
    | { kind: "effect-move"; id: string; offsetSec: number }
    | { kind: "effect-resize"; id: string }
    | null
  >(null);

  const selectedKeyframe = $derived(
    activeProject.keyframes.find((k) => k.id === selectedKeyframeId) ?? null,
  );
  const selectedEffect = $derived(
    activeProject.effects.find((e) => e.id === selectedEffectId) ?? null,
  );

  // Detecta si la secuencia contiene ajustes avanzados para mostrar el aviso no destructivo en Modo Captura
  const hasAdvancedFeatures = $derived(
    Boolean(activeProject.target?.enabled) ||
      Boolean(activeProject.calibration?.autoCorrect) ||
      Boolean(activeProject.calibration?.posOffset.some((v) => Math.abs(v) > 1e-4)) ||
      Math.abs(activeProject.calibration?.yawOffsetDeg ?? 0) > 1e-4 ||
      Math.abs(activeProject.calibration?.pitchOffsetDeg ?? 0) > 1e-4 ||
      Math.abs(activeProject.calibration?.compositionX ?? 0) > 1e-4 ||
      Math.abs(activeProject.calibration?.compositionY ?? 0) > 1e-4 ||
      activeProject.effects.some((e) => e.track > 0) ||
      activeProject.keyframes.some(
        (k) =>
          k.look?.zoom != null ||
          k.look?.exposure != null ||
          k.look?.focalDistance != null ||
          k.look?.aperture != null,
      ),
  );

  const lastKeyframeTime = $derived(
    activeProject.keyframes.length > 0
      ? Math.max(...activeProject.keyframes.map((k) => k.time))
      : 0,
  );
  const keyframesNeedFitting = $derived(
    activeProject.keyframes.length >= 2 &&
      lastKeyframeTime < activeProject.duration - 0.75,
  );

  const filteredCatalog = $derived(
    EFFECT_CATALOG.filter((item) => {
      if (libraryCategory !== "presets" && item.category !== libraryCategory) return false;
      if (!librarySearch.trim()) return true;
      const q = librarySearch.toLowerCase();
      return (
        t(item.labelKey).toLowerCase().includes(q) ||
        getEffectDesc(item).toLowerCase().includes(q) ||
        item.kind.toLowerCase().includes(q)
      );
    }),
  );

  function pushUndoSnapshot() {
    const snap = JSON.stringify(activeProject);
    if (undoStack.length > 0 && undoStack[undoStack.length - 1] === snap) return;
    undoStack = [...undoStack.slice(-39), snap];
    redoStack = [];
  }

  function undo() {
    if (undoStack.length === 0) return;
    const prev = undoStack[undoStack.length - 1];
    undoStack = undoStack.slice(0, -1);
    redoStack = [...redoStack, JSON.stringify(activeProject)];
    activeProject = JSON.parse(prev);
    refreshMathPreview(false);
  }

  function redo() {
    if (redoStack.length === 0) return;
    const next = redoStack[redoStack.length - 1];
    redoStack = redoStack.slice(0, -1);
    undoStack = [...undoStack, JSON.stringify(activeProject)];
    activeProject = JSON.parse(next);
    refreshMathPreview(false);
  }

  function snapTime(sec: number): number {
    const clamped = Math.max(0, Math.min(activeProject.duration, sec));
    if (!snapEnabled) return Math.round(clamped * 100) / 100;
    const step = adaptiveSnapStep(
      activeProject.duration,
      studioMode === "editor" ? timelineZoom : 1,
    );
    return Math.round(clamped / step) * step;
  }

  async function refreshMathPreview(sendOsc = false) {
    try {
      const sampleSteps = Math.min(300, Math.max(90, Math.round(activeProject.duration * 3)));
      const [ev, prev] = await Promise.all([
        api.evaluateSequence(activeProject, playheadTime, sendOsc),
        api.previewTrajectory(activeProject, sampleSteps),
      ]);
      evaluatedFrame = ev;
      trajectoryPreview = prev;
    } catch {
      /* backend en inicialización */
    }
  }

  // Sincronizar campos de Configuración del proyecto cuando cambie el proyecto activo o su duración
  $effect(() => {
    const unit = activeProject.durationUnit ?? "seconds";
    const mode = activeProject.durationInputMode ?? "valueUnit";
    const syncKey = `${activeProject.id}:${activeProject.duration}:${unit}:${mode}`;
    if (syncKey !== lastSyncedDurationKey) {
      lastSyncedDurationKey = syncKey;
      configUnit = unit;
      configInputMode = mode;
      configValueInput = String(secondsToUnitValue(activeProject.duration, unit));
      configMmSsInput = formatSecondsToMmSs(activeProject.duration);
      pendingDurationConflict = null;
    }
  });

  // Re-evaluar vista previa cuando cambie el proyecto y mantener sincronizada la copia en memoria
  $effect(() => {
    void activeProject.id;
    ensureProjectConfigs();
    void activeProject.duration;
    void activeProject.durationUnit;
    void activeProject.durationInputMode;
    void activeProject.keyframes.length;
    void activeProject.effects.length;
    void activeProject.space;
    void activeProject.trajectoryScale;
    // `syncActiveProjectToLibrary` lee y escribe `sequences`: sin `untrack` este effect se re-dispara a sí mismo en bucle.
    untrack(() => syncActiveProjectToLibrary(activeProject));
    refreshMathPreview(false);
  });

  // El path del rig (5 huesos en VRChat) debe reflejar SIEMPRE los puntos del proyecto: se resincroniza solo cuando cambia
  // lo que lo define (keyframes, tiempos, duración, espacio, escala o el ancla del jugador) y al cambiar de proyecto.
  // Antes solo se sincronizaba al arrastrar un punto o con los botones de Ajustes. Si el rig no está anclado el backend
  // responde con un error que se ignora. La sincronización tarda ~3 s (parámetros synced); el backend descarta las viejas.
  let pathSyncTimer: ReturnType<typeof setTimeout> | null = null;
  const PATH_RESYNC_MS = 30000;
  // `$derived` de texto: `status` se reemplaza en cada sondeo (750 ms) y, leído dentro del effect, reiniciaría el debounce
  // sin parar; así el effect solo corre cuando cambia el CONTENIDO de la firma.
  const pathSignature = $derived(
    [
      activeProject.id,
      activeProject.duration,
      activeProject.space,
      activeProject.trajectoryScale,
      status.anchor ? `${status.anchor.eye.join(",")}@${status.anchor.yaw}` : "",
      ...activeProject.keyframes.map(
        (k) => `${k.time}|${k.rawPose.pos.join(",")}|${k.localPose?.pos?.join(",") ?? ""}|${k.transition}|${k.knotMode}`,
      ),
    ].join(";"),
  );
  $effect(() => {
    void pathSignature;
    if (pathSyncTimer) clearTimeout(pathSyncTimer);
    pathSyncTimer = setTimeout(() => {
      void api.visualizerSyncPath($state.snapshot(activeProject) as SequenceProject).catch(() => {});
    }, 800);
    return () => {
      if (pathSyncTimer) clearTimeout(pathSyncTimer);
    };
  });

  // Reenvío periódico: los remotos reciben el path por el bus de red (sin canal de retorno, en bucle abierto), así que un jugador
  // que entra tarde o un paquete perdido dejan el path viejo. Reenviar los mismos puntos no mueve nada y lo cura en el siguiente
  // reenvío (≤ ~30 s + 12 s de ciclo). Con el rig apagado el backend responde con error, que se ignora.
  $effect(() => {
    const timer = setInterval(() => {
      void api.visualizerSyncPath($state.snapshot(activeProject) as SequenceProject).catch(() => {});
    }, PATH_RESYNC_MS);
    return () => clearInterval(timer);
  });

  // Vigilar Captura Armada: en cuanto llegue un nuevo paquete `/usercamera/Pose` de VRChat, capturar automáticamente
  $effect(() => {
    const seq = status.camera?.poseSeq ?? 0;
    const fresh = status.camera?.poseFresh ?? false;
    if (captureArmed && fresh && seq > armedBaselineSeq) {
      captureArmed = false;
      void executeCaptureFromCamera();
    }
  });

  function defaultTimeForNewPoint(): number {
    if (activeProject.keyframes.length === 0) return 0;
    const existsAtPlayhead = activeProject.keyframes.some(
      (k) => Math.abs(k.time - playheadTime) < 0.08,
    );
    if (!existsAtPlayhead) return playheadTime;
    const lastT = Math.max(...activeProject.keyframes.map((k) => k.time));
    if (lastT + 2.5 <= activeProject.duration) {
      return Math.round((lastT + 2.5) * 10) / 10;
    }
    return activeProject.duration;
  }

  async function executeCaptureFromCamera() {
    const targetTime = defaultTimeForNewPoint();
    const nextIdx = activeProject.keyframes.length;
    await run(async () => {
      const kf = await api.captureCameraPoint(nextIdx, targetTime, captureOptics);
      pushUndoSnapshot();
      const nextKfs = [...activeProject.keyframes, kf].sort((a, b) => a.time - b.time);
      activeProject = {
        ...activeProject,
        anchorSnapshot: activeProject.anchorSnapshot ?? status.anchor,
        keyframes: nextKfs,
      };
      selectedKeyframeId = kf.id;
      selectedEffectId = null;
      playheadTime = kf.time;
      await refreshMathPreview(false);
    }, t("toast.pointCaptured", { label: pointLabel(nextIdx), time: targetTime.toFixed(1) }));
  }

  async function handleCaptureButton() {
    if (captureArmed) {
      captureArmed = false;
      return;
    }
    // Si VRChat está conectado y la cámara abierta pero aún no ha emitido Pose fresca, armamos la captura automática
    if (status.connected && status.open && !status.camera?.poseFresh) {
      armedBaselineSeq = status.camera?.poseSeq ?? 0;
      captureArmed = true;
      return;
    }
    await executeCaptureFromCamera();
  }

  // Punto de prueba local (explícitamente marcado con capturedAtMs = 0 para no fingir lectura de VRChat)
  function addLocalTestPoint() {
    pushUndoSnapshot();
    const idx = activeProject.keyframes.length;
    const targetTime = defaultTimeForNewPoint();
    const angle = idx * 0.65 - 0.6;
    const radius = 1.6;
    const anchorEye = status.anchor?.eye ?? activeProject.anchorSnapshot?.eye ?? [0, 1.65, 0];
    const localPos: [number, number, number] = [
      Math.round(Math.sin(angle) * radius * 100) / 100,
      Math.round((idx % 2 === 0 ? 0.05 : 0.25) * 100) / 100,
      Math.round(Math.cos(angle) * radius * 100) / 100,
    ];
    const yawDeg = Math.round(((angle * 180) / Math.PI + 180) % 360) - 180;
    const kf: Keyframe = {
      id: `kf-sim-${Date.now()}-${idx}`,
      label: pointLabel(idx),
      time: targetTime,
      rawPose: {
        pos: [anchorEye[0] + localPos[0], anchorEye[1] + localPos[1], anchorEye[2] + localPos[2]],
        rot: [0, yawDeg, 0],
      },
      localPose: {
        pos: localPos,
        rot: [0, yawDeg, 0],
      },
      capturedAtMs: 0,
      transition: "smooth",
      knotMode: "passThrough",
      look: { zoom: 45, exposure: null, focalDistance: null, aperture: null },
    };
    const nextKfs = [...activeProject.keyframes, kf].sort((a, b) => a.time - b.time);
    activeProject = { ...activeProject, keyframes: nextKfs };
    selectedKeyframeId = kf.id;
    selectedEffectId = null;
    playheadTime = kf.time;
    void refreshMathPreview(false);
  }

  async function updateSelectedKeyframeFromCamera() {
    if (!selectedKeyframe) return;
    const targetId = selectedKeyframe.id;
    const idx = activeProject.keyframes.findIndex((k) => k.id === targetId);
    if (idx < 0) return;
    await run(async () => {
      const freshKf = await api.captureCameraPoint(idx, selectedKeyframe.time, captureOptics);
      pushUndoSnapshot();
      activeProject.keyframes[idx] = {
        ...selectedKeyframe,
        rawPose: freshKf.rawPose,
        localPose: freshKf.localPose,
        capturedAtMs: freshKf.capturedAtMs,
        look: captureOptics ? freshKf.look : selectedKeyframe.look,
      };
      activeProject = { ...activeProject };
      await refreshMathPreview(false);
    }, t("toast.pointUpdated", { label: selectedKeyframe.label }));
  }

  async function duplicateSelectedKeyframe() {
    if (!selectedKeyframe) return;
    pushUndoSnapshot();
    const newTime = Math.min(activeProject.duration, Math.round((selectedKeyframe.time + 1.5) * 10) / 10);
    let sampledPose: Pose = $state.snapshot(selectedKeyframe.rawPose);
    try {
      sampledPose = await api.sampleBasePoseAt(activeProject, newTime);
    } catch {
      // fallback a evaluatedFrame si está disponible
      if (evaluatedFrame?.basePose) {
        sampledPose = $state.snapshot(evaluatedFrame.basePose);
      }
    }

    // Si el tiempo duplicado está en o después del último keyframe o el pose es idéntico,
    // desplazamos suavemente la pose a lo largo de la tangente hacia adelante de la cámara
    // para evitar un plateau de posición congelada mientras el zoom cambia
    const isAtEnd = newTime >= lastKeyframeTime;
    const isIdentical =
      Math.abs(sampledPose.pos[0] - selectedKeyframe.rawPose.pos[0]) < 1e-3 &&
      Math.abs(sampledPose.pos[1] - selectedKeyframe.rawPose.pos[1]) < 1e-3 &&
      Math.abs(sampledPose.pos[2] - selectedKeyframe.rawPose.pos[2]) < 1e-3;

    if (isAtEnd || isIdentical) {
      const yawRad = (selectedKeyframe.rawPose.rot[1] * Math.PI) / 180;
      sampledPose.pos = [
        Math.round((sampledPose.pos[0] + Math.sin(yawRad) * 0.5) * 100) / 100,
        sampledPose.pos[1],
        Math.round((sampledPose.pos[2] + Math.cos(yawRad) * 0.5) * 100) / 100,
      ];
    }

    const copy: Keyframe = {
      ...$state.snapshot(selectedKeyframe),
      id: `kf-dup-${Date.now()}`,
      label: `${selectedKeyframe.label} ${t("studio.copySuffix")}`,
      time: newTime,
      rawPose: sampledPose,
      localPose:
        activeProject.space === "anchor" && activeProject.anchorSnapshot
          ? {
              pos: worldToAnchorLocal(sampledPose.pos, activeProject.anchorSnapshot),
              rot: [sampledPose.rot[0], sampledPose.rot[1], sampledPose.rot[2]],
            }
          : null,
      look: selectedKeyframe.look
        ? $state.snapshot(selectedKeyframe.look)
        : { zoom: 45, exposure: null, focalDistance: null, aperture: null },
    };
    activeProject.keyframes = [...activeProject.keyframes, copy].sort((a, b) => a.time - b.time);
    selectedKeyframeId = copy.id;
    playheadTime = copy.time;
    void refreshMathPreview(false);
  }

  async function fitKeyframesToDuration() {
    if (activeProject.keyframes.length < 2) return;
    pushUndoSnapshot();
    await run(async () => {
      activeProject = await api.fitKeyframesToDuration(activeProject, activeProject.duration);
      await refreshMathPreview(false);
    }, t("studio.toast.keyframesFitted"));
  }

  function deleteSelectedKeyframe() {
    if (!selectedKeyframeId) return;
    pushUndoSnapshot();
    activeProject.keyframes = activeProject.keyframes.filter((k) => k.id !== selectedKeyframeId);
    selectedKeyframeId = activeProject.keyframes[0]?.id ?? null;
    void refreshMathPreview(false);
  }

  const parsedConfigDuration = $derived(
    configInputMode === "valueUnit"
      ? parseValueUnitInput(configValueInput, configUnit)
      : parseMmSsInput(configMmSsInput),
  );

  const effectiveDraftSeconds = $derived(
    parsedConfigDuration.ok ? parsedConfigDuration.seconds : activeProject.duration,
  );

  const draftEquivalence = $derived(
    formatDurationEquivalence(effectiveDraftSeconds, configUnit),
  );

  const captureTimelineTicks = $derived(generateTimelineTicks(activeProject.duration, 1));
  const editorTimelineTicks = $derived(
    generateTimelineTicks(activeProject.duration, timelineZoom),
  );

  /**
   * Al cambiar la unidad (segundos <-> minutos), conserva la duración real de la toma
   * convirtiendo el valor mostrado en lugar de modificar la duración en segundos.
   */
  function switchConfigUnit(nextUnit: DurationUnit) {
    if (nextUnit === configUnit) return;
    const currentParsed = parseValueUnitInput(configValueInput, configUnit);
    const realSeconds = currentParsed.ok ? currentParsed.seconds : activeProject.duration;
    configUnit = nextUnit;
    configValueInput = String(secondsToUnitValue(realSeconds, nextUnit));
    configMmSsInput = formatSecondsToMmSs(realSeconds);
    activeProject = {
      ...activeProject,
      durationUnit: nextUnit,
    };
  }

  /**
   * Al cambiar entre modalidades de entrada (Valor y unidad <-> MM:SS),
   * conserva la duración real en segundos sin reiniciarla ni redondearla innecesariamente.
   */
  function switchConfigInputMode(nextMode: DurationInputMode) {
    if (nextMode === configInputMode) return;
    const realSeconds = parsedConfigDuration.ok
      ? parsedConfigDuration.seconds
      : activeProject.duration;
    configInputMode = nextMode;
    configValueInput = String(secondsToUnitValue(realSeconds, configUnit));
    configMmSsInput = formatSecondsToMmSs(realSeconds);
    activeProject = {
      ...activeProject,
      durationInputMode: nextMode,
    };
  }

  function commitDurationChange(targetSeconds: number, resolution: DurationChangeResolution) {
    pushUndoSnapshot();
    const updated = applyProjectDurationChange(
      activeProject,
      targetSeconds,
      configUnit,
      configInputMode,
      resolution,
    );
    activeProject = updated;
    syncActiveProjectToLibrary(updated);
    if (playheadTime > updated.duration) {
      playheadTime = updated.duration;
    }
    configValueInput = String(secondsToUnitValue(updated.duration, configUnit));
    configMmSsInput = formatSecondsToMmSs(updated.duration);
    pendingDurationConflict = null;
    void run(
      async () => {
        await refreshMathPreview(false);
      },
      t("studio.config.applied", {
        dur: `${updated.duration} s (${formatSecondsToMmSs(updated.duration)})`,
      }),
    );
  }

  function requestDurationChange(targetSeconds: number) {
    if (!Number.isFinite(targetSeconds)) return;
    const clampedTarget = Math.max(MIN_DURATION_SEC, Math.min(MAX_DURATION_SEC, targetSeconds));
    if (Math.abs(clampedTarget - activeProject.duration) < 1e-4) {
      activeProject = {
        ...activeProject,
        durationUnit: configUnit,
        durationInputMode: configInputMode,
      };
      pendingDurationConflict = null;
      return;
    }
    // Al reducir la duración, verificar si existen keyframes o movimientos que queden fuera del nuevo rango
    if (clampedTarget < activeProject.duration - 1e-4) {
      const conflicts = detectOutOfRangeElements(activeProject, clampedTarget);
      if (conflicts.hasConflicts) {
        pendingDurationConflict = {
          targetSeconds: clampedTarget,
          keyframesCount: conflicts.keyframes.length,
          effectsCount: conflicts.effects.length,
        };
        showProjectConfig = true;
        return;
      }
    }
    // Al aumentar la duración (o reducir sin conflictos), preservar intactos todos los tiempos originales
    commitDurationChange(clampedTarget, "preserve");
  }

  function handleApplyProjectConfig() {
    if (!parsedConfigDuration.ok) return;
    requestDurationChange(parsedConfigDuration.seconds);
  }

  function addEffectToTimeline(kind: EffectKind, customStartTime?: number, preferredTrack?: number) {
    pushUndoSnapshot();
    const entry = EFFECT_CATALOG.find((e) => e.kind === kind);
    const defaultTrack =
      preferredTrack ??
      (entry?.category === "optics" ? 0 : entry?.category === "movement" ? 0 : 1);
    const start = customStartTime !== undefined ? customStartTime : playheadTime;
    const clip = createEffectClip(kind, start, activeProject.duration, defaultTrack);
    activeProject = {
      ...activeProject,
      effects: [...activeProject.effects, clip],
    };
    if (playheadTime < clip.startTime || playheadTime >= clip.startTime + clip.duration) {
      playheadTime = clip.startTime;
    }
    selectedEffectId = clip.id;
    selectedKeyframeId = null;
    inspectorTab = "selection";
    void refreshMathPreview(liveScrub);
  }

  let pathSyncDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleVisualizerPathSync() {
    if (pathSyncDebounceTimer) {
      clearTimeout(pathSyncDebounceTimer);
    }
    pathSyncDebounceTimer = setTimeout(() => {
      pathSyncDebounceTimer = null;
      void api.visualizerSyncPath(activeProject).catch(() => {});
    }, 120);
  }

  function updateKeyframePos(id: string, newPos: [number, number, number]) {
    const kf = activeProject.keyframes.find((k) => k.id === id);
    if (!kf) return;
    kf.rawPose.pos = [newPos[0], newPos[1], newPos[2]];
    if (activeProject.space === "anchor" && activeProject.anchorSnapshot) {
      kf.localPose = {
        pos: worldToAnchorLocal(newPos, activeProject.anchorSnapshot),
        rot: [kf.rawPose.rot[0], kf.rawPose.rot[1], kf.rawPose.rot[2]],
      };
    }
    void refreshMathPreview(false);

    // Actualización de la cámara en tiempo real
    void api.visualizerSyncCamera(kf.rawPose).catch(() => {});

    // Debounce / Throttling del path: el dueño lo ve al instante y el bus de red lo manda a los remotos sin solapar envíos
    scheduleVisualizerPathSync();
  }

  function openKeyframeContextMenu(kf: Keyframe, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    selectedKeyframeId = kf.id;
    selectedEffectId = null;
    inspectorTab = "selection";
    contextMenu = {
      open: true,
      x: e.clientX,
      y: e.clientY,
      items: [
        {
          label: `${t("studio.ctx.editProps")} (${formatKeyframeLabel(kf.label)})`,
          icon: Sliders,
          action: () => {
            quickEditorState = { open: true, keyframe: kf, x: e.clientX, y: e.clientY };
          },
        },
        {
          label: t("studio.ctx.customCurve"),
          icon: Sparkles,
          action: () => {
            curveEditorTarget = {
              type: "keyframe",
              id: kf.id,
              curve: (kf as any).customCurve ?? [0.4, 0.0, 0.2, 1.0],
            };
            showCurveEditorModal = true;
          },
        },
        {
          label: t("studio.ctx.updatePose"),
          icon: Camera,
          disabled: !status.connected || !status.open,
          action: updateSelectedKeyframeFromCamera,
        },
        {
          label: t("menu.copyPoint"),
          icon: Copy,
          shortcut: "Ctrl+C",
          action: copySelected,
        },
        {
          label: t("menu.paste"),
          icon: FolderOpen,
          shortcut: "Ctrl+V",
          disabled: !studioClipboard,
          action: pasteSelected,
        },
        {
          label: t("studio.ctx.duplicate"),
          icon: Copy,
          shortcut: "Ctrl+D",
          action: duplicateSelectedKeyframe,
        },
        { separator: true },
        {
          label: t("studio.ctx.delete"),
          icon: Trash2,
          shortcut: "Delete",
          danger: true,
          action: deleteSelectedKeyframe,
        },
      ],
    };
  }

  function openEffectContextMenu(ef: EffectClip, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    selectedEffectId = ef.id;
    selectedKeyframeId = null;
    inspectorTab = "selection";
    const entry = EFFECT_CATALOG.find((c) => c.kind === ef.kind);
    contextMenu = {
      open: true,
      x: e.clientX,
      y: e.clientY,
      items: [
        {
          label: `${entry ? t(entry.labelKey) : ef.kind} (${ef.enabled ? t("studio.action.deactivate") : t("studio.action.activate")})`,
          icon: Sparkles,
          action: () => {
            ef.enabled = !ef.enabled;
            void refreshMathPreview(false);
          },
        },
        {
          label: t("studio.ctx.customCurve"),
          icon: Sparkles,
          action: () => {
            curveEditorTarget = {
              type: "effect",
              id: ef.id,
              curve: (ef as any).customCurve ?? [0.4, 0.0, 0.2, 1.0],
            };
            showCurveEditorModal = true;
          },
        },
        {
          label: `${t("studio.ctx.moveToTrack")} 1`,
          action: () => {
            ef.track = 0;
            void refreshMathPreview(false);
          },
        },
        {
          label: `${t("studio.ctx.moveToTrack")} 2`,
          action: () => {
            ef.track = 1;
            void refreshMathPreview(false);
          },
        },
        {
          label: `${t("studio.ctx.moveToTrack")} 3`,
          action: () => {
            ef.track = 2;
            void refreshMathPreview(false);
          },
        },
        {
          label: t("menu.copyEffect"),
          icon: Copy,
          shortcut: "Ctrl+C",
          action: copySelected,
        },
        {
          label: t("menu.paste"),
          icon: FolderOpen,
          shortcut: "Ctrl+V",
          disabled: !studioClipboard,
          action: pasteSelected,
        },
        {
          label: t("studio.ctx.duplicate"),
          icon: Copy,
          shortcut: "Ctrl+D",
          action: duplicateSelectedEffect,
        },
        { separator: true },
        {
          label: t("studio.ctx.delete"),
          icon: Trash2,
          shortcut: "Delete",
          danger: true,
          action: deleteSelectedEffect,
        },
      ],
    };
  }

  function openTimelineContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const clickTime = snapTime(timeFromPointer(e.clientX));
    contextMenu = {
      open: true,
      x: e.clientX,
      y: e.clientY,
      items: [
        {
          label: `${t("studio.ctx.addKfHere")} (${clickTime.toFixed(1)}s)`,
          icon: Camera,
          disabled: !status.connected || !status.open,
          action: () => {
            playheadTime = clickTime;
            void executeCaptureFromCamera();
          },
        },
        {
          label: `${t("menu.pasteHere")} (${clickTime.toFixed(1)}s)`,
          icon: FolderOpen,
          shortcut: "Ctrl+V",
          disabled: !studioClipboard,
          action: () => {
            playheadTime = clickTime;
            pasteSelected();
          },
        },
        {
          label: `${t("studio.ctx.addEffectHere")} (Smooth Zoom)`,
          icon: Sparkles,
          action: () => {
            addEffectToTimeline("smoothZoom", clickTime, 0);
          },
        },
        {
          label: `${t("studio.ctx.addEffectHere")} (Camera Drift)`,
          icon: Sparkles,
          action: () => {
            addEffectToTimeline("cameraDrift", clickTime, 1);
          },
        },
      ],
    };
  }

  function handleCustomCurveSaved(curve: BezierCurve) {
    if (!curveEditorTarget) return;
    if (curveEditorTarget.type === "keyframe") {
      const kf = activeProject.keyframes.find((k) => k.id === curveEditorTarget?.id);
      if (kf) {
        (kf as any).customCurve = curve;
      }
    } else if (curveEditorTarget.type === "effect") {
      const ef = activeProject.effects.find((e) => e.id === curveEditorTarget?.id);
      if (ef) {
        (ef as any).customCurve = curve;
      }
    }
    void refreshMathPreview(false);
  }

  function openCurveEditorForKeyframe(kf: Keyframe) {
    curveEditorTarget = {
      type: "keyframe",
      id: kf.id,
      curve: (kf as any).customCurve ?? [0.4, 0.0, 0.2, 1.0],
    };
    showCurveEditorModal = true;
  }

  function openCurveEditorForEffect(ef: EffectClip) {
    curveEditorTarget = {
      type: "effect",
      id: ef.id,
      curve: (ef as any).customCurve ?? [0.4, 0.0, 0.2, 1.0],
    };
    showCurveEditorModal = true;
  }

  function onLibraryItemPointerDown(kind: EffectKind, e: PointerEvent) {
    e.stopPropagation();
    document.body.classList.add("timeline-dragging");
    draggingLibraryEffect = { kind, x: e.clientX, y: e.clientY };
  }

  function duplicateSelectedEffect() {
    if (!selectedEffect) return;
    pushUndoSnapshot();
    const nextStart = Math.min(
      Math.max(0, activeProject.duration - selectedEffect.duration),
      selectedEffect.startTime + selectedEffect.duration,
    );
    const copy: EffectClip = {
      ...$state.snapshot(selectedEffect),
      id: `ef-dup-${Date.now()}`,
      startTime: Math.round(nextStart * 10) / 10,
    };
    activeProject = {
      ...activeProject,
      effects: [...activeProject.effects, copy],
    };
    selectedEffectId = copy.id;
    void refreshMathPreview(false);
  }

  function deleteSelectedEffect() {
    if (!selectedEffectId) return;
    pushUndoSnapshot();
    activeProject = {
      ...activeProject,
      effects: activeProject.effects.filter((e) => e.id !== selectedEffectId),
    };
    selectedEffectId = null;
    void refreshMathPreview(false);
  }

  function copySelected() {
    if (selectedKeyframe) {
      studioClipboard = {
        type: "keyframe",
        data: structuredClone($state.snapshot(selectedKeyframe)),
      };
      void run(async () => {}, t("toast.keyframeCopied", { label: selectedKeyframe.label }));
    } else if (selectedEffect) {
      const cat = EFFECT_CATALOG.find((c) => c.kind === selectedEffect.kind);
      const name = cat ? t(cat.labelKey) : selectedEffect.kind;
      studioClipboard = {
        type: "effect",
        data: structuredClone($state.snapshot(selectedEffect)),
      };
      void run(async () => {}, t("toast.effectCopied", { name }));
    }
  }

  function pasteSelected() {
    if (!studioClipboard) return;
    pushUndoSnapshot();

    if (studioClipboard.type === "keyframe") {
      const src = studioClipboard.data;
      let pasteTime = Math.round(playheadTime * 100) / 100;
      if (Math.abs(pasteTime - src.time) < 0.05) {
        pasteTime = Math.min(activeProject.duration, Math.round((src.time + 1.0) * 100) / 100);
      }
      const newId = `kf-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      const nextIdx = activeProject.keyframes.length;
      const copy: Keyframe = {
        ...structuredClone(src),
        id: newId,
        label: pointLabel(nextIdx),
        time: pasteTime,
      };
      activeProject.keyframes = [...activeProject.keyframes, copy].sort((a, b) => a.time - b.time);
      selectedKeyframeId = copy.id;
      selectedEffectId = null;
      playheadTime = copy.time;
      void refreshMathPreview(false);
      void run(async () => {}, t("toast.keyframePasted", { label: copy.label, time: copy.time.toFixed(1) }));
    } else if (studioClipboard.type === "effect") {
      const src = studioClipboard.data;
      let pasteStart = Math.round(playheadTime * 100) / 100;
      if (Math.abs(pasteStart - src.startTime) < 0.05) {
        pasteStart = Math.min(
          Math.max(0, activeProject.duration - src.duration),
          Math.round((src.startTime + 0.5) * 100) / 100
        );
      }
      const newId = `ef-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      const copy: EffectClip = {
        ...structuredClone(src),
        id: newId,
        startTime: pasteStart,
      };
      activeProject = {
        ...activeProject,
        effects: [...activeProject.effects, copy],
      };
      selectedEffectId = copy.id;
      selectedKeyframeId = null;
      void refreshMathPreview(false);
      const cat = EFFECT_CATALOG.find((c) => c.kind === copy.kind);
      const name = cat ? t(cat.labelKey) : copy.kind;
      void run(async () => {}, t("toast.effectPasted", { name, time: copy.startTime.toFixed(1) }));
    }
  }

  function duplicateSelected() {
    if (selectedKeyframeId) {
      duplicateSelectedKeyframe();
    } else if (selectedEffectId) {
      duplicateSelectedEffect();
    }
  }

  // Controles de transporte y navegación temporal
  function jumpToStart() {
    playheadTime = 0;
    void refreshMathPreview(liveScrub);
  }

  function jumpToEnd() {
    playheadTime = activeProject.duration;
    void refreshMathPreview(liveScrub);
  }

  function stepTime(deltaSec: number) {
    playheadTime = Math.max(0, Math.min(activeProject.duration, Math.round((playheadTime + deltaSec) * 100) / 100));
    void refreshMathPreview(liveScrub);
  }

  function jumpToPrevKeyframe() {
    const sorted = [...activeProject.keyframes].sort((a, b) => a.time - b.time);
    const prev = sorted.filter((k) => k.time < playheadTime - 0.05).pop();
    if (prev) {
      playheadTime = prev.time;
      selectedKeyframeId = prev.id;
      selectedEffectId = null;
    } else {
      playheadTime = 0;
    }
    void refreshMathPreview(liveScrub);
  }

  function jumpToNextKeyframe() {
    const sorted = [...activeProject.keyframes].sort((a, b) => a.time - b.time);
    const next = sorted.find((k) => k.time > playheadTime + 0.05);
    if (next) {
      playheadTime = next.time;
      selectedKeyframeId = next.id;
      selectedEffectId = null;
    } else {
      playheadTime = activeProject.duration;
    }
    void refreshMathPreview(liveScrub);
  }

  /** "Nueva secuencia" = proyecto en blanco: sin keyframes ni efectos (ni los de ejemplo), sin historial del anterior. */
  async function createNewProject() {
    // Una reproducción en curso sigue con el proyecto anterior: se detiene antes de reemplazarlo.
    if (playbackStatus.state !== "stopped") {
      try {
        await api.stopStudio();
      } catch {
        /* ya estaba detenida */
      }
    }
    // Las ediciones del proyecto que se abandona quedan en la biblioteca en memoria (si estaba guardado).
    syncActiveProjectToLibrary(activeProject);
    activeProject = createDefaultSequenceProject(t("default.newProjectName"), status.anchor, false);
    selectedKeyframeId = null;
    selectedEffectId = null;
    playheadTime = 0;
    undoStack = [];
    redoStack = [];
    evaluatedFrame = null;
    trajectoryPreview = null;
    pendingDurationConflict = null;
    void refreshMathPreview(false);
  }

  async function saveCurrentSequence() {
    await run(async () => {
      const saved = await api.saveSequence(activeProject);
      activeProject = saved;
      const lib = await api.listSequences();
      sequences = lib.sequences;
    }, t("studio.saved"));
  }

  // Reproducción Vista Previa vs En Vivo (VRChat OSC)
  async function startPlayback(liveOsc: boolean) {
    const isAtEnd = playheadTime >= activeProject.duration - 0.25;
    if (isAtEnd) {
      playheadTime = 0;
      void refreshMathPreview(false);
    }
    const startAt = playheadTime;
    await run(async () => {
      await api.playSequence(activeProject, startAt, playbackStatus.looping, liveOsc);
    });
  }

  // Reproducción unificada (En Vivo si VRChat está abierto, o Vista Previa local)
  async function handleTogglePlayback() {
    if (playbackStatus.state === "playing") {
      await api.studioControl("pause");
      return;
    }
    if (activeProject.keyframes.length === 0) return;
    const liveOsc = Boolean(status.connected && status.open);
    await startPlayback(liveOsc);
  }

  async function toggleRecording() {
    if (!recordingActive) {
      await run(async () => {
        await api.startRecording();
        recordingActive = true;
        recordingElapsed = 0;
        recordingSamples = 0;
      });
    } else {
      await run(async () => {
        const res = await api.stopRecording(posTolM, rotTolDeg, smoothingPasses);
        recordingActive = false;
        pushUndoSnapshot();
        activeProject = {
          ...activeProject,
          duration: res.duration,
          rawRecording: res.rawSamples,
          keyframes: res.keyframes,
        };
        selectedKeyframeId = res.keyframes[0]?.id ?? null;
        playheadTime = 0;
        await refreshMathPreview(false);
      }, t("toast.recordingSaved"));
    }
  }

  async function applyResimplification() {
    if (activeProject.rawRecording.length === 0) return;
    await run(async () => {
      const kfs = await api.resimplifyRecording(
        activeProject.rawRecording,
        posTolM,
        rotTolDeg,
        smoothingPasses,
      );
      pushUndoSnapshot();
      activeProject = {
        ...activeProject,
        keyframes: kfs,
      };
      selectedKeyframeId = kfs[0]?.id ?? null;
      await refreshMathPreview(false);
    }, t("toast.simplified", { count: activeProject.keyframes.length }));
  }

  function bindCurrentAnchorToProject() {
    if (!status.anchor) return;
    pushUndoSnapshot();
    const a = status.anchor;
    const rad = (a.yaw * Math.PI) / 180;
    const s = Math.sin(rad);
    const c = Math.cos(rad);
    const wrap = (deg: number) => {
      const r = ((deg % 360) + 360) % 360;
      return r > 180 ? r - 360 : r;
    };
    const updatedKfs = activeProject.keyframes.map((kf) => {
      const dx = kf.rawPose.pos[0] - a.eye[0];
      const dy = kf.rawPose.pos[1] - a.eye[1];
      const dz = kf.rawPose.pos[2] - a.eye[2];
      return {
        ...kf,
        localPose: {
          pos: [dx * c - dz * s, dy, dx * s + dz * c] as [number, number, number],
          rot: [
            wrap(kf.rawPose.rot[0]),
            wrap(kf.rawPose.rot[1] - a.yaw),
            wrap(kf.rawPose.rot[2]),
          ] as [number, number, number],
        },
      };
    });
    activeProject = {
      ...activeProject,
      anchorSnapshot: a,
      keyframes: updatedKfs,
    };
    void refreshMathPreview(false);
  }

  async function captureTargetFromCamera() {
    const currentBlend = activeProject.target?.blend ?? 1.0;
    await run(async () => {
      const tg = await api.captureTargetPoint(currentBlend);
      pushUndoSnapshot();
      activeProject = {
        ...activeProject,
        target: tg,
      };
      await refreshMathPreview(false);
    }, t("toast.targetCaptured"));
  }

  // Interacción con el timeline (arrastrar cursor, keyframes y clips)
  function timeFromPointer(clientX: number): number {
    if (!timelineTrackEl) return playheadTime;
    const rect = timelineTrackEl.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / Math.max(1, rect.width)));
    return ratio * activeProject.duration;
  }

  function onTimelinePointerMove(e: PointerEvent) {
    if (draggingLibraryEffect) {
      draggingLibraryEffect.x = e.clientX;
      draggingLibraryEffect.y = e.clientY;
      for (let tIdx = 0; tIdx < 3; tIdx++) {
        const laneEl = fxLaneEls[tIdx];
        if (laneEl) {
          const rect = laneEl.getBoundingClientRect();
          if (e.clientY >= rect.top - 12 && e.clientY <= rect.bottom + 12) {
            hoveredFxTrack = tIdx;
            break;
          }
        }
      }
      return;
    }

    const ds = dragState;
    if (!ds) return;
    const rawSec = timeFromPointer(e.clientX);
    if (ds.kind === "playhead") {
      playheadTime = Math.round(rawSec * 100) / 100;
      void refreshMathPreview(liveScrub);
    } else if (ds.kind === "keyframe") {
      const tSec = snapTime(rawSec);
      const idx = activeProject.keyframes.findIndex((k) => k.id === ds.id);
      if (idx >= 0) {
        // Actualizar sin reordenar destructivamente durante el arrastre para evitar bugs de selección
        activeProject.keyframes[idx].time = tSec;
        playheadTime = tSec;
        void refreshMathPreview(liveScrub);
      }
    } else if (ds.kind === "keyframe-stretch") {
      const tSec = snapTime(rawSec);
      const nextIdx = activeProject.keyframes.findIndex((k) => k.id === ds.nextId);
      const currIdx = activeProject.keyframes.findIndex((k) => k.id === ds.id);
      if (nextIdx >= 0 && currIdx >= 0) {
        const minTime = activeProject.keyframes[currIdx].time + 0.1;
        const newTime = Math.min(activeProject.duration, Math.max(minTime, tSec));
        activeProject.keyframes[nextIdx].time = newTime;
        playheadTime = newTime;
        void refreshMathPreview(false);
      }
    } else if (ds.kind === "effect-move") {
      const idx = activeProject.effects.findIndex((ef) => ef.id === ds.id);
      if (idx >= 0) {
        const ef = activeProject.effects[idx];
        const maxStart = Math.max(0, activeProject.duration - ef.duration);
        ef.startTime = Math.min(maxStart, Math.max(0, snapTime(rawSec - ds.offsetSec)));

        // Calcular dinámicamente si el cursor cruzó a otra pista FX verticalmente
        let bestTrack = ef.track;
        let bestDist = Infinity;
        for (let tIdx = 0; tIdx < 3; tIdx++) {
          const laneEl = fxLaneEls[tIdx];
          if (laneEl) {
            const rect = laneEl.getBoundingClientRect();
            const midY = (rect.top + rect.bottom) / 2;
            const dist = Math.abs(e.clientY - midY);
            if (dist < bestDist) {
              bestDist = dist;
              bestTrack = tIdx;
            }
          }
        }
        ef.track = bestTrack;
        void refreshMathPreview(false);
      }
    } else if (ds.kind === "effect-resize") {
      const idx = activeProject.effects.findIndex((ef) => ef.id === ds.id);
      if (idx >= 0) {
        const ef = activeProject.effects[idx];
        const newEnd = Math.min(activeProject.duration, Math.max(ef.startTime + 0.3, snapTime(rawSec)));
        ef.duration = Math.round((newEnd - ef.startTime) * 100) / 100;
        void refreshMathPreview(false);
      }
    }
  }

  function onTimelinePointerUp() {
    if (dragState?.kind === "keyframe" || dragState?.kind === "keyframe-stretch") {
      activeProject.keyframes.sort((a, b) => a.time - b.time);
      void refreshMathPreview(false);
    }
    if (draggingLibraryEffect && hoveredFxTrack !== null && timelineTrackEl) {
      const dropTime = snapTime(timeFromPointer(draggingLibraryEffect.x));
      addEffectToTimeline(draggingLibraryEffect.kind, dropTime, hoveredFxTrack);
    }
    dragState = null;
    draggingLibraryEffect = null;
    hoveredFxTrack = null;
    document.body.classList.remove("timeline-dragging");
  }

  onMount(() => {
    ensureProjectConfigs();
    refreshMathPreview(false);
    api
      .listMotions()
      .then((m) => {
        presetMotions = m;
        if (m.length > 0) selectedPresetMotion = m[0].name;
      })
      .catch(() => {});
    let teleTick = 0;
    let lastState: string = "stopped";
    const poll = setInterval(async () => {
      try {
        const st = await api.studioStatus();
        playbackStatus = st;
        if (st.state === "playing") {
          playheadTime = st.currentTime;
          if (st.frame) evaluatedFrame = st.frame;
        } else if (lastState === "playing" && st.state === "stopped") {
          if (playheadTime >= activeProject.duration - 0.25) {
            playheadTime = 0;
            void refreshMathPreview(false);
          }
        }
        lastState = st.state;
        teleTick = (teleTick + 1) % 3;
        if (teleTick === 0) {
          telemetry = await api.telemetryStatus(activeProject);
        }
        if (recordingActive) {
          const rec = await api.recordingStatus();
          recordingActive = rec.active;
          recordingElapsed = rec.elapsed;
          recordingSamples = rec.sampleCount;
        }
      } catch {
        /* ignorar mientras no haya sesión */
      }
    }, 80);

    const unsubs = [
      shortcutManager.registerAction("playPause", () => {
        void handleTogglePlayback();
      }),
      shortcutManager.registerAction("shuttleStop", () => {
        void api.stopStudio();
      }),
      shortcutManager.registerAction("copySelected", () => {
        copySelected();
      }),
      shortcutManager.registerAction("pasteSelected", () => {
        pasteSelected();
      }),
      shortcutManager.registerAction("stepPrev", () => {
        playheadTime = Math.max(0, Math.round((playheadTime - 0.1) * 100) / 100);
        void refreshMathPreview(liveScrub);
      }),
      shortcutManager.registerAction("stepNext", () => {
        playheadTime = Math.min(activeProject.duration, Math.round((playheadTime + 0.1) * 100) / 100);
        void refreshMathPreview(liveScrub);
      }),
      shortcutManager.registerAction("jumpPrev1s", () => {
        playheadTime = Math.max(0, Math.round((playheadTime - 1.0) * 100) / 100);
        void refreshMathPreview(liveScrub);
      }),
      shortcutManager.registerAction("jumpNext1s", () => {
        playheadTime = Math.min(activeProject.duration, Math.round((playheadTime + 1.0) * 100) / 100);
        void refreshMathPreview(liveScrub);
      }),
      shortcutManager.registerAction("prevKeyframe", () => {
        const sorted = [...activeProject.keyframes].sort((a, b) => a.time - b.time);
        const prev = sorted.filter((k) => k.time < playheadTime - 0.05).pop();
        if (prev) {
          playheadTime = prev.time;
          selectedKeyframeId = prev.id;
          void refreshMathPreview(liveScrub);
        }
      }),
      shortcutManager.registerAction("nextKeyframe", () => {
        const sorted = [...activeProject.keyframes].sort((a, b) => a.time - b.time);
        const next = sorted.find((k) => k.time > playheadTime + 0.05);
        if (next) {
          playheadTime = next.time;
          selectedKeyframeId = next.id;
          void refreshMathPreview(liveScrub);
        }
      }),
      shortcutManager.registerAction("homeStart", () => {
        playheadTime = 0;
        void refreshMathPreview(liveScrub);
      }),
      shortcutManager.registerAction("endFinish", () => {
        playheadTime = activeProject.duration;
        void refreshMathPreview(liveScrub);
      }),
      shortcutManager.registerAction("addKeyframe", () => {
        if (status.connected && status.open) {
          void executeCaptureFromCamera();
        }
      }),
      shortcutManager.registerAction("duplicateSelected", () => {
        if (selectedKeyframeId) duplicateSelectedKeyframe();
        else if (selectedEffectId) duplicateSelectedEffect();
      }),
      shortcutManager.registerAction("deleteSelected", () => {
        if (selectedKeyframeId) deleteSelectedKeyframe();
        else if (selectedEffectId) deleteSelectedEffect();
      }),
      shortcutManager.registerAction("toggleSnap", () => {
        snapEnabled = !snapEnabled;
      }),
      shortcutManager.registerAction("zoomIn", () => {
        timelineZoom = Math.min(8, timelineZoom + 0.5);
      }),
      shortcutManager.registerAction("zoomOut", () => {
        timelineZoom = Math.max(1, timelineZoom - 0.5);
      }),
      shortcutManager.registerAction("openKeybindings", () => {
        showKeybindingsModal = true;
      }),
    ];

    const onKeyDown = (ev: KeyboardEvent) => {
      const target = ev.target as HTMLElement | null;
      const isInput =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);

      if (!isInput && (ev.ctrlKey || ev.metaKey) && !ev.shiftKey && !ev.altKey) {
        const k = ev.key.toLowerCase();
        if (k === "c" && (selectedKeyframeId || selectedEffectId)) {
          ev.preventDefault();
          copySelected();
          return;
        } else if (k === "v" && studioClipboard) {
          ev.preventDefault();
          pasteSelected();
          return;
        } else if (k === "d" && (selectedKeyframeId || selectedEffectId)) {
          ev.preventDefault();
          duplicateSelected();
          return;
        }
      }

      if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "z") {
        ev.preventDefault();
        if (ev.shiftKey) redo();
        else undo();
        return;
      } else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "y") {
        ev.preventDefault();
        redo();
        return;
      }
      shortcutManager.handleKeyEvent(ev);
    };
    window.addEventListener("keydown", onKeyDown);

    return () => {
      clearInterval(poll);
      unsubs.forEach((u) => u());
      window.removeEventListener("keydown", onKeyDown);
    };
  });
</script>

<svelte:window onpointermove={onTimelinePointerMove} onpointerup={onTimelinePointerUp} />

<div class="studio-root">
  <!-- BARRA SUPERIOR DE PROYECTO Y SELECTOR [ CAPTURA | EDITOR ] -->
  <header class="studio-topbar card">
    <div class="project-controls">
      <div class="seq-picker-wrap" title="Cambiar entre secuencias guardadas">
        <IrisSelect
          items={sequenceSelectOptions}
          value={activeProject.id}
          size="sm"
          onchange={(newId) => {
            const found = sequences.find((s) => s.id === newId);
            if (found && found.id !== activeProject.id) openProject(found);
          }}
        />
      </div>

      <input
        class="project-name-input"
        type="text"
        aria-label={t("studio.projectName")}
        bind:value={activeProject.name}
        placeholder={t("studio.projectName")}
      />

      <button type="button" class="ghost sm" onclick={createNewProject} title={t("studio.newProject")}>
        <Plus size={15} /> {t("studio.newProject")}
      </button>

      <button type="button" class="primary sm" onclick={saveCurrentSequence} title={t("studio.save")}>
        <Save size={15} /> {t("studio.save")}
      </button>

      <button
        type="button"
        class="ghost sm"
        class:pressed={showProjectConfig}
        aria-expanded={showProjectConfig}
        onclick={() => (showProjectConfig = !showProjectConfig)}
        title={t("studio.config.subtitle")}
      >
        <Sliders size={15} />
        <span>{t("studio.config.button")}</span>
        <span class="badge-dur">
          {activeProject.durationUnit === "minutes"
            ? `${secondsToUnitValue(activeProject.duration, "minutes")} min`
            : `${secondsToUnitValue(activeProject.duration, "seconds")} s`}
          ({formatSecondsToMmSs(activeProject.duration)})
        </span>
      </button>

      <div class="history-btns">
        <button
          type="button"
          class="ghost icon-only"
          disabled={undoStack.length === 0}
          onclick={undo}
          title="{t('studio.undo')} (Ctrl+Z)"
        >
          <Undo2 size={15} />
        </button>
        <button
          type="button"
          class="ghost icon-only"
          disabled={redoStack.length === 0}
          onclick={redo}
          title="{t('studio.redo')} (Ctrl+Y)"
        >
          <Redo2 size={15} />
        </button>
      </div>
    </div>

    <!-- Selector Principal: [ Captura | Editor ] -->
    <div class="mode-switch" role="tablist" aria-label={t("studio.tabAriaLabel")}>
      <button
        type="button"
        role="tab"
        aria-selected={studioMode === "capture"}
        class="mode-tab"
        class:active={studioMode === "capture"}
        onclick={() => (studioMode = "capture")}
      >
        <Camera size={15} /> {t("studio.modeCapture")}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={studioMode === "editor"}
        class="mode-tab"
        class:active={studioMode === "editor"}
        onclick={() => (studioMode = "editor")}
      >
        <Film size={15} /> {t("studio.modeEditor")}
      </button>
    </div>
  </header>

  <!-- SECCIÓN: CONFIGURACIÓN DEL PROYECTO (Duración total personalizable en s, min o MM:SS) -->
  {#if showProjectConfig || pendingDurationConflict}
    <section class="project-config-panel card" aria-label={t("studio.config.title")}>
      <div class="pc-head">
        <div>
          <h3><Sliders size={16} /> {t("studio.config.title")}</h3>
          <p class="sub">{t("studio.config.subtitle")} · {t("studio.config.techLimit")}</p>
        </div>
        <button
          type="button"
          class="ghost sm"
          onclick={() => {
            showProjectConfig = false;
            pendingDurationConflict = null;
          }}
        >
          {t("studio.close")}
        </button>
      </div>

      <div class="pc-grid">
        <!-- Selector de modalidad de entrada: 1) Valor y unidad  2) Minutos:Segundos (MM:SS) -->
        <div class="pc-field">
          <span class="pc-label">{t("studio.config.inputMode")}</span>
          <div class="mode-switch sm-switch" role="tablist" aria-label={t("studio.config.inputMode")}>
            <button
              type="button"
              role="tab"
              aria-selected={configInputMode === "valueUnit"}
              class="mode-tab"
              class:active={configInputMode === "valueUnit"}
              onclick={() => switchConfigInputMode("valueUnit")}
            >
              {t("studio.config.modeValueUnit")}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={configInputMode === "mmSs"}
              class="mode-tab"
              class:active={configInputMode === "mmSs"}
              onclick={() => switchConfigInputMode("mmSs")}
            >
              {t("studio.config.modeMmSs")}
            </button>
          </div>
        </div>

        {#if configInputMode === "valueUnit"}
          <!-- Modalidad 1: Valor numérico libre (admite decimales) + Selector de unidad (Segundos / Minutos) -->
          <label class="pc-field">
            <span class="pc-label">
              {t("studio.config.durationLabel")} ({configUnit === "minutes" ? "min" : "s"})
            </span>
            <input
              type="number"
              step="any"
              min={configUnit === "minutes" ? secondsToUnitValue(MIN_DURATION_SEC, "minutes") : MIN_DURATION_SEC}
              max={configUnit === "minutes" ? secondsToUnitValue(MAX_DURATION_SEC, "minutes") : MAX_DURATION_SEC}
              value={configValueInput}
              oninput={(e) => {
                configValueInput = e.currentTarget.value;
                pendingDurationConflict = null;
              }}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  handleApplyProjectConfig();
                }
              }}
            />
          </label>

          <label class="pc-field">
            <span class="pc-label">{t("studio.config.unitLabel")}</span>
            <select
              value={configUnit}
              onchange={(e) => switchConfigUnit(e.currentTarget.value as DurationUnit)}
            >
              <option value="seconds">{t("studio.config.unitSeconds")} (s)</option>
              <option value="minutes">{t("studio.config.unitMinutes")} (min)</option>
            </select>
          </label>
        {:else}
          <!-- Modalidad 2: Formato minutos:segundos (MM:SS) -->
          <label class="pc-field">
            <span class="pc-label">{t("studio.config.mmSsLabel")}</span>
            <input
              type="text"
              placeholder={t("studio.config.mmSsPlaceholder")}
              value={configMmSsInput}
              oninput={(e) => {
                configMmSsInput = e.currentTarget.value;
                pendingDurationConflict = null;
              }}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  handleApplyProjectConfig();
                }
              }}
            />
          </label>

          <label class="pc-field">
            <span class="pc-label">{t("studio.config.unitLabel")}</span>
            <select
              value={configUnit}
              onchange={(e) => switchConfigUnit(e.currentTarget.value as DurationUnit)}
            >
              <option value="seconds">{t("studio.config.unitSeconds")} (s)</option>
              <option value="minutes">{t("studio.config.unitMinutes")} (min)</option>
            </select>
          </label>
        {/if}

        <!-- Duración equivalente y total calculado -->
        <div class="pc-field pc-equiv">
          <span class="pc-label">{t("studio.config.equivalent")} / {t("studio.config.calculatedTotal")}</span>
          <div class="equiv-readout">
            <strong>{draftEquivalence.secondsText}</strong>
            <span>·</span>
            <span>{draftEquivalence.minutesText}</span>
            <span>·</span>
            <code>{draftEquivalence.mmSsText}</code>
          </div>
        </div>

        <!-- Botón explícito para Aplicar cambios -->
        <div class="pc-actions">
          <button
            type="button"
            class="primary sm"
            disabled={!parsedConfigDuration.ok}
            onclick={handleApplyProjectConfig}
          >
            {t("studio.config.apply")}
          </button>
        </div>
      </div>

      {#if !parsedConfigDuration.ok}
        <p class="studio-alert error" role="alert">
          {#if parsedConfigDuration.code === "secondsRange"}
            {t("studio.config.errSecondsRange")}
          {:else if parsedConfigDuration.code === "range"}
            {t("studio.config.errRange")}
          {:else}
            {t("studio.config.errInvalidNumber")}
          {/if}
        </p>
      {/if}

      {#if pendingDurationConflict}
        <div class="conflict-box" role="alertdialog" aria-label={t("studio.config.conflictTitle")}>
          <strong>{t("studio.config.conflictTitle")}</strong>
          <p>
            {t("studio.config.conflictDesc", {
              dur: `${pendingDurationConflict.targetSeconds} s (${formatSecondsToMmSs(pendingDurationConflict.targetSeconds)})`,
              kf: pendingDurationConflict.keyframesCount,
              fx: pendingDurationConflict.effectsCount,
            })}
          </p>
          <div class="conflict-actions">
            <button
              type="button"
              class="primary sm"
              onclick={() =>
                pendingDurationConflict &&
                commitDurationChange(pendingDurationConflict.targetSeconds, "clamp")}
            >
              {t("studio.config.resolveClamp")}
            </button>
            <button
              type="button"
              class="ghost sm"
              onclick={() =>
                pendingDurationConflict &&
                commitDurationChange(pendingDurationConflict.targetSeconds, "scale")}
            >
              {t("studio.config.resolveScale")}
            </button>
            <button
              type="button"
              class="ghost sm"
              onclick={() => {
                pendingDurationConflict = null;
                configValueInput = String(secondsToUnitValue(activeProject.duration, configUnit));
                configMmSsInput = formatSecondsToMmSs(activeProject.duration);
              }}
            >
              {t("studio.config.resolveCancel")}
            </button>
          </div>
        </div>
      {/if}
    </section>
  {/if}

  {#if playbackStatus.error}
    <p class="studio-alert error" role="alert">{playbackStatus.error}</p>
  {/if}

  {#if captureArmed}
    <div class="studio-alert armed" role="status">
      <Radio size={16} />
      <span>{t("studio.captureArmedHint")}</span>
      <button type="button" class="ghost sm" onclick={() => (captureArmed = false)}>{t("studio.cancel")}</button>
    </div>
  {/if}

  <!-- =========================================================================
       MODO 1: MODO SIMPLE — CAPTURA (Experiencia predeterminada y accesible)
       ========================================================================= -->
  {#if studioMode === "capture"}
    {#if hasAdvancedFeatures}
      <div class="studio-alert advanced-notice">
        <Sparkles size={16} />
        <span>{t("studio.advancedNotice")}</span>
        <button type="button" class="ghost sm" onclick={() => (studioMode = "editor")}>
          {t("studio.openInEditor")}
        </button>
      </div>
    {/if}

    <!-- Guía paso a paso para crear la primera secuencia -->
    <section class="guide-strip card">
      <strong>{t("studio.guideTitle")}:</strong>
      <span>{t("studio.guideStep1")}</span>
      <span>→</span>
      <span>{t("studio.guideStep2")}</span>
      <span>→</span>
      <span>{t("studio.guideStep3")}</span>
      <span>→</span>
      <span>{t("studio.guideStep4")}</span>
    </section>

    <!-- Barra de acciones esenciales del Modo Captura -->
    <section class="capture-toolbar card">
      <div class="capture-primary-actions">
        <button
          type="button"
          class="primary capture-hero-btn"
          class:armed={captureArmed}
          disabled={!status.connected || !status.open}
          title={!status.connected
            ? t("banner.disconnected")
            : !status.open
              ? t("banner.closed")
              : t("studio.capturePoint")}
          onclick={handleCaptureButton}
        >
          <CircleDot size={18} />
          {captureArmed ? t("studio.captureArmed") : t("studio.capturePoint")}
        </button>

        <button
          type="button"
          class="ghost sm"
          title={t("studio.simPointTooltip")}
          onclick={addLocalTestPoint}
        >
          <Plus size={15} /> {t("studio.addSimPoint")}
        </button>

        <button
          type="button"
          class="ghost sm"
          class:rec-active={recordingActive}
          disabled={!status.connected || !status.open}
          title={t("studio.recordHintTooltip")}
          onclick={toggleRecording}
        >
          <Radio size={15} />
          {recordingActive
            ? t("studio.recordStop", { s: recordingElapsed.toFixed(1), n: recordingSamples })
            : t("studio.recordStart")}
        </button>
      </div>

      <div class="duration-controls">
        <label class="dur-field">
          <span>
            {t("studio.duration")} ({configUnit === "minutes" ? "min" : "s"})
          </span>
          <input
            type="number"
            step="any"
            min={configUnit === "minutes" ? secondsToUnitValue(MIN_DURATION_SEC, "minutes") : MIN_DURATION_SEC}
            max={configUnit === "minutes" ? secondsToUnitValue(MAX_DURATION_SEC, "minutes") : MAX_DURATION_SEC}
            value={secondsToUnitValue(activeProject.duration, configUnit)}
            onchange={(e) => {
              const parsed = parseValueUnitInput(e.currentTarget.value, configUnit);
              if (parsed.ok) {
                configValueInput = e.currentTarget.value;
                configMmSsInput = formatSecondsToMmSs(parsed.seconds);
                requestDurationChange(parsed.seconds);
              } else {
                e.currentTarget.value = String(secondsToUnitValue(activeProject.duration, configUnit));
              }
            }}
          />
        </label>
        <div class="dur-unit-select-wrap">
          <IrisSelect
            items={configUnitOptions}
            value={configUnit}
            size="sm"
            onchange={(val) => switchConfigUnit(val as DurationUnit)}
          />
        </div>
        <div class="dur-preset-strip" title="Duraciones predeterminadas para acceso rápido">
          {#each [5, 10, 15, 30, 60] as presetSec (presetSec)}
            <button
              type="button"
              class="dur-pill"
              class:active={Math.abs(activeProject.duration - presetSec) < 0.05}
              onclick={() => requestDurationChange(presetSec)}
            >
              {presetSec}s
            </button>
          {/each}
        </div>
        {#if keyframesNeedFitting}
          <button
            type="button"
            class="dur-pill fit-warn-btn"
            title={t("studio.fitTooltip", { end: lastKeyframeTime.toFixed(1), total: activeProject.duration.toFixed(1) })}
            onclick={fitKeyframesToDuration}
          >
            <Sparkles size={12} /> {t("studio.fitToDuration")}
          </button>
        {/if}
        <button
          type="button"
          class="ghost sm"
          onclick={() => (showProjectConfig = !showProjectConfig)}
          title={t("studio.config.subtitle")}
        >
          <Sliders size={14} /> {formatSecondsToMmSs(activeProject.duration)}
        </button>
        <div class="dur-select-field" title={t("studio.engineTooltip")}>
          <span class="field-label">{t("studio.profileLabel")}</span>
          <IrisSelect
            items={cinematicProfileOptions}
            value={activeProject.cinematic?.profile ?? "cinematic"}
            size="sm"
            onchange={(val) => applyCinematicProfile(val as CinematicProfile)}
          />
        </div>
        {#if presetMotions.length > 0}
          <div class="dur-select-field preset-motion-field" title={t("studio.presetMotionFieldTooltip")}>
            <IrisSelect
              items={presetMotionOptions}
              bind:value={selectedPresetMotion}
              size="sm"
            />
            <button
              type="button"
              class="ghost sm"
              onclick={() => loadPresetMotionIntoStudio(selectedPresetMotion)}
            >
              <Orbit size={14} /> {t("studio.loadMotion")}
            </button>
          </div>
        {/if}
      </div>

      <div class="playback-controls">
        {#if playbackStatus.state === "playing"}
          <button type="button" class="ghost sm" onclick={() => api.studioControl("pause")}>
            <Pause size={15} /> {t("studio.pause")}
          </button>
          <button type="button" class="ghost sm" onclick={() => api.stopStudio()}>
            <Square size={15} /> {t("studio.stop")}
          </button>
        {:else}
          <button
            type="button"
            class="primary sm"
            disabled={activeProject.keyframes.length === 0}
            title={status.connected && status.open ? t("studio.playTooltipVrc") : t("studio.playTooltipPreview")}
            onclick={handleTogglePlayback}
          >
            <Play size={15} /> {t("studio.play")}
          </button>
        {/if}

        <button
          type="button"
          class="ghost icon-only"
          class:pressed={playbackStatus.looping}
          title={t("studio.loop")}
          onclick={() => {
            playbackStatus.looping = !playbackStatus.looping;
            void api.studioControl({ loop: playbackStatus.looping }).catch(() => {});
          }}
        >
          <Repeat size={15} />
        </button>

        <span class="time-readout">
          {formatTimeReadout(playheadTime, activeProject.duration)} /
          {formatTimeReadout(activeProject.duration, activeProject.duration)}
        </span>
      </div>
    </section>

    {#if activeProject.rawRecording.length > 0}
      <section class="recording-refine-bar card">
        <div>
          <strong>{t("studio.rawRecordingHeading", { samples: activeProject.rawRecording.length })}</strong>
          <p class="sub">{t("studio.pulseSub")}</p>
        </div>
        <div class="refine-controls">
          <label>
            {t("studio.tolerance")} <strong>{posTolM.toFixed(2)}m</strong>
            <input type="range" min="0.01" max="0.2" step="0.01" bind:value={posTolM} />
          </label>
          <label>
            {t("studio.pulseSmoothing", { passes: smoothingPasses })}
            <input type="range" min="0" max="5" step="1" bind:value={smoothingPasses} />
          </label>
          <button type="button" class="ghost sm" onclick={applyResimplification}>
            <Wand2 size={14} /> {t("studio.resimplify")}
          </button>
        </div>
      </section>
    {/if}

    <div class="capture-main-grid">
      <!-- Columna izquierda: Timeline Simplificado (2 pistas) + Controles del punto/efecto seleccionado -->
      <div class="capture-left-col">
        <section class="card simple-timeline-card">
          <div class="tl-header">
            <h3>
              {t("studio.timelineTitle", { dur: activeProject.duration, mmss: formatSecondsToMmSs(activeProject.duration) })}
            </h3>
            <div class="quick-effect-adder">
              <IrisSelect
                items={quickEffectKindOptions}
                bind:value={quickEffectKind}
                size="sm"
              />
              <button type="button" class="ghost sm" onclick={() => addEffectToTimeline(quickEffectKind)}>
                <Plus size={14} /> {t("studio.addEffect")}
              </button>
            </div>
          </div>

          <!-- Pista interactiva con Regla Temporal + Pista de Recorrido + Pista de Efectos -->
          <div class="timeline-lanes">
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="tl-track-body"
              bind:this={timelineTrackEl}
              oncontextmenu={openTimelineContextMenu}
            >
              <!-- Regla de tiempo adaptativa -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="tl-ruler"
                onpointerdown={(e) => {
                  playheadTime = Math.round(timeFromPointer(e.clientX) * 100) / 100;
                  dragState = { kind: "playhead" };
                  void refreshMathPreview(liveScrub);
                }}
              >
                {#each captureTimelineTicks as tick (tick.time)}
                  <span class="tick" style="left: {tick.leftPct}%">
                    {tick.label}
                  </span>
                {/each}
              </div>

            <!-- Pista 1: Recorrido de cámara (Puntos A, B, C...) con segmentos estirables -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="tl-lane path-lane"
              onpointerdown={(e) => {
                if (e.target === e.currentTarget) {
                  playheadTime = Math.round(timeFromPointer(e.clientX) * 100) / 100;
                  dragState = { kind: "playhead" };
                  void refreshMathPreview(liveScrub);
                }
              }}
            >
              <span class="lane-label">{t("studio.trackPath")}</span>
              {#if activeProject.keyframes.length === 0}
                <span class="lane-empty">{t("studio.timelineEmpty")}</span>
              {/if}

              <!-- Barras de segmento entre puntos para alargar/estirar directamente desde el timeline -->
              {#each activeProject.keyframes as kf, idx (kf.id + "-seg-cap")}
                {#if idx < activeProject.keyframes.length - 1}
                  {@const nextKf = activeProject.keyframes[idx + 1]}
                  {@const segLeftPct = (kf.time / Math.max(0.1, activeProject.duration)) * 100}
                  {@const segWidthPct = Math.max(0, ((nextKf.time - kf.time) / Math.max(0.1, activeProject.duration)) * 100)}
                  {@const segDuration = Math.max(0, nextKf.time - kf.time)}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div
                    class="kf-segment-bar"
                    style="left: {segLeftPct}%; width: {segWidthPct}%;"
                    title={t("studio.segmentStretchTooltip", { from: formatKeyframeLabel(kf.label), to: formatKeyframeLabel(nextKf.label), duration: segDuration.toFixed(2) })}
                  >
                    <span class="seg-fill"></span>
                    <span class="seg-badge">{segDuration.toFixed(1)}s</span>
                    <span
                      class="seg-stretch-handle"
                      title={t("studio.stretchHandleTooltip")}
                      onpointerdown={(e) => {
                        e.stopPropagation();
                        pushUndoSnapshot();
                        document.body.classList.add("timeline-dragging");
                        dragState = {
                          kind: "keyframe-stretch",
                          id: kf.id,
                          nextId: nextKf.id,
                          origNextTime: nextKf.time,
                        };
                      }}
                    ></span>
                  </div>
                {/if}
              {/each}

              {#each activeProject.keyframes as kf (kf.id)}
                {@const leftPct = (kf.time / Math.max(0.1, activeProject.duration)) * 100}
                <button
                  type="button"
                  class="kf-pill"
                  class:selected={selectedKeyframeId === kf.id}
                  style="left: {leftPct}%"
                  title={t("studio.keyframeTooltip", { label: formatKeyframeLabel(kf.label), time: kf.time.toFixed(2) })}
                  onpointerdown={(e) => {
                    e.stopPropagation();
                    pushUndoSnapshot();
                    document.body.classList.add("timeline-dragging");
                    selectedKeyframeId = kf.id;
                    selectedEffectId = null;
                    playheadTime = kf.time;
                    dragState = { kind: "keyframe", id: kf.id };
                    void refreshMathPreview(false);
                  }}
                  ondblclick={(e) => {
                    e.stopPropagation();
                    quickEditorState = { open: true, keyframe: kf, x: e.clientX, y: e.clientY };
                  }}
                  oncontextmenu={(e) => openKeyframeContextMenu(kf, e)}
                >
                  <span class="kf-dot"></span>
                  <span>{formatKeyframeLabel(kf.label)}</span>
                  <small>{kf.time.toFixed(1)}s</small>
                </button>
              {/each}
            </div>

            <!-- Pista 2: Movimientos y Efectos -->
            <div class="tl-lane fx-lane">
              <span class="lane-label">{t("studio.trackEffects")}</span>
              {#each activeProject.effects as ef (ef.id)}
                {@const startPct = (ef.startTime / Math.max(0.1, activeProject.duration)) * 100}
                {@const widthPct = Math.max(
                  4,
                  (ef.duration / Math.max(0.1, activeProject.duration)) * 100,
                )}
                {@const catEntry = EFFECT_CATALOG.find((c) => c.kind === ef.kind)}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="fx-block"
                  class:selected={selectedEffectId === ef.id}
                  style="left: {startPct}%; width: {widthPct}%"
                  title="{catEntry ? t(catEntry.labelKey) : ef.kind} ({ef.startTime.toFixed(1)}–{(ef.startTime + ef.duration).toFixed(1)}s)"
                  onpointerdown={(e) => {
                    e.stopPropagation();
                    pushUndoSnapshot();
                    document.body.classList.add("timeline-dragging");
                    selectedEffectId = ef.id;
                    selectedKeyframeId = null;
                    const clickSec = timeFromPointer(e.clientX);
                    dragState = {
                      kind: "effect-move",
                      id: ef.id,
                      offsetSec: clickSec - ef.startTime,
                    };
                  }}
                  oncontextmenu={(e) => openEffectContextMenu(ef, e)}
                >
                  <span class="fx-title">{catEntry ? t(catEntry.labelKey) : ef.kind}</span>
                  <span
                    class="fx-resize-handle"
                    title={t("studio.dragDurationTooltip")}
                    onpointerdown={(e) => {
                      e.stopPropagation();
                      pushUndoSnapshot();
                      document.body.classList.add("timeline-dragging");
                      selectedEffectId = ef.id;
                      dragState = { kind: "effect-resize", id: ef.id };
                    }}
                  ></span>
                </div>
              {/each}
            </div>

            <!-- Cursor de reproducción (Playhead) con badge de tiempo -->
            <div
              class="playhead-line"
              style="left: {(playheadTime / Math.max(0.1, activeProject.duration)) * 100}%"
            >
              <span class="playhead-badge">
                {formatTimeReadout(playheadTime, activeProject.duration)}
              </span>
            </div>
          </div>
        </div>
      </section>

        <!-- Tarjeta de edición sencilla para el Punto o Efecto seleccionado -->
        {#if selectedKeyframe}
          <section class="card quick-inspector">
            <div class="qi-head">
              <input
                class="label-edit"
                type="text"
                bind:value={selectedKeyframe.label}
                aria-label={t("studio.pointNameAria")}
              />
              <span class="badge">
                {selectedKeyframe.capturedAtMs > 0 ? t("studio.readingVrc") : t("studio.readingLocal")} ·
                {selectedKeyframe.time.toFixed(2)}s
              </span>
            </div>

            <div class="qi-row">
              <label>
                <span>{t("studio.transition")}</span>
                <IrisSelect
                  items={transitionOptions}
                  bind:value={selectedKeyframe.transition}
                  size="sm"
                  onchange={() => refreshMathPreview(false)}
                />
              </label>

              <label title={t("studio.knotModeTooltip")}>
                <span>{t("studio.knotModeTitle")}</span>
                <IrisSelect
                  items={knotModeOptions}
                  value={selectedKeyframe.knotMode ?? "passThrough"}
                  size="sm"
                  onchange={(val) => {
                    selectedKeyframe.knotMode = val as "passThrough" | "stop";
                    refreshMathPreview(false);
                  }}
                />
              </label>

              <div class="qi-actions">
                <button
                  type="button"
                  class="ghost sm"
                  disabled={!status.connected || !status.open}
                  onclick={updateSelectedKeyframeFromCamera}
                >
                  <Camera size={14} /> {t("studio.updatePoint")}
                </button>
                <button type="button" class="ghost sm" onclick={duplicateSelectedKeyframe}>
                  <Copy size={14} /> {t("studio.duplicatePoint")}
                </button>
                <button type="button" class="ghost sm danger" onclick={deleteSelectedKeyframe}>
                  <Trash2 size={14} /> {t("studio.deletePoint")}
                </button>
              </div>
            </div>
          </section>
        {:else if selectedEffect}
          {@const catInfo = EFFECT_CATALOG.find((c) => c.kind === selectedEffect.kind)}
          <section class="card quick-inspector">
            <div class="qi-head">
              <strong>{catInfo ? t(catInfo.labelKey) : selectedEffect.kind}</strong>
              <span class="badge">
                {selectedEffect.startTime.toFixed(1)}s – {(selectedEffect.startTime + selectedEffect.duration).toFixed(1)}s
              </span>
            </div>
            <div class="qi-sliders">
              <label>
                <span>{t("studio.intensity", { val: selectedEffect.intensity.toFixed(1) })}</span>
                <input
                  type="range"
                  min="0.1"
                  max="2.5"
                  step="0.1"
                  bind:value={selectedEffect.intensity}
                  oninput={() => refreshMathPreview(false)}
                />
              </label>
              <label class="dur-field">
                <span>{t("studio.clipDuration")}</span>
                <input
                  type="number"
                  min="0.2"
                  max={Math.max(0.2, activeProject.duration - selectedEffect.startTime)}
                  step="0.1"
                  bind:value={selectedEffect.duration}
                  oninput={() => refreshMathPreview(false)}
                />
              </label>
              <label>
                <span>{t("studio.clipSpeed", { speed: selectedEffect.speed.toFixed(1) })}</span>
                <input
                  type="range"
                  min="0.2"
                  max="3.0"
                  step="0.1"
                  bind:value={selectedEffect.speed}
                  oninput={() => refreshMathPreview(false)}
                />
              </label>
              <button type="button" class="ghost sm danger" onclick={deleteSelectedEffect}>
                <Trash2 size={14} /> {t("studio.deleteEffect")}
              </button>
            </div>
          </section>
        {/if}
      </div>

      <!-- Columna derecha: Visor Espacial 3D compacto -->
      <div class="capture-right-col">
        <SpatialViewport
          preview={trajectoryPreview}
          currentFrame={evaluatedFrame}
          liveCameraPose={status.camera?.poseFresh ? status.camera.pose : null}
          {selectedKeyframeId}
          compact={true}
          onSelectKeyframe={(id) => {
            selectedKeyframeId = id;
            selectedEffectId = null;
          }}
          onUpdateKeyframePos={updateKeyframePos}
        />
      </div>
    </div>
  {:else}
    <!-- =========================================================================
         MODO 2: MODO AVANZADO — EDITOR (4 Regiones Profesionales sin perder nada)
         ========================================================================= -->
    <div class="editor-grid">
      <!-- REGIÓN A: BIBLIOTECA IZQUIERDA (Movimientos, Microefectos, Óptica, Presets) -->
      <aside class="editor-library card">
        <div class="lib-search">
          <Search size={14} />
          <input
            type="search"
            placeholder={t("studio.searchPlaceholder")}
            bind:value={librarySearch}
          />
        </div>

        <div class="lib-tabs" role="tablist">
          <button
            type="button"
            class="lib-tab"
            class:active={libraryCategory === "movement"}
            onclick={() => (libraryCategory = "movement")}
          >
            {t("studio.tabMovements")}
          </button>
          <button
            type="button"
            class="lib-tab"
            class:active={libraryCategory === "micro"}
            onclick={() => (libraryCategory = "micro")}
          >
            {t("studio.secMicro")}
          </button>
          <button
            type="button"
            class="lib-tab"
            class:active={libraryCategory === "optics"}
            onclick={() => (libraryCategory = "optics")}
          >
            {t("studio.secOptics")}
          </button>
          <button
            type="button"
            class="lib-tab"
            class:active={libraryCategory === "presets"}
            onclick={() => (libraryCategory = "presets")}
          >
            {t("studio.secSequences")}
          </button>
        </div>

        <div class="lib-items">
          {#if libraryCategory !== "presets"}
            {#each filteredCatalog as item (item.kind)}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="lib-item draggable-item"
                title={t("studio.effectItemTooltip")}
                onpointerdown={(e) => onLibraryItemPointerDown(item.kind, e)}
              >
                <div>
                  <strong>{t(item.labelKey)}</strong>
                  <p>{getEffectDesc(item)}</p>
                </div>
                <button
                  type="button"
                  class="ghost sm"
                  title={t("studio.addAtCursorTooltip", { time: playheadTime.toFixed(1) })}
                  onclick={(e) => {
                    e.stopPropagation();
                    addEffectToTimeline(item.kind);
                  }}
                >
                  <Plus size={14} />
                </button>
              </div>
            {/each}
          {:else}
            {#each sequences as seq (seq.id)}
              <div class="lib-item">
                <div>
                  <strong>{seq.name}</strong>
                  <p>{seq.duration}s · {t("library.pointsCount", { count: seq.keyframes.length })} · {t("library.effectsCount", { count: seq.effects.length })}</p>
                </div>
                <button
                  type="button"
                  class="ghost sm"
                  onclick={() => {
                    openProject(seq);
                    selectedKeyframeId = activeProject.keyframes[0]?.id ?? null;
                    playheadTime = 0;
                  }}
                >
                  {t("studio.load")}
                </button>
              </div>
            {/each}
          {/if}
        </div>
      </aside>

      <!-- REGIÓN B: EDITOR ESPACIAL CENTRAL + TELEMETRÍA COMPARATIVA -->
      <section class="editor-center">
        <SpatialViewport
          preview={trajectoryPreview}
          currentFrame={evaluatedFrame}
          liveCameraPose={status.camera?.poseFresh ? status.camera.pose : null}
          {selectedKeyframeId}
          compact={false}
          onSelectKeyframe={(id) => {
            selectedKeyframeId = id;
            selectedEffectId = null;
            inspectorTab = "selection";
          }}
          onUpdateKeyframePos={updateKeyframePos}
        />

        <!-- Telemetría que diferencia Pose Recibida (VRChat), Pose Relativa (Ancla) y Pose Evaluada/Enviada -->
        <div class="telemetry-bar card">
          <div class="tele-col">
            <span class="tele-label vrc">{t("studio.telemetry1")}</span>
            {#if status.camera?.poseFresh && status.camera.pose}
              <code>
                XYZ: {status.camera.pose.pos.map((v) => v.toFixed(2)).join(", ")} m | PYR:
                {status.camera.pose.rot.map((v) => v.toFixed(1)).join("°, ")}°
              </code>
            {:else}
              <code class="muted">{t("studio.noRecentTelemetry")}</code>
            {/if}
          </div>
          <div class="tele-col">
            <span class="tele-label base">{t("studio.telemetry2", { time: playheadTime.toFixed(2) })}</span>
            {#if evaluatedFrame}
              <code>
                XYZ: {evaluatedFrame.basePose.pos.map((v) => v.toFixed(2)).join(", ")} m | PYR:
                {evaluatedFrame.basePose.rot.map((v) => v.toFixed(1)).join("°, ")}°
              </code>
            {/if}
          </div>
          <div class="tele-col">
            <span class="tele-label final">{t("studio.telemetry3")}</span>
            {#if evaluatedFrame}
              <code>
                XYZ: {evaluatedFrame.finalPose.pos.map((v) => v.toFixed(2)).join(", ")} m | Zoom:
                {(evaluatedFrame.zoom ?? 45).toFixed(0)} mm | Vel:
                {(evaluatedFrame.linearSpeedMps ?? 0).toFixed(2)} m/s
              </code>
            {/if}
          </div>
          <div class="tele-col">
            <span class="tele-label vrc">{t("studio.feedbackTelemetry")}</span>
            {#if telemetry}
              <code>
                {#if telemetry.certainty.kind === "measured"}
                  {t("studio.telemetry.measured", { seq: telemetry.certainty.seq, age: telemetry.certainty.ageMs })}
                {:else if telemetry.certainty.kind === "estimated"}
                  {t("studio.telemetry.estimated", { pct: Math.round(telemetry.certainty.confidence * 100), reason: telemetry.certainty.reason })}
                {:else}
                  {t("studio.telemetry.openLoop", { reason: telemetry.certainty.reason })}
                {/if}
                {#if telemetry.posErrorM != null}
                  | Err: {(telemetry.posErrorM * 1000).toFixed(1)} mm / {(telemetry.rotErrorDeg ?? 0).toFixed(2)}°
                {/if}
              </code>
            {:else}
              <code class="muted">{t("studio.initDiag")}</code>
            {/if}
          </div>
        </div>
      </section>

      <!-- REGIÓN C: INSPECTOR CONTEXTUAL DERECHO -->
      <aside class="editor-inspector card">
        <div class="insp-tabs">
          <button
            type="button"
            class="lib-tab"
            class:active={inspectorTab === "selection"}
            onclick={() => (inspectorTab = "selection")}
          >
            <Sliders size={13} /> {t("studio.inspectorTitle")}
          </button>
          <button
            type="button"
            class="lib-tab"
            class:active={inspectorTab === "calibration"}
            onclick={() => (inspectorTab = "calibration")}
          >
            <Crosshair size={13} /> {t("studio.tabSpaceTarget")}
          </button>
        </div>

        {#if inspectorTab === "selection"}
          {#if selectedKeyframe}
            <div class="insp-body">
              <div class="insp-row-two">
                <label>
                  <span>{t("studio.labelField")}</span>
                  <input type="text" bind:value={selectedKeyframe.label} />
                </label>
                <label>
                  <span>{t("studio.timeSecField")}</span>
                  <input
                    type="number"
                    min="0"
                    max={activeProject.duration}
                    step="0.1"
                    bind:value={selectedKeyframe.time}
                    onchange={() => {
                      activeProject.keyframes.sort((a, b) => a.time - b.time);
                      refreshMathPreview(false);
                    }}
                  />
                </label>
              </div>

              <div class="insp-row-two">
                <div class="curve-field-col">
                  <label>
                    <span>{t("studio.easingCurve")}</span>
                    <IrisSelect
                      items={transitionOptions}
                      bind:value={selectedKeyframe.transition}
                      size="sm"
                      onchange={() => refreshMathPreview(false)}
                    />
                  </label>
                  <button
                    type="button"
                    class="ghost sm custom-curve-btn"
                    class:has-curve={Boolean(selectedKeyframe.customCurve)}
                    onclick={() => openCurveEditorForKeyframe(selectedKeyframe)}
                    title={t("studio.customCurveBtnTooltip")}
                  >
                    <Sparkles size={13} />
                    <span>{selectedKeyframe.customCurve ? t("studio.curveActive") : t("studio.customCurve")}</span>
                  </button>
                </div>

                <label title={t("studio.knotModeTooltip")}>
                  <span>{t("studio.knotModeTitle")}</span>
                  <IrisSelect
                    items={knotModeOptions}
                    value={selectedKeyframe.knotMode ?? "passThrough"}
                    size="sm"
                    onchange={(val) => {
                      selectedKeyframe.knotMode = val as "passThrough" | "stop";
                      refreshMathPreview(false);
                    }}
                  />
                </label>
              </div>

              <!-- Posición 3D en el Mundo con Acordeón Moderno -->
              <button
                type="button"
                class="insp-accordion-btn"
                onclick={() => (inspectorAccordion.pos = !inspectorAccordion.pos)}
              >
                <span>{t("studio.posWorld")}</span>
                <ChevronDown size={14} class="insp-chevron {!inspectorAccordion.pos ? 'rotated' : ''}" />
              </button>
              {#if inspectorAccordion.pos}
                <div class="xyz-grid">
                  {#each ["X", "Y", "Z"] as axis, i (axis)}
                    <label>
                      <span>{axis}</span>
                      <input
                        type="number"
                        step="0.05"
                        bind:value={selectedKeyframe.rawPose.pos[i]}
                        oninput={() => refreshMathPreview(liveScrub)}
                      />
                    </label>
                  {/each}
                </div>
              {/if}

              <!-- Rotación Euler con Acordeón Moderno -->
              <button
                type="button"
                class="insp-accordion-btn"
                onclick={() => (inspectorAccordion.rot = !inspectorAccordion.rot)}
              >
                <span>{t("studio.rotEuler")}</span>
                <ChevronDown size={14} class="insp-chevron {!inspectorAccordion.rot ? 'rotated' : ''}" />
              </button>
              {#if inspectorAccordion.rot}
                <div class="xyz-grid">
                  {#each ["Pitch", "Yaw", "Roll"] as axis, i (axis)}
                    <label>
                      <span>{axis}</span>
                      <input
                        type="number"
                        step="1"
                        bind:value={selectedKeyframe.rawPose.rot[i]}
                        oninput={() => refreshMathPreview(liveScrub)}
                      />
                    </label>
                  {/each}
                </div>
              {/if}

              {#if selectedKeyframe.localPose}
                <p class="local-readout">
                  {t("studio.anchorOffsetReadout", { pos: selectedKeyframe.localPose.pos.map((v) => v.toFixed(2)).join(", ") })}
                </p>
              {/if}

              <!-- Canales Ópticos con Acordeón Moderno -->
              <button
                type="button"
                class="insp-accordion-btn"
                onclick={() => (inspectorAccordion.optics = !inspectorAccordion.optics)}
              >
                <span>{t("studio.opticalChannels")}</span>
                <ChevronDown size={14} class="insp-chevron {!inspectorAccordion.optics ? 'rotated' : ''}" />
              </button>
              {#if inspectorAccordion.optics}
                <div class="insp-row-two">
                  <label>
                    <span>Zoom (20–300 mm)</span>
                    <input
                      type="number"
                      min="20"
                      max="300"
                      step="1"
                      placeholder={t("studio.noChangePlaceholder")}
                      value={selectedKeyframe.look?.zoom ?? ""}
                      oninput={(e) => {
                        const val = e.currentTarget.value.trim();
                        if (!selectedKeyframe.look) {
                          selectedKeyframe.look = { zoom: null, exposure: null, focalDistance: null, aperture: null };
                        }
                        selectedKeyframe.look.zoom = val === "" ? null : Number(val);
                        refreshMathPreview(liveScrub);
                      }}
                    />
                  </label>
                  <label>
                    <span>{t("studio.exposureRange")}</span>
                    <input
                      type="number"
                      min="-10"
                      max="4"
                      step="0.2"
                      placeholder="Sin cambio"
                      value={selectedKeyframe.look?.exposure ?? ""}
                      oninput={(e) => {
                        const val = e.currentTarget.value.trim();
                        if (!selectedKeyframe.look) {
                          selectedKeyframe.look = { zoom: null, exposure: null, focalDistance: null, aperture: null };
                        }
                        selectedKeyframe.look.exposure = val === "" ? null : Number(val);
                        refreshMathPreview(liveScrub);
                      }}
                    />
                  </label>
                  <label>
                    <span>{t("studio.focusManualRange")}</span>
                    <input
                      type="number"
                      min="0"
                      max="10"
                      step="0.1"
                      placeholder="Sin cambio"
                      value={selectedKeyframe.look?.focalDistance ?? ""}
                      oninput={(e) => {
                        const val = e.currentTarget.value.trim();
                        if (!selectedKeyframe.look) {
                          selectedKeyframe.look = { zoom: null, exposure: null, focalDistance: null, aperture: null };
                        }
                        selectedKeyframe.look.focalDistance = val === "" ? null : Number(val);
                        refreshMathPreview(liveScrub);
                      }}
                    />
                  </label>
                  <label>
                    <span>{t("studio.apertureRange")}</span>
                    <input
                      type="number"
                      min="1.4"
                      max="32"
                      step="0.2"
                      placeholder="Sin cambio"
                      value={selectedKeyframe.look?.aperture ?? ""}
                      oninput={(e) => {
                        const val = e.currentTarget.value.trim();
                        if (!selectedKeyframe.look) {
                          selectedKeyframe.look = { zoom: null, exposure: null, focalDistance: null, aperture: null };
                        }
                        selectedKeyframe.look.aperture = val === "" ? null : Number(val);
                        refreshMathPreview(liveScrub);
                      }}
                    />
                  </label>
                </div>
              {/if}

              <div class="insp-actions">
                <button
                  type="button"
                  class="ghost sm"
                  disabled={!status.connected || !status.open}
                  onclick={updateSelectedKeyframeFromCamera}
                >
                  <Camera size={14} /> {t("studio.updatePoint")}
                </button>
                <button type="button" class="ghost sm" onclick={duplicateSelectedKeyframe}>
                  <Copy size={14} /> {t("studio.duplicatePoint")}
                </button>
                <button type="button" class="ghost sm danger" onclick={deleteSelectedKeyframe}>
                  <Trash2 size={14} /> {t("studio.deletePoint")}
                </button>
              </div>
            </div>
          {:else if selectedEffect}
            {@const catEntry = EFFECT_CATALOG.find((c) => c.kind === selectedEffect.kind)}
            <div class="insp-body">
              <div class="insp-head-inline">
                <strong>{catEntry ? t(catEntry.labelKey) : selectedEffect.kind}</strong>
                <IrisCheckbox
                  variant="switch"
                  bind:checked={selectedEffect.enabled}
                  onchange={() => refreshMathPreview(false)}
                  label={t("studio.active")}
                />
              </div>

              <div class="insp-row-two">
                <label>
                  <span>{t("studio.startSec")}</span>
                  <input
                    type="number"
                    min="0"
                    max={activeProject.duration}
                    step="0.1"
                    bind:value={selectedEffect.startTime}
                    oninput={() => refreshMathPreview(false)}
                  />
                </label>
                <label>
                  <span>{t("studio.durationSec")}</span>
                  <input
                    type="number"
                    min="0.2"
                    max={activeProject.duration}
                    step="0.1"
                    bind:value={selectedEffect.duration}
                    oninput={() => refreshMathPreview(false)}
                  />
                </label>
              </div>

              <div class="insp-row-two">
                <label>
                  <span>{t("studio.multitrack")}</span>
                  <IrisSelect
                    items={fxTrackOptions}
                    value={String(selectedEffect.track)}
                    size="sm"
                    onchange={(val) => {
                      selectedEffect.track = Number(val);
                      refreshMathPreview(false);
                    }}
                  />
                </label>
                <div class="curve-field-col">
                  <label>
                    <span>{t("studio.easingCurve")}</span>
                    <IrisSelect
                      items={transitionOptions}
                      bind:value={selectedEffect.easing}
                      size="sm"
                      onchange={() => refreshMathPreview(false)}
                    />
                  </label>
                  <button
                    type="button"
                    class="ghost sm custom-curve-btn"
                    class:has-curve={Boolean(selectedEffect.customCurve)}
                    onclick={() => openCurveEditorForEffect(selectedEffect)}
                    title={t("studio.customCurveBtnTooltip")}
                  >
                    <Sparkles size={13} />
                    <span>{selectedEffect.customCurve ? t("studio.curveActive") : t("studio.customCurve")}</span>
                  </button>
                </div>
              </div>

              <label>
                <span>{t("studio.intensity", { val: selectedEffect.intensity.toFixed(2) })}</span>
                <input
                  type="range"
                  min="0.1"
                  max="3.0"
                  step="0.05"
                  bind:value={selectedEffect.intensity}
                  oninput={() => refreshMathPreview(false)}
                />
              </label>

              <label>
                <span>{t("studio.clipSpeedFreq", { speed: selectedEffect.speed.toFixed(2) })}</span>
                <input
                  type="range"
                  min="0.2"
                  max="4.0"
                  step="0.1"
                  bind:value={selectedEffect.speed}
                  oninput={() => refreshMathPreview(false)}
                />
              </label>

              {#if catEntry && getEffectParamALabel(catEntry)}
                <div class="insp-row-two">
                  <label>
                    <span>{getEffectParamALabel(catEntry)}</span>
                    <input
                      type="number"
                      step="0.2"
                      value={selectedEffect.paramA ?? catEntry.defaultParamA ?? 0}
                      oninput={(e) => {
                        selectedEffect.paramA = Number(e.currentTarget.value);
                        refreshMathPreview(false);
                      }}
                    />
                  </label>
                  {#if getEffectParamBLabel(catEntry)}
                    <label>
                      <span>{getEffectParamBLabel(catEntry)}</span>
                      <input
                        type="number"
                        step="0.2"
                        value={selectedEffect.paramB ?? catEntry.defaultParamB ?? 0}
                        oninput={(e) => {
                          selectedEffect.paramB = Number(e.currentTarget.value);
                          refreshMathPreview(false);
                        }}
                      />
                    </label>
                  {/if}
                </div>
              {/if}

              <div class="insp-actions">
                <button type="button" class="ghost sm" onclick={duplicateSelectedEffect}>
                  <Copy size={14} /> {t("studio.duplicateClip")}
                </button>
                <button type="button" class="ghost sm danger" onclick={deleteSelectedEffect}>
                  <Trash2 size={14} /> {t("studio.deleteClip")}
                </button>
              </div>
            </div>
          {:else}
            <p class="insp-empty">{t("studio.noSelection")}</p>
          {/if}
        {:else}
          <!-- Pestaña de Motor Cinematográfico, Calibración Técnica/Visual, Espacio y TargetProvider -->
          <div class="insp-body">
            <h4>{t("studio.engineTitle")}</h4>
            <label>
              <span>{t("studio.profileLabel")}</span>
              <IrisSelect
                items={cinematicProfileOptions}
                value={activeProject.cinematic?.profile ?? "cinematic"}
                size="sm"
                onchange={(val) => applyCinematicProfile(val as CinematicProfile)}
              />
            </label>

            {#if activeProject.cinematic}
              <div class="insp-row-two">
                <label>
                  <span>{t("studio.inertiaTau", { tau: activeProject.cinematic.inertiaTau.toFixed(2) })}</span>
                  <input
                    type="range"
                    min="0"
                    max="0.6"
                    step="0.02"
                    bind:value={activeProject.cinematic.inertiaTau}
                  />
                </label>
                <label>
                  <span>{t("studio.maxSpeed", { val: activeProject.cinematic.maxLinearSpeed.toFixed(1) })}</span>
                  <input
                    type="range"
                    min="0.5"
                    max="15"
                    step="0.5"
                    bind:value={activeProject.cinematic.maxLinearSpeed}
                  />
                </label>
              </div>

              <label>
                <span>{t("studio.organicVar", { pct: Math.round((activeProject.cinematic.organicVariation ?? 0) * 100) })}</span>
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  bind:value={activeProject.cinematic.organicVariation}
                  oninput={() => refreshMathPreview(false)}
                />
              </label>
            {/if}

            <hr class="sep" />

            <h4>{t("studio.calibTitle")}</h4>
            {#if activeProject.calibration}
              <div class="insp-row-two">
                <IrisCheckbox
                  variant="switch"
                  bind:checked={activeProject.calibration.enabled}
                  onchange={() => refreshMathPreview(liveScrub)}
                  label={t("studio.calibActive")}
                />
                <IrisCheckbox
                  variant="switch"
                  bind:checked={activeProject.calibration.autoCorrect}
                  disabled={!activeProject.calibration.enabled}
                  label={t("studio.autoCorrect")}
                />
              </div>

              {#if activeProject.calibration.enabled}
                <div class="xyz-grid">
                  {#each ["Off X", "Off Y", "Off Z"] as labelAxis, i (labelAxis)}
                    <label>
                      <span>{labelAxis} (m)</span>
                      <input
                        type="number"
                        step="0.02"
                        min="-2"
                        max="2"
                        bind:value={activeProject.calibration.posOffset[i]}
                        oninput={() => refreshMathPreview(liveScrub)}
                      />
                    </label>
                  {/each}
                </div>

                <div class="insp-row-two">
                  <label>
                    <span>{t("studio.offsetYaw")}</span>
                    <input
                      type="number"
                      step="0.5"
                      bind:value={activeProject.calibration.yawOffsetDeg}
                      oninput={() => refreshMathPreview(liveScrub)}
                    />
                  </label>
                  <label>
                    <span>{t("studio.offsetPitch")}</span>
                    <input
                      type="number"
                      step="0.5"
                      bind:value={activeProject.calibration.pitchOffsetDeg}
                      oninput={() => refreshMathPreview(liveScrub)}
                    />
                  </label>
                </div>

                <label title={t("studio.framingOffset")}>
                  <span>{t("studio.framingOffsetRule", { val: activeProject.calibration.compositionX.toFixed(2) })}</span>
                  <input
                    type="range"
                    min="-1"
                    max="1"
                    step="0.05"
                    bind:value={activeProject.calibration.compositionX}
                    oninput={() => refreshMathPreview(liveScrub)}
                  />
                </label>

                <div class="insp-row-two">
                  <label title={t("studio.headroomTooltip")}>
                    <span>{t("studio.headroomY", { val: activeProject.calibration.compositionY.toFixed(2) })}</span>
                    <input
                      type="range"
                      min="-1"
                      max="1"
                      step="0.05"
                      bind:value={activeProject.calibration.compositionY}
                      oninput={() => refreshMathPreview(liveScrub)}
                    />
                  </label>
                  <label title={t("studio.leadRoom")}>
                    <span>{t("studio.leadRoomVal", { val: activeProject.calibration.leadRoom.toFixed(2) })}</span>
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.05"
                      bind:value={activeProject.calibration.leadRoom}
                      oninput={() => refreshMathPreview(liveScrub)}
                    />
                  </label>
                </div>
              {/if}

              <button type="button" class="ghost sm" onclick={restoreCalibrationDefaults}>
                <RotateCcw size={14} /> {t("studio.resetCalib")}
              </button>

              {#if telemetry && telemetry.log.length > 0}
                <p class="local-readout">
                  {t("studio.lastAdjustment", { reason: telemetry.log[telemetry.log.length - 1].reason })}
                </p>
              {/if}
            {/if}

            <hr class="sep" />

            <h4>{t("studio.spacePortability")}</h4>
            <label>
              <span>{t("studio.spatialMode")}</span>
              <IrisSelect
                items={referenceSpaceOptions}
                bind:value={activeProject.space}
                size="sm"
                onchange={() => refreshMathPreview(false)}
              />
            </label>

            <label>
              <span>{t("studio.trajectoryScale", { scale: activeProject.trajectoryScale.toFixed(2) })}</span>
              <input
                type="range"
                min="0.25"
                max="3.0"
                step="0.05"
                bind:value={activeProject.trajectoryScale}
                oninput={() => refreshMathPreview(false)}
              />
            </label>

            <button
              type="button"
              class="ghost sm"
              disabled={!status.anchored}
              title={t("studio.rebindTooltip")}
              onclick={bindCurrentAnchorToProject}
            >
              <Crosshair size={14} /> {t("studio.rebindAnchor")}
            </button>

            <hr class="sep" />

            <h4>{t("studio.trackingTargetTitle")}</h4>
            <IrisCheckbox
              variant="switch"
              checked={Boolean(activeProject.target?.enabled)}
              onchange={(checked) => {
                const defaultPos = status.anchor?.eye ?? [0, 1.65, 0];
                activeProject.target = {
                  enabled: checked,
                  source: activeProject.target?.source ?? "calibratedAnchor",
                  worldPos: activeProject.target?.worldPos ?? [...defaultPos],
                  blend: activeProject.target?.blend ?? 1.0,
                  confidence: 1.0,
                  capturedAtMs: activeProject.target?.capturedAtMs ?? 0,
                };
                refreshMathPreview(false);
              }}
              label={t("studio.keepTargetFramed")}
            />

            {#if activeProject.target}
              <label>
                <span>{t("studio.targetSourceLabel")}</span>
                <IrisSelect
                  items={targetSourceOptions}
                  value={activeProject.target.source ?? "calibratedAnchor"}
                  size="sm"
                  onchange={(val) => {
                    if (!activeProject.target) return;
                    const src = val as TargetSourceKind;
                    activeProject.target.source = src;
                    if (src === "calibratedAnchor" && status.anchor) {
                      activeProject.target.worldPos = [...status.anchor.eye];
                    }
                    refreshMathPreview(false);
                  }}
                />
              </label>

              <button
                type="button"
                class="ghost sm"
                disabled={!status.connected || !status.open}
                title={t("studio.captureTargetTooltip")}
                onclick={captureTargetFromCamera}
              >
                <Camera size={14} /> {t("studio.captureTargetCamera")}
              </button>

              <div class="xyz-grid">
                {#each ["X", "Y", "Z"] as axis, i (axis)}
                  <label>
                    <span>Obj {axis}</span>
                    <input
                      type="number"
                      step="0.1"
                      bind:value={activeProject.target.worldPos[i]}
                      oninput={() => refreshMathPreview(false)}
                    />
                  </label>
                {/each}
              </div>

              <label>
                <span>{t("studio.lookBlend", { pct: Math.round(activeProject.target.blend * 100) })}</span>
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  bind:value={activeProject.target.blend}
                  oninput={() => refreshMathPreview(false)}
                />
              </label>
            {/if}
          </div>
        {/if}
      </aside>
    </div>

    <!-- REGIÓN D: TIMELINE MULTIPISTA INFERIOR DEL MODO EDITOR -->
    <section class="editor-timeline card">
      <div class="ed-tl-toolbar">
        <div class="ed-transport">
          <button
            type="button"
            class="primary sm"
            disabled={!status.connected || !status.open}
            onclick={handleCaptureButton}
          >
            <CircleDot size={14} /> {t("studio.capturePoint")}
          </button>
          <button type="button" class="ghost sm" onclick={addLocalTestPoint}>
            <Plus size={14} /> {t("studio.localPoint")}
          </button>

          <!-- Barra de Transporte Completa Profesional -->
          <div class="transport-nav-group" role="group" aria-label="Transport">
            <button
              type="button"
              class="ghost sm icon-only"
              onclick={jumpToStart}
              title={t("studio.transport.toStart")}
            >
              <SkipBack size={13} />
            </button>
            <button
              type="button"
              class="ghost sm icon-only"
              onclick={() => stepTime(-1.0)}
              title={t("studio.transport.prev1s")}
            >
              <Rewind size={13} />
            </button>
            <button
              type="button"
              class="ghost sm icon-only"
              onclick={jumpToPrevKeyframe}
              title={t("studio.transport.prevKf")}
            >
              <ChevronLeft size={13} />
            </button>

            {#if playbackStatus.state === "playing"}
              <button
                type="button"
                class="primary sm icon-only play-toggle-btn"
                onclick={() => api.studioControl("pause")}
                title={t("studio.pause")}
              >
                <Pause size={14} />
              </button>
            {:else}
              <button
                type="button"
                class="primary sm icon-only play-toggle-btn"
                disabled={activeProject.keyframes.length === 0}
                onclick={handleTogglePlayback}
                title={status.connected && status.open ? t("studio.playTooltipVrc") : t("studio.playTooltipPreview")}
              >
                <Play size={14} />
              </button>
            {/if}

            <button
              type="button"
              class="ghost sm icon-only"
              onclick={jumpToNextKeyframe}
              title={t("studio.transport.nextKf")}
            >
              <ChevronRight size={13} />
            </button>
            <button
              type="button"
              class="ghost sm icon-only"
              onclick={() => stepTime(1.0)}
              title={t("studio.transport.next1s")}
            >
              <FastForward size={13} />
            </button>
            <button
              type="button"
              class="ghost sm icon-only"
              onclick={jumpToEnd}
              title={t("studio.transport.toEnd")}
            >
              <SkipForward size={13} />
            </button>
          </div>

          <span class="time-readout">
            {formatTimeReadout(playheadTime, activeProject.duration)} /
            {formatTimeReadout(activeProject.duration, activeProject.duration)}
          </span>
        </div>

        <div class="ed-tl-tools">
          <button
            type="button"
            class="ghost sm"
            class:pressed={showProjectConfig}
            onclick={() => (showProjectConfig = !showProjectConfig)}
            title={t("studio.config.subtitle")}
          >
            <Sliders size={14} /> {t("studio.config.button")} ({formatSecondsToMmSs(activeProject.duration)})
          </button>

          <IrisCheckbox
            variant="checkbox"
            size="sm"
            bind:checked={liveScrub}
            label="Live Scrub OSC"
            title={t("studio.liveScrubLabel")}
          />

          <button
            type="button"
            class="ghost sm"
            class:pressed={snapEnabled}
            onclick={() => (snapEnabled = !snapEnabled)}
            title={t("studio.snapGrid")}
          >
            <Magnet size={14} /> Snap
          </button>

          <label class="zoom-slider">
            <ZoomIn size={14} />
            <input type="range" min="1" max="8" step="0.5" bind:value={timelineZoom} />
            <span>{timelineZoom}×</span>
          </label>
        </div>
      </div>

      <div class="ed-tl-scroll">
        <div
          class="ed-tl-canvas"
          style="width: {100 * timelineZoom}%"
        >
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="tl-track-body"
            bind:this={timelineTrackEl}
            oncontextmenu={openTimelineContextMenu}
          >
            <!-- Regla de tiempo adaptativa -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="tl-ruler"
              onpointerdown={(e) => {
                playheadTime = Math.round(timeFromPointer(e.clientX) * 100) / 100;
                dragState = { kind: "playhead" };
                void refreshMathPreview(liveScrub);
              }}
            >
              {#each editorTimelineTicks as tick (tick.time)}
                <span class="tick" style="left: {tick.leftPct}%">
                  {tick.label}
                </span>
              {/each}
            </div>

          <!-- Pista 1: Trayectoria 3D (Posición X/Y/Z) con segmentos estirables -->
          <div class="tl-lane track-pos">
            <span class="lane-label"><Video size={12} /> {t("studio.trackPath")} (X/Y/Z)</span>

            <!-- Barras de segmento entre puntos para alargar/estirar directamente desde el timeline -->
            {#each activeProject.keyframes as kf, idx (kf.id + "-seg-ed")}
              {#if idx < activeProject.keyframes.length - 1}
                {@const nextKf = activeProject.keyframes[idx + 1]}
                {@const segLeftPct = (kf.time / Math.max(0.1, activeProject.duration)) * 100}
                {@const segWidthPct = Math.max(0, ((nextKf.time - kf.time) / Math.max(0.1, activeProject.duration)) * 100)}
                {@const segDuration = Math.max(0, nextKf.time - kf.time)}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="kf-segment-bar"
                  style="left: {segLeftPct}%; width: {segWidthPct}%;"
                  title={t("studio.segmentStretchTooltip", { from: formatKeyframeLabel(kf.label), to: formatKeyframeLabel(nextKf.label), duration: segDuration.toFixed(2) })}
                >
                  <span class="seg-fill"></span>
                  <span class="seg-badge">{segDuration.toFixed(1)}s</span>
                  <span
                    class="seg-stretch-handle"
                    title={t("studio.stretchHandleTooltip")}
                    onpointerdown={(e) => {
                      e.stopPropagation();
                      pushUndoSnapshot();
                      document.body.classList.add("timeline-dragging");
                      dragState = {
                        kind: "keyframe-stretch",
                        id: kf.id,
                        nextId: nextKf.id,
                        origNextTime: nextKf.time,
                      };
                    }}
                  ></span>
                </div>
              {/if}
            {/each}

            {#each activeProject.keyframes as kf (kf.id)}
              {@const leftPct = (kf.time / Math.max(0.1, activeProject.duration)) * 100}
              <button
                type="button"
                class="kf-pill"
                class:selected={selectedKeyframeId === kf.id}
                style="left: {leftPct}%"
                title={t("studio.keyframeTooltip", { label: formatKeyframeLabel(kf.label), time: kf.time.toFixed(2) })}
                onpointerdown={(e) => {
                  e.stopPropagation();
                  pushUndoSnapshot();
                  document.body.classList.add("timeline-dragging");
                  selectedKeyframeId = kf.id;
                  selectedEffectId = null;
                  inspectorTab = "selection";
                  playheadTime = kf.time;
                  dragState = { kind: "keyframe", id: kf.id };
                }}
                ondblclick={(e) => {
                  e.stopPropagation();
                  quickEditorState = { open: true, keyframe: kf, x: e.clientX, y: e.clientY };
                }}
                oncontextmenu={(e) => openKeyframeContextMenu(kf, e)}
              >
                <span class="kf-dot"></span>
                <span>{formatKeyframeLabel(kf.label)}</span>
                <small>{kf.time.toFixed(1)}s</small>
              </button>
            {/each}
          </div>

          <!-- Pista 2: Rotación (Quaternions SLERP) -->
          <div class="tl-lane track-rot">
            <span class="lane-label">{t("studio.trackRotation")} (SLERP)</span>
            {#each activeProject.keyframes as kf (kf.id)}
              {@const leftPct = (kf.time / Math.max(0.1, activeProject.duration)) * 100}
              <button
                type="button"
                class="rot-node"
                class:selected={selectedKeyframeId === kf.id}
                style="left: {leftPct}%"
                title="Yaw {kf.rawPose.rot[1].toFixed(0)}° / Pitch {kf.rawPose.rot[0].toFixed(0)}°"
                onclick={() => {
                  selectedKeyframeId = kf.id;
                  selectedEffectId = null;
                  inspectorTab = "selection";
                }}
              >
                {kf.rawPose.rot[1].toFixed(0)}°
              </button>
            {/each}
          </div>

          <!-- Pista 3: Óptica por Keyframe (Zoom / Enfoque / Apertura) -->
          <div class="tl-lane track-optics">
            <span class="lane-label">{t("studio.trackOptics")}</span>
            {#each activeProject.keyframes.filter((k) => k.look?.zoom != null || k.look?.aperture != null || k.look?.focalDistance != null) as kf (kf.id)}
              {@const leftPct = (kf.time / Math.max(0.1, activeProject.duration)) * 100}
              <button
                type="button"
                class="optic-node"
                style="left: {leftPct}%"
                onclick={() => {
                  selectedKeyframeId = kf.id;
                  selectedEffectId = null;
                  inspectorTab = "selection";
                }}
              >
                {kf.look?.zoom ? `${kf.look.zoom}mm` : ""}
                {kf.look?.aperture ? `f/${kf.look.aperture}` : ""}
              </button>
            {/each}
          </div>

          <!-- Pistas Multipista de Efectos Superponibles (Pistas FX 1, FX 2, FX 3) -->
          {#each [0, 1, 2] as trackIdx (trackIdx)}
            <div
              class="tl-lane fx-lane"
              class:is-drop-target={hoveredFxTrack === trackIdx}
              bind:this={fxLaneEls[trackIdx]}
            >
              <span class="lane-label"><Layers size={12} /> {t("studio.trackFx", { n: trackIdx + 1 })}</span>
              {#each activeProject.effects.filter((e) => e.track === trackIdx) as ef (ef.id)}
                {@const startPct = (ef.startTime / Math.max(0.1, activeProject.duration)) * 100}
                {@const widthPct = Math.max(
                  3,
                  (ef.duration / Math.max(0.1, activeProject.duration)) * 100,
                )}
                {@const catEntry = EFFECT_CATALOG.find((c) => c.kind === ef.kind)}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="fx-block cat-{catEntry?.category ?? 'movement'}"
                  class:selected={selectedEffectId === ef.id}
                  class:disabled={!ef.enabled}
                  style="left: {startPct}%; width: {widthPct}%"
                  title={t("studio.effectClipTooltip", { name: catEntry ? t(catEntry.labelKey) : ef.kind, start: ef.startTime.toFixed(1), end: (ef.startTime + ef.duration).toFixed(1) })}
                  onpointerdown={(e) => {
                    e.stopPropagation();
                    pushUndoSnapshot();
                    document.body.classList.add("timeline-dragging");
                    selectedEffectId = ef.id;
                    selectedKeyframeId = null;
                    inspectorTab = "selection";
                    const clickSec = timeFromPointer(e.clientX);
                    dragState = {
                      kind: "effect-move",
                      id: ef.id,
                      offsetSec: clickSec - ef.startTime,
                    };
                  }}
                  oncontextmenu={(e) => openEffectContextMenu(ef, e)}
                >
                  <span class="fx-title">{catEntry ? t(catEntry.labelKey) : ef.kind}</span>
                  <span
                    class="fx-resize-handle"
                    title={t("studio.dragDurationTooltip")}
                    onpointerdown={(e) => {
                      e.stopPropagation();
                      pushUndoSnapshot();
                      document.body.classList.add("timeline-dragging");
                      selectedEffectId = ef.id;
                      dragState = { kind: "effect-resize", id: ef.id };
                    }}
                  ></span>
                </div>
              {/each}
            </div>
          {/each}

          <!-- Cursor de reproducción (Playhead) con badge numérico -->
          <div
            class="playhead-line"
            style="left: {(playheadTime / Math.max(0.1, activeProject.duration)) * 100}%"
          >
            <span class="playhead-badge">
              {formatTimeReadout(playheadTime, activeProject.duration)}
            </span>
          </div>
        </div>
      </div>
    </div>
  </section>
  {/if}

  <!-- Menú Contextual Flotante -->
  <IrisContextMenu
    bind:open={contextMenu.open}
    x={contextMenu.x}
    y={contextMenu.y}
    items={contextMenu.items}
    onclose={() => (contextMenu.open = false)}
  />

  <!-- Editor Rápido de Keyframe en Popover Flotante al dar Doble Click -->
  {#if quickEditorState.open && quickEditorState.keyframe}
    <KeyframeQuickEditor
      bind:open={quickEditorState.open}
      bind:keyframe={quickEditorState.keyframe}
      x={quickEditorState.x}
      y={quickEditorState.y}
      maxDuration={activeProject.duration}
      canUpdateFromCamera={status.connected && status.open}
      onupdate={updateSelectedKeyframeFromCamera}
      onduplicate={duplicateSelectedKeyframe}
      ondelete={deleteSelectedKeyframe}
      onchange={() => refreshMathPreview(false)}
      onclose={() => {
        quickEditorState.open = false;
        refreshMathPreview(false);
      }}
    />
  {/if}

  <!-- Modal del Editor de Curvas Bézier Personalizadas -->
  <CurveEditorModal
    bind:open={showCurveEditorModal}
    curve={curveEditorTarget?.curve ?? [0.4, 0.0, 0.2, 1.0]}
    onapply={handleCustomCurveSaved}
    onclose={() => {
      showCurveEditorModal = false;
      curveEditorTarget = null;
    }}
  />

  <!-- Modal de Atajos de Teclado Personalizables (Estilo Premiere Pro) -->
  <KeybindingsModal
    bind:open={showKeybindingsModal}
    onclose={() => (showKeybindingsModal = false)}
  />

  <!-- Ghost flotante animado al arrastrar efectos desde la biblioteca -->
  {#if draggingLibraryEffect}
    {@const dragged = draggingLibraryEffect}
    <div
      class="library-drag-ghost"
      style="left: {dragged.x + 12}px; top: {dragged.y + 12}px;"
    >
      <Sparkles size={14} />
      <span>
        {t(EFFECT_CATALOG.find((c) => c.kind === dragged.kind)?.labelKey ?? dragged.kind)}
      </span>
      {#if hoveredFxTrack !== null}
        <span class="ghost-target-pill">{t("studio.trackFx", { n: hoveredFxTrack + 1 })}</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .studio-root {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .studio-topbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    flex-wrap: wrap;
  }
  .project-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .seq-picker-wrap {
    min-width: 220px;
    max-width: 280px;
  }
  .dur-select-field {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 150px;
  }
  .preset-motion-field {
    min-width: 280px;
  }
  .field-label {
    font-size: 0.82rem;
    color: var(--text-2);
    white-space: nowrap;
  }
  .seq-picker {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--surface);
    padding: 5px 10px;
    border-radius: 10px;
    color: var(--text-2);
  }
  .project-name-input,
  input[type="text"],
  input[type="number"],
  input[type="search"],
  select {
    background: var(--surface);
    color: var(--text);
    border-radius: 9px;
    padding: 6px 10px;
    font: inherit;
    font-size: 0.85rem;
  }
  .project-name-input {
    min-width: 210px;
    font-weight: 600;
  }
  .history-btns {
    display: inline-flex;
    gap: 4px;
  }
  button.sm {
    padding: 6px 12px;
    font-size: 0.82rem;
    border-radius: 10px;
  }
  button.icon-only {
    padding: 6px 9px;
    border-radius: 10px;
  }
  button.danger:hover:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }

  /* Selector [ Captura | Editor ] */
  .mode-switch {
    display: inline-flex;
    background: var(--bg);
    padding: 4px;
    border-radius: 12px;
  }
  .mode-tab {
    padding: 6px 16px;
    font-size: 0.85rem;
    font-weight: 600;
    border-radius: 9px;
    background: transparent;
    color: var(--text-2);
  }
  .mode-tab.active {
    background: var(--accent);
    color: #fff;
  }
  .badge-dur {
    font-size: 0.75rem;
    padding: 2px 7px;
    border-radius: 999px;
    background: rgba(var(--accent-color-rgb), 0.25);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }

  /* Panel de Configuración del Proyecto */
  .project-config-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 18px;
    border-color: rgba(var(--accent-color-rgb), 0.45);
    background: linear-gradient(180deg, var(--surface), var(--surface));
  }
  .pc-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
    flex-wrap: wrap;
  }
  .pc-head h3 {
    margin: 0;
    font-size: 0.95rem;
    color: var(--text-2);
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .pc-head .sub {
    margin: 3px 0 0;
    font-size: 0.78rem;
    color: var(--text-2);
  }
  .pc-grid {
    display: flex;
    align-items: flex-end;
    gap: 14px;
    flex-wrap: wrap;
  }
  .pc-field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 0.8rem;
    color: var(--text-2);
  }
  .pc-label {
    font-size: 0.76rem;
    font-weight: 600;
    color: var(--text-2);
  }
  .sm-switch .mode-tab {
    padding: 5px 11px;
    font-size: 0.78rem;
  }
  .pc-equiv {
    flex: 1;
    min-width: 210px;
  }
  .equiv-readout {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-radius: 9px;
    background: var(--surface);
    color: var(--text);
    font-size: 0.84rem;
    font-variant-numeric: tabular-nums;
  }
  .equiv-readout code {
    color: var(--text-2);
    font-weight: 600;
  }
  .pc-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .conflict-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border-radius: 11px;
    background: rgba(255, 112, 141, 0.12);
    border: 1px solid rgba(255, 112, 141, 0.45);
    font-size: 0.82rem;
  }
  .conflict-box strong {
    color: #ff9bb0;
  }
  .conflict-box p {
    margin: 0;
    color: var(--text);
  }
  .conflict-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .studio-alert {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    padding: 10px 14px;
    border-radius: 12px;
    font-size: 0.86rem;
    background: var(--surface-raised);
  }
  .studio-alert.error {
    border-color: var(--danger);
  }
  .studio-alert.armed {
    border-color: var(--ok);
  }
  .studio-alert.advanced-notice {
    border-color: var(--text-2);
  }

  .guide-strip {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 12px;
    padding: 10px 16px;
    font-size: 0.82rem;
    color: var(--text-2);
    background: var(--surface-raised);
  }
  .guide-strip strong {
    color: var(--text-2);
  }

  .capture-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
    padding: 12px 16px;
  }
  .capture-primary-actions,
  .duration-controls,
  .playback-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .capture-hero-btn {
    padding: 10px 18px;
    font-size: 0.92rem;
  }
  .capture-hero-btn.armed {
    background: #22c55e;
    color: #081c15;
  }
  .rec-active {
    border-color: var(--danger) !important;
    color: var(--danger) !important;
  }
  .dur-field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.82rem;
    color: var(--text-2);
  }
  .dur-field input {
    width: 72px;
  }
  .scale-check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.8rem;
    color: var(--text-2);
    cursor: pointer;
  }
  .time-readout {
    font-variant-numeric: tabular-nums;
    font-size: 0.82rem;
    color: var(--text-2);
    font-weight: 600;
    padding: 0 6px;
  }

  .recording-refine-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
    padding: 12px 16px;
  }
  .recording-refine-bar .sub {
    margin: 2px 0 0;
    font-size: 0.8rem;
    color: var(--text-2);
  }
  .refine-controls {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    font-size: 0.8rem;
  }

  .capture-main-grid {
    display: grid;
    grid-template-columns: 1.35fr 1fr;
    gap: 14px;
    align-items: start;
  }
  @media (max-width: 1020px) {
    .capture-main-grid {
      grid-template-columns: 1fr;
    }
  }
  .capture-left-col {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .simple-timeline-card {
    padding: 14px;
  }
  .tl-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 10px;
  }
  .tl-header h3 {
    margin: 0;
    font-size: 0.9rem;
    color: var(--text-2);
  }
  .quick-effect-adder {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  /* Pistas de Timeline compartidas */
  .timeline-lanes,
  .ed-tl-canvas {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg);
    border-radius: 12px;
    padding: 8px 12px 12px;
    user-select: none;
    overflow: hidden;
  }
  .tl-track-body {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0 36px;
  }
  .tl-ruler {
    position: relative;
    height: 22px;
    border-bottom: 1px solid var(--border-subtle);
    cursor: pointer;
  }
  .tl-ruler .tick {
    position: absolute;
    top: 2px;
    transform: translateX(-50%);
    font-size: 0.7rem;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .tl-lane {
    position: relative;
    height: 44px;
    background: var(--surface);
    border-radius: 8px;
    display: flex;
    align-items: center;
  }
  .lane-label {
    position: absolute;
    left: 8px;
    top: 4px;
    font-size: 0.68rem;
    color: var(--text-2);
    pointer-events: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    z-index: 1;
  }
  .lane-empty {
    margin: 0 auto;
    font-size: 0.78rem;
    color: var(--text-2);
    pointer-events: none;
  }
  .kf-pill {
    position: absolute;
    top: 10px;
    transform: translateX(-50%);
    padding: 4px 9px;
    border-radius: 999px;
    background: var(--surface-raised);
    border: 1.5px solid var(--accent);
    color: var(--text);
    font-size: 0.75rem;
    font-weight: 600;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    z-index: 2;
    white-space: nowrap;
  }
  .kf-pill.selected {
    background: var(--accent);
    color: #fff;
    border-color: #fff;
    box-shadow: 0 0 12px rgba(var(--accent-color-rgb), 0.6);
  }
  .kf-pill small {
    opacity: 0.8;
    font-size: 0.68rem;
  }
  .kf-dot {
    width: 7px;
    height: 7px;
    border-radius: 999px;
    background: var(--text-2);
  }
  .rot-node,
  .optic-node {
    position: absolute;
    top: 11px;
    transform: translateX(-50%);
    padding: 3px 7px;
    font-size: 0.7rem;
    border-radius: 6px;
    background: rgba(39, 65, 200, 0.35);
    color: var(--text-2);
    z-index: 2;
  }
  .fx-block {
    position: absolute;
    top: 12px;
    height: 26px;
    border-radius: 7px;
    background: linear-gradient(90deg, rgba(var(--accent-color-rgb), 0.55), var(--surface));
    border: 1px solid var(--text-2);
    display: flex;
    align-items: center;
    padding: 0 8px;
    cursor: grab;
    z-index: 2;
    overflow: hidden;
  }
  .fx-block.cat-micro {
    background: linear-gradient(90deg, rgba(140, 82, 255, 0.55), var(--surface));
  }
  .fx-block.cat-optics {
    background: linear-gradient(90deg, rgba(38, 198, 218, 0.5), rgba(39, 65, 200, 0.8));
  }
  .fx-block.selected {
    border: 2px solid #ffffff;
    box-shadow: 0 0 12px var(--border-strong);
  }
  .fx-block.disabled {
    opacity: 0.4;
  }
  .fx-title {
    font-size: 0.72rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fx-resize-handle {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: 8px;
    cursor: ew-resize;
    background: rgba(255, 255, 255, 0.22);
  }
  .playhead-line {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--danger);
    pointer-events: none;
    z-index: 6;
    box-shadow: 0 0 10px rgba(255, 112, 141, 0.6);
  }
  .playhead-line::before {
    content: "";
    position: absolute;
    top: 0;
    left: -5px;
    width: 12px;
    height: 12px;
    border-radius: 999px;
    background: var(--danger);
    box-shadow: 0 0 8px rgba(255, 112, 141, 0.8);
  }
  .playhead-badge {
    position: absolute;
    top: -20px;
    left: 50%;
    transform: translateX(-50%);
    font-size: 0.68rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: #ffffff;
    background: #ff4770;
    border: 1px solid rgba(255, 255, 255, 0.4);
    border-radius: 6px;
    padding: 2px 6px;
    white-space: nowrap;
    box-shadow: 0 3px 8px rgba(0, 0, 0, 0.5);
    pointer-events: none;
  }

  /* Quick Inspector del Modo Captura */
  .quick-inspector {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .qi-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .badge {
    font-size: 0.75rem;
    padding: 3px 9px;
    border-radius: 999px;
    background: var(--surface-hover);
    color: var(--text-2);
  }
  .qi-row,
  .qi-sliders {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 0.82rem;
  }
  .qi-actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  /* MODO EDITOR (4 Regiones) */
  .editor-grid {
    display: grid;
    grid-template-columns: 265px 1fr 320px;
    gap: 12px;
    align-items: stretch;
  }
  @media (max-width: 1180px) {
    .editor-grid {
      grid-template-columns: 1fr;
    }
  }
  .editor-library {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: 455px;
    overflow-y: auto;
  }
  .editor-inspector {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: 455px;
    overflow-y: auto;
    overflow-x: hidden !important;
  }
  .lib-search {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg);
    border-radius: 9px;
    padding: 4px 9px;
  }
  .lib-search input {
    border: 0;
    background: transparent;
    width: 100%;
    padding: 3px;
    outline: none;
  }
  .lib-tabs,
  .insp-tabs {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .lib-tab {
    flex: 1;
    justify-content: center;
    padding: 5px 8px;
    font-size: 0.74rem;
    border-radius: 8px;
    background: var(--surface-raised);
    color: var(--text-2);
  }
  .lib-tab.active {
    color: var(--text);
    background: var(--accent-subtle);
  }
  .lib-items {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
  }
  .lib-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 10px;
    background: var(--surface-raised);
  }
  .lib-item strong {
    font-size: 0.8rem;
    display: block;
  }
  .lib-item p {
    margin: 2px 0 0;
    font-size: 0.72rem;
    color: var(--text-2);
  }

  .editor-center {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .telemetry-bar {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    padding: 9px 12px;
    font-size: 0.74rem;
  }
  .tele-col {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .tele-label {
    font-weight: 600;
    font-size: 0.7rem;
  }
  .tele-label.vrc {
    color: var(--ok);
  }
  .tele-label.base {
    color: var(--text-2);
  }
  .tele-label.final {
    color: #38bdf8;
  }
  .telemetry-bar code {
    font-size: 0.72rem;
    color: var(--text);
  }
  .telemetry-bar code.muted {
    color: var(--text-2);
  }

  .insp-body {
    display: flex;
    flex-direction: column;
    gap: 9px;
    font-size: 0.78rem;
    min-width: 0;
    max-width: 100%;
    overflow-x: hidden;
  }
  .insp-body input[type="range"],
  .qi-sliders input[type="range"] {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    max-width: 100%;
    height: 5px;
    border-radius: 999px;
    background: var(--surface-hover);
    outline: none;
    cursor: pointer;
    box-sizing: border-box;
    transition: background 150ms ease;
  }
  .insp-body input[type="range"]:hover,
  .qi-sliders input[type="range"]:hover {
    background: var(--surface-hover);
  }
  .insp-body input[type="range"]::-webkit-slider-thumb,
  .qi-sliders input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    border: 2px solid #ffffff;
    box-shadow: 0 0 6px var(--accent-glow, rgba(var(--accent-color-rgb), 0.45));
    cursor: grab;
    transition: transform 120ms ease, box-shadow 120ms ease;
  }
  .insp-body input[type="range"]::-webkit-slider-thumb:hover,
  .qi-sliders input[type="range"]::-webkit-slider-thumb:hover {
    transform: scale(1.2);
    box-shadow: 0 0 10px var(--accent-glow, rgba(var(--accent-color-rgb), 0.65));
  }
  .insp-body input[type="number"],
  .insp-body input[type="text"] {
    width: 100%;
    max-width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }
  .insp-body h4 {
    margin: 6px 0 0;
    font-size: 0.76rem;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .insp-body label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    color: var(--text-2);
    min-width: 0;
  }
  .insp-row-two {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 6px;
    min-width: 0;
  }
  .insp-row-two > * {
    min-width: 0;
  }
  .xyz-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
    min-width: 0;
  }
  .xyz-grid > * {
    min-width: 0;
  }
  .xyz-grid input {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }
  .local-readout {
    margin: 0;
    font-size: 0.72rem;
    color: var(--text-2);
  }
  .insp-actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 4px;
  }
  .insp-head-inline {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .insp-empty {
    color: var(--text-2);
    font-size: 0.82rem;
    margin: 18px 0;
    text-align: center;
  }
  .sep {
    border: 0;
    border-top: 1px solid var(--border-subtle);
    margin: 4px 0;
  }

  .editor-timeline {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .ed-tl-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
  }
  .ed-transport,
  .ed-tl-tools {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .transport-nav-group {
    display: inline-flex;
    align-items: center;
    background: var(--surface-raised);
    border-radius: 12px;
    padding: 3px;
    gap: 2px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  }
  .transport-nav-group :global(.icon-only) {
    padding: 5px 8px;
    border-radius: 8px;
    border: 0;
    color: var(--text-2);
    min-width: 30px;
    height: 30px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    transition: background 120ms ease, color 120ms ease, transform 100ms ease;
  }
  .transport-nav-group :global(.icon-only:hover:not(:disabled)) {
    background: var(--surface-hover);
    color: var(--text);
  }
  .transport-nav-group :global(.icon-only:active:not(:disabled)) {
    transform: scale(0.95);
  }
  .transport-nav-group :global(.play-toggle-btn) {
    background: var(--accent) !important;
    color: var(--accent-foreground, #ffffff) !important;
    font-weight: 600;
    min-width: 36px;
    height: 30px;
    padding: 5px 12px;
    border-radius: 9px;
    box-shadow: 0 0 10px var(--accent-glow);
    transition: filter 120ms ease, box-shadow 120ms ease, transform 100ms ease;
  }
  .transport-nav-group :global(.play-toggle-btn:hover:not(:disabled)) {
    filter: brightness(1.15);
    box-shadow: 0 0 16px var(--accent-glow);
    transform: scale(1.04);
  }
  .transport-nav-group :global(.play-toggle-btn:active:not(:disabled)) {
    transform: scale(0.96);
  }
  .zoom-slider {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 0.78rem;
    color: var(--text-2);
    background: var(--surface-raised);
    border-radius: 10px;
    padding: 4px 10px;
  }
  .zoom-slider input[type="range"] {
    -webkit-appearance: none;
    appearance: none;
    width: 85px;
    height: 4px;
    border-radius: 999px;
    background: var(--surface-hover);
    outline: none;
    cursor: pointer;
    transition: background 150ms ease;
  }
  .zoom-slider input[type="range"]:hover {
    background: var(--surface-hover);
  }
  .zoom-slider input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    border: 2px solid #ffffff;
    box-shadow: 0 0 6px var(--accent-glow, rgba(var(--accent-color-rgb), 0.5));
    cursor: grab;
    transition: transform 120ms ease, box-shadow 120ms ease;
  }
  .zoom-slider input[type="range"]::-webkit-slider-thumb:hover {
    transform: scale(1.2);
    box-shadow: 0 0 10px var(--accent-glow, rgba(var(--accent-color-rgb), 0.7));
  }
  .ed-tl-scroll {
    overflow-x: auto;
  }
  .track-pos {
    height: 40px;
  }
  .track-rot,
  .track-optics {
    height: 34px;
  }

  /* Preset pills in Duration Bar */
  .dur-preset-strip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 4px;
    background: var(--surface-raised);
    border-radius: 8px;
  }
  .dur-pill {
    background: transparent;
    border: 0;
    color: var(--text-2);
    font-size: 0.74rem;
    font-weight: 500;
    padding: 3px 7px;
    border-radius: 6px;
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease;
  }
  .dur-pill:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .dur-pill.active {
    background: var(--accent);
    color: #ffffff;
    font-weight: 600;
  }
  .dur-pill.fit-warn-btn {
    background: rgba(245, 158, 11, 0.15);
    border: 1px solid rgba(245, 158, 11, 0.4);
    color: #fbbf24;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-weight: 600;
  }
  .dur-pill.fit-warn-btn:hover {
    background: rgba(245, 158, 11, 0.28);
    color: #fef3c7;
  }
  .dur-unit-select-wrap {
    min-width: 90px;
  }

  /* Connecting Segment Bars between Keyframe points */
  .kf-segment-bar {
    position: absolute;
    top: 13px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1;
    pointer-events: auto;
  }
  .seg-fill {
    position: absolute;
    left: 0;
    right: 0;
    height: 3px;
    background: linear-gradient(90deg, rgba(var(--accent-color-rgb), 0.5), var(--surface-hover));
    border-radius: 2px;
    pointer-events: none;
  }
  .seg-badge {
    position: relative;
    z-index: 2;
    font-size: 0.64rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
    background: var(--surface);
    border-radius: 999px;
    padding: 1px 6px;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }
  .seg-stretch-handle {
    position: absolute;
    right: -4px;
    top: 0;
    bottom: 0;
    width: 9px;
    cursor: ew-resize;
    z-index: 3;
    border-radius: 4px;
    transition: background 0.15s ease;
  }
  .seg-stretch-handle:hover {
    background: rgba(255, 255, 255, 0.4);
    box-shadow: 0 0 8px rgba(255, 255, 255, 0.6);
  }

  /* Multi-track FX Lane Drop Target Highlighting */
  .tl-lane.fx-lane {
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .tl-lane.fx-lane.is-drop-target {
    background: rgba(var(--accent-color-rgb), 0.22);
    border-color: rgba(var(--accent-color-rgb), 0.65);
    box-shadow: inset 0 0 14px rgba(var(--accent-color-rgb), 0.25);
  }

  /* Library Draggable Item */
  .draggable-item {
    cursor: grab;
    user-select: none;
    transition: transform 0.12s ease, border-color 0.12s ease;
  }
  .draggable-item:active {
    cursor: grabbing;
    transform: scale(0.98);
  }

  /* Library Drag Ghost Animation */
  .library-drag-ghost {
    position: fixed;
    z-index: 9999;
    pointer-events: none;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    background: rgba(22, 14, 40, 0.92);
    backdrop-filter: blur(12px);
    border: 1.5px solid var(--accent);
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.6), 0 0 16px rgba(var(--accent-color-rgb), 0.4);
    border-radius: 10px;
    color: #ffffff;
    font-size: 0.8rem;
    font-weight: 600;
    transform: translate(-50%, -50%);
    animation: ghostPulse 1.5s infinite alternate ease-in-out;
  }
  .ghost-target-pill {
    font-size: 0.68rem;
    padding: 2px 6px;
    border-radius: 6px;
    background: var(--accent);
    color: #fff;
  }
  @keyframes ghostPulse {
    from {
      box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5), 0 0 10px rgba(var(--accent-color-rgb), 0.3);
    }
    to {
      box-shadow: 0 12px 32px rgba(0, 0, 0, 0.7), 0 0 20px rgba(var(--accent-color-rgb), 0.6);
    }
  }

  /* Modern Accordion in Right Inspector */
  .insp-accordion-btn {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    padding: 7px 10px;
    background: var(--surface-raised);
    border-radius: 8px;
    color: var(--text-2);
    font-size: 0.76rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
    margin-top: 4px;
  }
  .insp-accordion-btn:hover {
    background: var(--surface-hover);
  }
  :global(.insp-chevron) {
    transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
  }
  :global(.insp-chevron.rotated) {
    transform: rotate(-90deg);
  }

  /* Custom Curve Button */
  .curve-field-col {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .curve-field-col > label {
    min-width: 0;
  }
  .custom-curve-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 0.72rem;
    padding: 3px 8px;
    align-self: flex-start;
    border-radius: 6px;
    color: var(--text-2);
  }
  .custom-curve-btn:hover {
    border-color: var(--text-2);
    color: var(--text-2);
  }
  .custom-curve-btn.has-curve {
    color: var(--text-2);
    background: var(--accent-subtle);
  }

  /* =========================================================================
     MODO CLARO (WHITE THEME) PARA VELZA IRIS STUDIO
     ========================================================================= */
</style>
