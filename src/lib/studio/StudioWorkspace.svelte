<script lang="ts">
  import { onMount } from "svelte";
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
    type SequenceProject,
    type Status,
    type StudioPlaybackStatus,
    type TargetSourceKind,
    type TelemetryReport,
    type TrajectoryPreview,
    type Transition,
  } from "$lib/api";
  import { t, type Key } from "$lib/i18n/index.svelte";
  import { createEffectClip, EFFECT_CATALOG, type EffectCategory } from "$lib/studio/catalog";
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
      description: `${s.keyframes.length} puntos · ${s.duration}s`,
    })),
    ...(!sequences.some((s) => s.id === activeProject.id)
      ? [
          {
            value: activeProject.id,
            label: `${activeProject.name} *`,
            icon: Film,
            description: "Sin guardar",
          },
        ]
      : []),
  ]);

  const cinematicProfileOptions: SelectOption[] = [
    { value: "cinematic", label: "Cinemático", description: "Inercia suave y amortiguada" },
    { value: "precision", label: "Precisión", description: "Respuesta rígida sin rebote" },
    { value: "organic", label: "Orgánico", description: "Micro-variaciones sutiles" },
  ];

  const presetMotionOptions = $derived<SelectOption[]>(
    presetMotions.map((m) => ({
      value: m.name,
      label: m.name,
      icon: Orbit,
      description: `${m.duration}s · ${m.loop}`,
    }))
  );

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

  const defaultCinematic: CinematicConfig = {
    profile: "cinematic",
    inertiaTau: 0.18,
    maxLinearSpeed: 4.5,
    maxLinearAccel: 8.0,
    maxAngularSpeed: 180.0,
    organicVariation: 0.0,
    organicSeed: 42,
  };

  const defaultCalibration: CalibrationConfig = {
    enabled: true,
    autoCorrect: false,
    posOffset: [0, 0, 0],
    yawOffsetDeg: 0,
    pitchOffsetDeg: 0,
    compositionX: 0,
    compositionY: 0,
    leadRoom: 0,
  };

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
  }

  function syncActiveProjectToLibrary(proj: SequenceProject) {
    const idx = sequences.findIndex((s) => s.id === proj.id);
    if (idx >= 0) {
      sequences[idx] = structuredClone(proj);
    }
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
    }, `Movimiento «${name}» cargado como secuencia editable en Iris Studio.`);
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
          k.look.zoom !== null ||
          k.look.exposure !== null ||
          k.look.focalDistance !== null ||
          k.look.aperture !== null,
      ),
  );

  const filteredCatalog = $derived(
    EFFECT_CATALOG.filter((item) => {
      if (libraryCategory !== "presets" && item.category !== libraryCategory) return false;
      if (!librarySearch.trim()) return true;
      const q = librarySearch.toLowerCase();
      return (
        t(item.labelKey).toLowerCase().includes(q) ||
        item.shortDesc.toLowerCase().includes(q) ||
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

  function pointLabel(idx: number): string {
    const letter = String.fromCharCode(65 + (idx % 26));
    return idx < 26 ? `Punto ${letter}` : `Punto ${letter}${Math.floor(idx / 26) + 1}`;
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
    syncActiveProjectToLibrary(activeProject);
    refreshMathPreview(false);
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
    }, `${pointLabel(nextIdx)} capturado desde la cámara de VRChat (${targetTime.toFixed(1)} s).`);
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
    }, `${selectedKeyframe.label} actualizado con la posición actual de VRChat.`);
  }

  function duplicateSelectedKeyframe() {
    if (!selectedKeyframe) return;
    pushUndoSnapshot();
    const newTime = Math.min(activeProject.duration, Math.round((selectedKeyframe.time + 1.5) * 10) / 10);
    const copy: Keyframe = {
      ...structuredClone(selectedKeyframe),
      id: `kf-dup-${Date.now()}`,
      label: `${selectedKeyframe.label} (copia)`,
      time: newTime,
    };
    activeProject.keyframes = [...activeProject.keyframes, copy].sort((a, b) => a.time - b.time);
    selectedKeyframeId = copy.id;
    playheadTime = copy.time;
    void refreshMathPreview(false);
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

  function addEffectToTimeline(kind: EffectKind, preferredTrack?: number) {
    pushUndoSnapshot();
    const entry = EFFECT_CATALOG.find((e) => e.kind === kind);
    const defaultTrack =
      preferredTrack ??
      (entry?.category === "optics" ? 0 : entry?.category === "movement" ? 0 : 1);
    const clip = createEffectClip(kind, playheadTime, activeProject.duration, defaultTrack);
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

  function duplicateSelectedEffect() {
    if (!selectedEffect) return;
    pushUndoSnapshot();
    const nextStart = Math.min(
      Math.max(0, activeProject.duration - selectedEffect.duration),
      selectedEffect.startTime + selectedEffect.duration,
    );
    const copy: EffectClip = {
      ...structuredClone(selectedEffect),
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

  function createNewProject() {
    pushUndoSnapshot();
    activeProject = {
      schemaVersion: 2,
      id: `seq-${Date.now()}`,
      name: "Nueva secuencia cinematográfica",
      duration: DEFAULT_DURATION_SEC,
      durationUnit: "seconds",
      durationInputMode: "valueUnit",
      space: "anchor",
      anchorSnapshot: status.anchor,
      trajectoryScale: 1,
      target: null,
      cinematic: structuredClone(defaultCinematic),
      calibration: structuredClone(defaultCalibration),
      keyframes: [],
      effects: [],
      rawRecording: [],
    };
    selectedKeyframeId = null;
    selectedEffectId = null;
    playheadTime = 0;
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
    const startAt = playheadTime >= activeProject.duration - 0.05 ? 0 : playheadTime;
    await run(async () => {
      await api.playSequence(activeProject, startAt, playbackStatus.looping, liveOsc);
    });
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
      }, "Recorrido grabado y convertido en puntos editables.");
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
    }, `Trayectoria simplificada a ${activeProject.keyframes.length} puntos sin alterar las muestras originales.`);
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
    }, "Objetivo de seguimiento capturado desde la posición actual de la cámara.");
  }

  // Interacción con el timeline (arrastrar cursor, keyframes y clips)
  function timeFromPointer(clientX: number): number {
    if (!timelineTrackEl) return playheadTime;
    const rect = timelineTrackEl.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / Math.max(1, rect.width)));
    return ratio * activeProject.duration;
  }

  function onTimelinePointerMove(e: PointerEvent) {
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
        // Mover un punto en el tiempo jamás altera sus coordenadas originales (rawPose / localPose)
        activeProject.keyframes[idx].time = tSec;
        activeProject.keyframes.sort((a, b) => a.time - b.time);
        playheadTime = tSec;
        void refreshMathPreview(liveScrub);
      }
    } else if (ds.kind === "effect-move") {
      const idx = activeProject.effects.findIndex((ef) => ef.id === ds.id);
      if (idx >= 0) {
        const ef = activeProject.effects[idx];
        const maxStart = Math.max(0, activeProject.duration - ef.duration);
        ef.startTime = Math.min(maxStart, Math.max(0, snapTime(rawSec - ds.offsetSec)));
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
    dragState = null;
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
    const poll = setInterval(async () => {
      try {
        const st = await api.studioStatus();
        playbackStatus = st;
        if (st.state === "playing") {
          playheadTime = st.currentTime;
          if (st.frame) evaluatedFrame = st.frame;
        }
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

    const onKeyDown = (ev: KeyboardEvent) => {
      if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "z") {
        ev.preventDefault();
        if (ev.shiftKey) redo();
        else undo();
      } else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "y") {
        ev.preventDefault();
        redo();
      }
    };
    window.addEventListener("keydown", onKeyDown);

    return () => {
      clearInterval(poll);
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
            if (found) {
              activeProject = structuredClone(found);
              selectedKeyframeId = activeProject.keyframes[0]?.id ?? null;
              selectedEffectId = null;
              playheadTime = 0;
              undoStack = [];
              redoStack = [];
            }
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
    <div class="mode-switch" role="tablist" aria-label="Modo de Iris Studio">
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
          Cerrar
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
      <button type="button" class="ghost sm" onclick={() => (captureArmed = false)}>Cancelar</button>
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
              : "Registra la posición y rotación exactas de tu cámara de VRChat"}
          onclick={handleCaptureButton}
        >
          <CircleDot size={18} />
          {captureArmed ? t("studio.captureArmed") : t("studio.capturePoint")}
        </button>

        <button
          type="button"
          class="ghost sm"
          title="Añade un punto de prueba local para diseñar o practicar sin tener VRChat abierto"
          onclick={addLocalTestPoint}
        >
          <Plus size={15} /> {t("studio.addSimPoint")}
        </button>

        <button
          type="button"
          class="ghost sm"
          class:rec-active={recordingActive}
          disabled={!status.connected || !status.open}
          title="Mueve tu cámara libremente mientras grabas; Iris convertirá el recorrido en puntos editables"
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
        <select
          aria-label={t("studio.config.unitLabel")}
          value={configUnit}
          onchange={(e) => switchConfigUnit(e.currentTarget.value as DurationUnit)}
        >
          <option value="seconds">{t("studio.config.unitSeconds")} (s)</option>
          <option value="minutes">{t("studio.config.unitMinutes")} (min)</option>
        </select>
        <button
          type="button"
          class="ghost sm"
          onclick={() => (showProjectConfig = !showProjectConfig)}
          title={t("studio.config.subtitle")}
        >
          <Sliders size={14} /> {formatSecondsToMmSs(activeProject.duration)}
        </button>
        <div class="dur-select-field" title="Perfil del motor cinematográfico (inercia, suavizado y carácter de movimiento)">
          <span class="field-label">Perfil</span>
          <IrisSelect
            items={cinematicProfileOptions}
            value={activeProject.cinematic?.profile ?? "cinematic"}
            size="sm"
            onchange={(val) => applyCinematicProfile(val as CinematicProfile)}
          />
        </div>
        {#if presetMotions.length > 0}
          <div class="dur-select-field preset-motion-field" title="Convierte un movimiento predefinido en una secuencia editable de puntos">
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
              <Orbit size={14} /> Cargar recorrido
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
            class="ghost sm"
            disabled={activeProject.keyframes.length === 0}
            title="Previsualiza el movimiento en el mapa espacial sin necesidad de conectar con VRChat"
            onclick={() => startPlayback(false)}
          >
            <Eye size={15} /> {t("studio.previewPlay")}
          </button>
          <button
            type="button"
            class="primary sm"
            disabled={activeProject.keyframes.length === 0 || !status.connected || !status.open}
            title="Mueve la cámara de VRChat en tiempo real a 60 Hz siguiendo tu secuencia (no requiere VRC+)"
            onclick={() => startPlayback(true)}
          >
            <Play size={15} /> {t("studio.livePlay")}
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
          <strong>Recorrido continuo grabado ({activeProject.rawRecording.length} muestras originales intactas)</strong>
          <p class="sub">Ajusta cuánto quieres suavizar o simplificar el recorrido sin perder tu grabación original:</p>
        </div>
        <div class="refine-controls">
          <label>
            Tolerancia (m): <strong>{posTolM.toFixed(2)}m</strong>
            <input type="range" min="0.01" max="0.2" step="0.01" bind:value={posTolM} />
          </label>
          <label>
            Suavizado de pulso: <strong>{smoothingPasses} pasadas</strong>
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
              Línea de tiempo ({activeProject.duration} s · {formatSecondsToMmSs(activeProject.duration)})
            </h3>
            <div class="quick-effect-adder">
              <select bind:value={quickEffectKind} aria-label={t("studio.addEffect")}>
                {#each EFFECT_CATALOG as item (item.kind)}
                  <option value={item.kind}>{t(item.labelKey)}</option>
                {/each}
              </select>
              <button type="button" class="ghost sm" onclick={() => addEffectToTimeline(quickEffectKind)}>
                <Plus size={14} /> {t("studio.addEffect")}
              </button>
            </div>
          </div>

          <!-- Pista interactiva con Regla Temporal + Pista de Recorrido + Pista de Efectos -->
          <div class="timeline-lanes" bind:this={timelineTrackEl}>
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

            <!-- Pista 1: Recorrido de cámara (Puntos A, B, C...) -->
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
                <span class="lane-empty">Pulsa «Capturar posición» para colocar el Punto A</span>
              {/if}
              {#each activeProject.keyframes as kf (kf.id)}
                {@const leftPct = (kf.time / Math.max(0.1, activeProject.duration)) * 100}
                <button
                  type="button"
                  class="kf-pill"
                  class:selected={selectedKeyframeId === kf.id}
                  style="left: {leftPct}%"
                  title="{kf.label} ({kf.time.toFixed(2)}s) — Arrastra para cambiar su instante temporal"
                  onpointerdown={(e) => {
                    e.stopPropagation();
                    pushUndoSnapshot();
                    selectedKeyframeId = kf.id;
                    selectedEffectId = null;
                    playheadTime = kf.time;
                    dragState = { kind: "keyframe", id: kf.id };
                    void refreshMathPreview(false);
                  }}
                >
                  <span class="kf-dot"></span>
                  <span>{kf.label}</span>
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
                    selectedEffectId = ef.id;
                    selectedKeyframeId = null;
                    const clickSec = timeFromPointer(e.clientX);
                    dragState = {
                      kind: "effect-move",
                      id: ef.id,
                      offsetSec: clickSec - ef.startTime,
                    };
                  }}
                >
                  <span class="fx-title">{catEntry ? t(catEntry.labelKey) : ef.kind}</span>
                  <span
                    class="fx-resize-handle"
                    title="Arrastra para ajustar duración"
                    onpointerdown={(e) => {
                      e.stopPropagation();
                      pushUndoSnapshot();
                      selectedEffectId = ef.id;
                      dragState = { kind: "effect-resize", id: ef.id };
                    }}
                  ></span>
                </div>
              {/each}
            </div>

            <!-- Cursor de reproducción (Playhead) -->
            <div
              class="playhead-line"
              style="left: {(playheadTime / Math.max(0.1, activeProject.duration)) * 100}%"
            ></div>
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
                aria-label="Nombre del punto"
              />
              <span class="badge">
                {selectedKeyframe.capturedAtMs > 0 ? "Lectura real VRChat" : "Punto local"} ·
                {selectedKeyframe.time.toFixed(2)}s
              </span>
            </div>

            <div class="qi-row">
              <label>
                <span>{t("studio.transition")}</span>
                <select
                  bind:value={selectedKeyframe.transition}
                  onchange={() => refreshMathPreview(false)}
                >
                  <option value="linear">{t("studio.trans.linear")}</option>
                  <option value="smooth">{t("studio.trans.smooth")}</option>
                  <option value="slowIn">{t("studio.trans.slowIn")}</option>
                  <option value="slowOut">{t("studio.trans.slowOut")}</option>
                  <option value="smoothInOut">{t("studio.trans.smoothInOut")}</option>
                </select>
              </label>

              <label title="Define si la cámara cruza este punto con velocidad continua o si frena suavemente en él">
                <span>Continuidad en punto</span>
                <select
                  value={selectedKeyframe.knotMode ?? "passThrough"}
                  onchange={(e) => {
                    selectedKeyframe.knotMode = e.currentTarget.value as "passThrough" | "stop";
                    refreshMathPreview(false);
                  }}
                >
                  <option value="passThrough">Paso continuo (C1 sin frenar)</option>
                  <option value="stop">Parada suave en punto</option>
                </select>
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
                <span>Intensidad ({selectedEffect.intensity.toFixed(1)}×)</span>
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
                <span>Duración clip (s)</span>
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
                <span>Velocidad ({selectedEffect.speed.toFixed(1)}×)</span>
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
                <Trash2 size={14} /> Eliminar efecto
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
            placeholder="Buscar movimiento o efecto…"
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
            Movimientos
          </button>
          <button
            type="button"
            class="lib-tab"
            class:active={libraryCategory === "micro"}
            onclick={() => (libraryCategory = "micro")}
          >
            Microefectos
          </button>
          <button
            type="button"
            class="lib-tab"
            class:active={libraryCategory === "optics"}
            onclick={() => (libraryCategory = "optics")}
          >
            Óptica
          </button>
          <button
            type="button"
            class="lib-tab"
            class:active={libraryCategory === "presets"}
            onclick={() => (libraryCategory = "presets")}
          >
            Secuencias
          </button>
        </div>

        <div class="lib-items">
          {#if libraryCategory !== "presets"}
            {#each filteredCatalog as item (item.kind)}
              <div class="lib-item">
                <div>
                  <strong>{t(item.labelKey)}</strong>
                  <p>{item.shortDesc}</p>
                </div>
                <button
                  type="button"
                  class="ghost sm"
                  title="Añadir al timeline en la posición del cursor ({playheadTime.toFixed(1)}s)"
                  onclick={() => addEffectToTimeline(item.kind)}
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
                  <p>{seq.duration}s · {seq.keyframes.length} puntos · {seq.effects.length} efectos</p>
                </div>
                <button
                  type="button"
                  class="ghost sm"
                  onclick={() => {
                    activeProject = structuredClone(seq);
                    selectedKeyframeId = activeProject.keyframes[0]?.id ?? null;
                    playheadTime = 0;
                  }}
                >
                  Cargar
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
        />

        <!-- Telemetría que diferencia Pose Recibida (VRChat), Pose Relativa (Ancla) y Pose Evaluada/Enviada -->
        <div class="telemetry-bar card">
          <div class="tele-col">
            <span class="tele-label vrc">1. Lectura real VRChat (`/usercamera/Pose`)</span>
            {#if status.camera?.poseFresh && status.camera.pose}
              <code>
                XYZ: {status.camera.pose.pos.map((v) => v.toFixed(2)).join(", ")} m | PYR:
                {status.camera.pose.rot.map((v) => v.toFixed(1)).join("°, ")}°
              </code>
            {:else}
              <code class="muted">Sin lectura reciente (mueve la cámara en VRChat)</code>
            {/if}
          </div>
          <div class="tele-col">
            <span class="tele-label base">2. Pose Base Interpolada ({playheadTime.toFixed(2)}s)</span>
            {#if evaluatedFrame}
              <code>
                XYZ: {evaluatedFrame.basePose.pos.map((v) => v.toFixed(2)).join(", ")} m | PYR:
                {evaluatedFrame.basePose.rot.map((v) => v.toFixed(1)).join("°, ")}°
              </code>
            {/if}
          </div>
          <div class="tele-col">
            <span class="tele-label final">3. Pose Final con Efectos (Enviada a OSC)</span>
            {#if evaluatedFrame}
              <code>
                XYZ: {evaluatedFrame.finalPose.pos.map((v) => v.toFixed(2)).join(", ")} m | Zoom:
                {(evaluatedFrame.zoom ?? 45).toFixed(0)} mm | Vel:
                {(evaluatedFrame.linearSpeedMps ?? 0).toFixed(2)} m/s
              </code>
            {/if}
          </div>
          <div class="tele-col">
            <span class="tele-label vrc">4. Realimentación y Calibración</span>
            {#if telemetry}
              <code>
                {#if telemetry.certainty.kind === "measured"}
                  Medido (seq #{telemetry.certainty.seq} · {telemetry.certainty.ageMs}ms)
                {:else if telemetry.certainty.kind === "estimated"}
                  Estimado ({Math.round(telemetry.certainty.confidence * 100)}% · {telemetry.certainty.reason})
                {:else}
                  Control Abierto ({telemetry.certainty.reason})
                {/if}
                {#if telemetry.posErrorM != null}
                  | Err: {(telemetry.posErrorM * 1000).toFixed(1)} mm / {(telemetry.rotErrorDeg ?? 0).toFixed(2)}°
                {/if}
              </code>
            {:else}
              <code class="muted">Inicializando diagnóstico…</code>
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
            <Crosshair size={13} /> Espacio y Objetivo
          </button>
        </div>

        {#if inspectorTab === "selection"}
          {#if selectedKeyframe}
            <div class="insp-body">
              <div class="insp-row-two">
                <label>
                  <span>Etiqueta</span>
                  <input type="text" bind:value={selectedKeyframe.label} />
                </label>
                <label>
                  <span>Tiempo (s)</span>
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
                <label>
                  <span>Curva de interpolación (Easing)</span>
                  <select
                    bind:value={selectedKeyframe.transition}
                    onchange={() => refreshMathPreview(false)}
                  >
                    <option value="linear">{t("studio.trans.linear")} — Linear</option>
                    <option value="smooth">{t("studio.trans.smooth")} — Cosine In-Out</option>
                    <option value="slowIn">{t("studio.trans.slowIn")} — Cubic Ease-In</option>
                    <option value="slowOut">{t("studio.trans.slowOut")} — Cubic Ease-Out</option>
                    <option value="smoothInOut">{t("studio.trans.smoothInOut")} — Cubic S-Curve</option>
                  </select>
                </label>

                <label title="Paso continuo C1 evita que la cámara se detenga en puntos intermedios">
                  <span>Continuidad en punto</span>
                  <select
                    value={selectedKeyframe.knotMode ?? "passThrough"}
                    onchange={(e) => {
                      selectedKeyframe.knotMode = e.currentTarget.value as "passThrough" | "stop";
                      refreshMathPreview(false);
                    }}
                  >
                    <option value="passThrough">Paso continuo (C1)</option>
                    <option value="stop">Parada suave</option>
                  </select>
                </label>
              </div>

              <h4>Posición recibida / mundo (m)</h4>
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

              <h4>Rotación Euler (° · SLERP Quaternion)</h4>
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

              {#if selectedKeyframe.localPose}
                <p class="local-readout">
                  Offset relativo al ancla: [{selectedKeyframe.localPose.pos
                    .map((v) => v.toFixed(2))
                    .join(", ")} m]
                </p>
              {/if}

              <h4>Canales ópticos en este keyframe</h4>
              <div class="insp-row-two">
                <label>
                  <span>Zoom (20–300 mm)</span>
                  <input
                    type="number"
                    min="20"
                    max="300"
                    step="1"
                    placeholder="Sin cambio"
                    value={selectedKeyframe.look.zoom ?? ""}
                    oninput={(e) => {
                      const val = e.currentTarget.value.trim();
                      selectedKeyframe.look.zoom = val === "" ? null : Number(val);
                      refreshMathPreview(liveScrub);
                    }}
                  />
                </label>
                <label>
                  <span>Exposición (-10..4 EV)</span>
                  <input
                    type="number"
                    min="-10"
                    max="4"
                    step="0.2"
                    placeholder="Sin cambio"
                    value={selectedKeyframe.look.exposure ?? ""}
                    oninput={(e) => {
                      const val = e.currentTarget.value.trim();
                      selectedKeyframe.look.exposure = val === "" ? null : Number(val);
                      refreshMathPreview(liveScrub);
                    }}
                  />
                </label>
                <label>
                  <span>Enfoque (0–10 m)</span>
                  <input
                    type="number"
                    min="0"
                    max="10"
                    step="0.1"
                    placeholder="Sin cambio"
                    value={selectedKeyframe.look.focalDistance ?? ""}
                    oninput={(e) => {
                      const val = e.currentTarget.value.trim();
                      selectedKeyframe.look.focalDistance = val === "" ? null : Number(val);
                      refreshMathPreview(liveScrub);
                    }}
                  />
                </label>
                <label>
                  <span>Apertura (f/1.4–32)</span>
                  <input
                    type="number"
                    min="1.4"
                    max="32"
                    step="0.2"
                    placeholder="Sin cambio"
                    value={selectedKeyframe.look.aperture ?? ""}
                    oninput={(e) => {
                      const val = e.currentTarget.value.trim();
                      selectedKeyframe.look.aperture = val === "" ? null : Number(val);
                      refreshMathPreview(liveScrub);
                    }}
                  />
                </label>
              </div>

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
                  <Copy size={14} /> Duplicar
                </button>
                <button type="button" class="ghost sm danger" onclick={deleteSelectedKeyframe}>
                  <Trash2 size={14} /> Eliminar
                </button>
              </div>
            </div>
          {:else if selectedEffect}
            {@const catEntry = EFFECT_CATALOG.find((c) => c.kind === selectedEffect.kind)}
            <div class="insp-body">
              <div class="insp-head-inline">
                <strong>{catEntry ? t(catEntry.labelKey) : selectedEffect.kind}</strong>
                <label class="scale-check">
                  <input
                    type="checkbox"
                    bind:checked={selectedEffect.enabled}
                    onchange={() => refreshMathPreview(false)}
                  />
                  <span>Activo</span>
                </label>
              </div>

              <div class="insp-row-two">
                <label>
                  <span>Inicio (s)</span>
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
                  <span>Duración (s)</span>
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
                  <span>Pista multipista</span>
                  <select
                    value={selectedEffect.track}
                    onchange={(e) => {
                      selectedEffect.track = Number(e.currentTarget.value);
                      refreshMathPreview(false);
                    }}
                  >
                    <option value={0}>Pista FX 1 (Principal)</option>
                    <option value={1}>Pista FX 2 (Superpuesta)</option>
                    <option value={2}>Pista FX 3 (Auxiliar)</option>
                  </select>
                </label>
                <label>
                  <span>Curva Easing</span>
                  <select
                    bind:value={selectedEffect.easing}
                    onchange={() => refreshMathPreview(false)}
                  >
                    <option value="linear">Lineal</option>
                    <option value="smooth">Suave (Seno)</option>
                    <option value="slowIn">Entrada lenta</option>
                    <option value="slowOut">Salida lenta</option>
                    <option value="smoothInOut">Entrada/Salida suaves</option>
                  </select>
                </label>
              </div>

              <label>
                <span>Intensidad ({selectedEffect.intensity.toFixed(2)}×)</span>
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
                <span>Velocidad / Frecuencia ({selectedEffect.speed.toFixed(2)}×)</span>
                <input
                  type="range"
                  min="0.2"
                  max="4.0"
                  step="0.1"
                  bind:value={selectedEffect.speed}
                  oninput={() => refreshMathPreview(false)}
                />
              </label>

              {#if catEntry?.paramALabel}
                <div class="insp-row-two">
                  <label>
                    <span>{catEntry.paramALabel}</span>
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
                  {#if catEntry?.paramBLabel}
                    <label>
                      <span>{catEntry.paramBLabel}</span>
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
                  <Copy size={14} /> Duplicar clip
                </button>
                <button type="button" class="ghost sm danger" onclick={deleteSelectedEffect}>
                  <Trash2 size={14} /> Eliminar clip
                </button>
              </div>
            </div>
          {:else}
            <p class="insp-empty">{t("studio.noSelection")}</p>
          {/if}
        {:else}
          <!-- Pestaña de Motor Cinematográfico, Calibración Técnica/Visual, Espacio y TargetProvider -->
          <div class="insp-body">
            <h4>Motor Cinematográfico</h4>
            <label>
              <span>Perfil de movimiento</span>
              <select
                value={activeProject.cinematic?.profile ?? "cinematic"}
                onchange={(e) => applyCinematicProfile(e.currentTarget.value as CinematicProfile)}
              >
                <option value="cinematic">Cinemático (suave y natural)</option>
                <option value="precision">Precisión (reproducible y exacto)</option>
                <option value="organic">Orgánico (variación sutil reproducible)</option>
              </select>
            </label>

            {#if activeProject.cinematic}
              <div class="insp-row-two">
                <label>
                  <span>Inercia τ ({activeProject.cinematic.inertiaTau.toFixed(2)} s)</span>
                  <input
                    type="range"
                    min="0"
                    max="0.6"
                    step="0.02"
                    bind:value={activeProject.cinematic.inertiaTau}
                  />
                </label>
                <label>
                  <span>Vel. máx ({activeProject.cinematic.maxLinearSpeed.toFixed(1)} m/s)</span>
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
                <span>Variación orgánica ({Math.round((activeProject.cinematic.organicVariation ?? 0) * 100)}%)</span>
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

            <h4>Calibración Técnica y Visual (No destructiva)</h4>
            {#if activeProject.calibration}
              <div class="insp-row-two">
                <label class="scale-check">
                  <input
                    type="checkbox"
                    bind:checked={activeProject.calibration.enabled}
                    onchange={() => refreshMathPreview(liveScrub)}
                  />
                  <span>Calibración activa</span>
                </label>
                <label
                  class="scale-check"
                  title="Corrige desviaciones sistemáticas solo cuando existe lectura real verificada (máx ±15 cm / ±5°)"
                >
                  <input
                    type="checkbox"
                    bind:checked={activeProject.calibration.autoCorrect}
                    disabled={!activeProject.calibration.enabled}
                  />
                  <span>Auto-corrección</span>
                </label>
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
                    <span>Offset Yaw (°)</span>
                    <input
                      type="number"
                      step="0.5"
                      bind:value={activeProject.calibration.yawOffsetDeg}
                      oninput={() => refreshMathPreview(liveScrub)}
                    />
                  </label>
                  <label>
                    <span>Offset Pitch (°)</span>
                    <input
                      type="number"
                      step="0.5"
                      bind:value={activeProject.calibration.pitchOffsetDeg}
                      oninput={() => refreshMathPreview(liveScrub)}
                    />
                  </label>
                </div>

                <label title="Encuadre visual horizontal según la focal activa (Regla de los Tercios)">
                  <span>Encuadre horizontal / Tercios ({activeProject.calibration.compositionX.toFixed(2)})</span>
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
                  <label title="Ajuste vertical de aire sobre la cabeza (Headroom)">
                    <span>Headroom Y ({activeProject.calibration.compositionY.toFixed(2)})</span>
                    <input
                      type="range"
                      min="-1"
                      max="1"
                      step="0.05"
                      bind:value={activeProject.calibration.compositionY}
                      oninput={() => refreshMathPreview(liveScrub)}
                    />
                  </label>
                  <label title="Anticipa el giro hacia la dirección de avance de la cámara">
                    <span>Lead Room ({activeProject.calibration.leadRoom.toFixed(2)})</span>
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
                <RotateCcw size={14} /> Restaurar calibración
              </button>

              {#if telemetry && telemetry.log.length > 0}
                <p class="local-readout">
                  Último ajuste: {telemetry.log[telemetry.log.length - 1].reason}
                </p>
              {/if}
            {/if}

            <hr class="sep" />

            <h4>Espacio de Referencia y Portabilidad</h4>
            <label>
              <span>Modo espacial</span>
              <select
                bind:value={activeProject.space}
                onchange={() => refreshMathPreview(false)}
              >
                <option value="anchor">{t("studio.space.anchor")}</option>
                <option value="world">{t("studio.space.world")}</option>
              </select>
            </label>

            <label>
              <span>Escala de trayectoria ({activeProject.trajectoryScale.toFixed(2)}×)</span>
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
              title="Recalcula los offsets relativos de los puntos respecto a tu ancla calibrada actual"
              onclick={bindCurrentAnchorToProject}
            >
              <Crosshair size={14} /> Re-vincular puntos al ancla actual
            </button>

            <hr class="sep" />

            <h4>Seguimiento de Objetivo (`TargetProvider`)</h4>
            <label class="scale-check">
              <input
                type="checkbox"
                checked={Boolean(activeProject.target?.enabled)}
                onchange={(e) => {
                  const enabled = e.currentTarget.checked;
                  const defaultPos = status.anchor?.eye ?? [0, 1.65, 0];
                  activeProject.target = {
                    enabled,
                    source: activeProject.target?.source ?? "staticCalibratedPoint",
                    worldPos: activeProject.target?.worldPos ?? [...defaultPos],
                    blend: activeProject.target?.blend ?? 1.0,
                    confidence: 1.0,
                    capturedAtMs: activeProject.target?.capturedAtMs ?? 0,
                  };
                  refreshMathPreview(false);
                }}
              />
              <span>Mantener objetivo encuadrado</span>
            </label>

            {#if activeProject.target}
              <label>
                <span>Fuente (`TargetSourceKind`)</span>
                <select
                  value={activeProject.target.source ?? "staticCalibratedPoint"}
                  onchange={(e) => {
                    if (!activeProject.target) return;
                    const src = e.currentTarget.value as TargetSourceKind;
                    activeProject.target.source = src;
                    if (src === "calibratedAnchor" && status.anchor) {
                      activeProject.target.worldPos = [...status.anchor.eye];
                    }
                    refreshMathPreview(false);
                  }}
                >
                  <option value="staticCalibratedPoint">Punto estático calibrado con cámara</option>
                  <option value="calibratedAnchor">Ancla calibrada del jugador</option>
                  <option value="manualReference">Coordenada manual</option>
                  <option value="companionAvatarOsc" disabled>
                    Puente OSC de Avatar Compatible (Experimental)
                  </option>
                  <option value="companionUdonBridge" disabled>
                    Asset de Mundo Udon (Experimental)
                  </option>
                </select>
              </label>

              <button
                type="button"
                class="ghost sm"
                disabled={!status.connected || !status.open}
                title="Coloca la cámara en el punto u objeto que quieres mirar y pulsa aquí"
                onclick={captureTargetFromCamera}
              >
                <Camera size={14} /> Capturar objetivo con la cámara
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
                <span>Mezcla de mirada ({Math.round(activeProject.target.blend * 100)}%)</span>
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
            <Plus size={14} /> Punto local
          </button>

          {#if playbackStatus.state === "playing"}
            <button type="button" class="ghost sm" onclick={() => api.studioControl("pause")}>
              <Pause size={14} /> {t("studio.pause")}
            </button>
            <button type="button" class="ghost sm" onclick={() => api.stopStudio()}>
              <Square size={14} /> {t("studio.stop")}
            </button>
          {:else}
            <button
              type="button"
              class="ghost sm"
              disabled={activeProject.keyframes.length === 0}
              onclick={() => startPlayback(false)}
            >
              <Eye size={14} /> {t("studio.previewPlay")}
            </button>
            <button
              type="button"
              class="primary sm"
              disabled={activeProject.keyframes.length === 0 || !status.connected || !status.open}
              onclick={() => startPlayback(true)}
            >
              <Play size={14} /> {t("studio.livePlay")}
            </button>
          {/if}

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

          <label class="scale-check" title="Envía la pose evaluada a la cámara de VRChat mientras arrastras el cursor">
            <input type="checkbox" bind:checked={liveScrub} />
            <span>Live Scrub OSC</span>
          </label>

          <button
            type="button"
            class="ghost sm"
            class:pressed={snapEnabled}
            onclick={() => (snapEnabled = !snapEnabled)}
            title="Imantar puntos y clips a la rejilla temporal"
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
          bind:this={timelineTrackEl}
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

          <!-- Pista 1: Trayectoria 3D (Posición X/Y/Z) -->
          <div class="tl-lane track-pos">
            <span class="lane-label"><Video size={12} /> {t("studio.trackPath")} (X/Y/Z)</span>
            {#each activeProject.keyframes as kf (kf.id)}
              {@const leftPct = (kf.time / Math.max(0.1, activeProject.duration)) * 100}
              <button
                type="button"
                class="kf-pill"
                class:selected={selectedKeyframeId === kf.id}
                style="left: {leftPct}%"
                onpointerdown={(e) => {
                  e.stopPropagation();
                  pushUndoSnapshot();
                  selectedKeyframeId = kf.id;
                  selectedEffectId = null;
                  inspectorTab = "selection";
                  playheadTime = kf.time;
                  dragState = { kind: "keyframe", id: kf.id };
                }}
              >
                <span class="kf-dot"></span>
                <span>{kf.label}</span>
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
            {#each activeProject.keyframes.filter((k) => k.look.zoom !== null || k.look.aperture !== null || k.look.focalDistance !== null) as kf (kf.id)}
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
                {kf.look.zoom ? `${kf.look.zoom}mm` : ""}
                {kf.look.aperture ? `f/${kf.look.aperture}` : ""}
              </button>
            {/each}
          </div>

          <!-- Pistas Multipista de Efectos Superponibles (Pistas FX 1, FX 2, FX 3) -->
          {#each [0, 1, 2] as trackIdx (trackIdx)}
            <div class="tl-lane fx-lane">
              <span class="lane-label"><Layers size={12} /> Pista FX {trackIdx + 1}</span>
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
                  onpointerdown={(e) => {
                    e.stopPropagation();
                    pushUndoSnapshot();
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
                >
                  <span class="fx-title">{catEntry ? t(catEntry.labelKey) : ef.kind}</span>
                  <span
                    class="fx-resize-handle"
                    onpointerdown={(e) => {
                      e.stopPropagation();
                      pushUndoSnapshot();
                      selectedEffectId = ef.id;
                      dragState = { kind: "effect-resize", id: ef.id };
                    }}
                  ></span>
                </div>
              {/each}
            </div>
          {/each}

          <!-- Cursor de reproducción (Playhead) -->
          <div
            class="playhead-line"
            style="left: {(playheadTime / Math.max(0.1, activeProject.duration)) * 100}%"
          ></div>
        </div>
      </div>
    </section>
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
    background: rgba(14, 10, 26, 0.7);
    border: 1px solid rgba(184, 169, 255, 0.16);
    padding: 5px 10px;
    border-radius: 10px;
    color: var(--lavender);
  }
  .project-name-input,
  input[type="text"],
  input[type="number"],
  input[type="search"],
  select {
    background: rgba(14, 10, 26, 0.75);
    color: var(--text);
    border: 1px solid rgba(184, 169, 255, 0.18);
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
    border-color: #ff708d;
    color: #ff708d;
  }

  /* Selector [ Captura | Editor ] */
  .mode-switch {
    display: inline-flex;
    background: #0e0a1a;
    padding: 4px;
    border-radius: 12px;
    border: 1px solid rgba(184, 169, 255, 0.18);
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
    background: rgba(61, 90, 254, 0.25);
    color: var(--lavender);
    font-variant-numeric: tabular-nums;
  }

  /* Panel de Configuración del Proyecto */
  .project-config-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 18px;
    border-color: rgba(61, 90, 254, 0.45);
    background: linear-gradient(180deg, rgba(42, 20, 70, 0.72), rgba(26, 18, 48, 0.92));
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
    color: var(--lavender);
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
    color: var(--lavender);
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
    background: rgba(14, 10, 26, 0.72);
    border: 1px solid rgba(184, 169, 255, 0.16);
    color: var(--text);
    font-size: 0.84rem;
    font-variant-numeric: tabular-nums;
  }
  .equiv-readout code {
    color: var(--lavender);
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
    background: var(--velvet);
    border: 1px solid rgba(184, 169, 255, 0.25);
  }
  .studio-alert.error {
    border-color: #ff708d;
  }
  .studio-alert.armed {
    border-color: #4ade80;
  }
  .studio-alert.advanced-notice {
    border-color: var(--lavender);
  }

  .guide-strip {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 12px;
    padding: 10px 16px;
    font-size: 0.82rem;
    color: var(--text-2);
    background: rgba(42, 20, 70, 0.45);
  }
  .guide-strip strong {
    color: var(--lavender);
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
    border-color: #ff708d !important;
    color: #ff708d !important;
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
    color: var(--lavender);
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
    color: var(--lavender);
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
    background: #0e0a1a;
    border: 1px solid rgba(184, 169, 255, 0.14);
    border-radius: 12px;
    padding: 8px 12px 12px;
    user-select: none;
    overflow: hidden;
  }
  .tl-ruler {
    position: relative;
    height: 22px;
    border-bottom: 1px solid rgba(184, 169, 255, 0.12);
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
    background: rgba(26, 18, 48, 0.75);
    border-radius: 8px;
    border: 1px solid rgba(184, 169, 255, 0.08);
    display: flex;
    align-items: center;
  }
  .lane-label {
    position: absolute;
    left: 8px;
    top: 4px;
    font-size: 0.68rem;
    color: rgba(184, 169, 255, 0.55);
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
    background: var(--velvet);
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
    box-shadow: 0 0 12px rgba(61, 90, 254, 0.6);
  }
  .kf-pill small {
    opacity: 0.8;
    font-size: 0.68rem;
  }
  .kf-dot {
    width: 7px;
    height: 7px;
    border-radius: 999px;
    background: var(--lavender);
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
    border: 1px solid rgba(184, 169, 255, 0.3);
    color: var(--lavender);
    z-index: 2;
  }
  .fx-block {
    position: absolute;
    top: 12px;
    height: 26px;
    border-radius: 7px;
    background: linear-gradient(90deg, rgba(61, 90, 254, 0.55), rgba(42, 20, 70, 0.85));
    border: 1px solid var(--lavender);
    display: flex;
    align-items: center;
    padding: 0 8px;
    cursor: grab;
    z-index: 2;
    overflow: hidden;
  }
  .fx-block.cat-micro {
    background: linear-gradient(90deg, rgba(140, 82, 255, 0.55), rgba(42, 20, 70, 0.85));
  }
  .fx-block.cat-optics {
    background: linear-gradient(90deg, rgba(38, 198, 218, 0.5), rgba(39, 65, 200, 0.8));
  }
  .fx-block.selected {
    border: 2px solid #ffffff;
    box-shadow: 0 0 12px rgba(184, 169, 255, 0.45);
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
    background: #ff708d;
    pointer-events: none;
    z-index: 5;
  }
  .playhead-line::before {
    content: "";
    position: absolute;
    top: 0;
    left: -5px;
    width: 12px;
    height: 12px;
    border-radius: 999px;
    background: #ff708d;
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
    background: rgba(184, 169, 255, 0.12);
    color: var(--lavender);
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
    grid-template-columns: 265px 1fr 305px;
    gap: 12px;
    align-items: stretch;
  }
  @media (max-width: 1180px) {
    .editor-grid {
      grid-template-columns: 1fr;
    }
  }
  .editor-library,
  .editor-inspector {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: 455px;
    overflow-y: auto;
  }
  .lib-search {
    display: flex;
    align-items: center;
    gap: 6px;
    background: #0e0a1a;
    border: 1px solid rgba(184, 169, 255, 0.16);
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
    background: rgba(14, 10, 26, 0.55);
    color: var(--text-2);
  }
  .lib-tab.active {
    background: var(--velvet);
    color: var(--text);
    border: 1px solid var(--accent);
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
    background: rgba(14, 10, 26, 0.55);
    border: 1px solid rgba(184, 169, 255, 0.08);
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
    color: #4ade80;
  }
  .tele-label.base {
    color: var(--lavender);
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
  }
  .insp-body h4 {
    margin: 6px 0 0;
    font-size: 0.76rem;
    color: var(--lavender);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .insp-body label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    color: var(--text-2);
  }
  .insp-row-two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .xyz-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .xyz-grid input {
    width: 100%;
  }
  .local-readout {
    margin: 0;
    font-size: 0.72rem;
    color: var(--lavender);
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
    border-top: 1px solid rgba(184, 169, 255, 0.12);
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
  .zoom-slider {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.78rem;
    color: var(--text-2);
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
</style>
