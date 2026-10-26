import { invoke } from "@tauri-apps/api/core";

export type Pose = {
  pos: [number, number, number];
  rot: [number, number, number];
};

export type Anchor = {
  eye: [number, number, number];
  yaw: number;
};

export type AnchorLifecycleState =
  | "inactive"
  | "acquiring"
  | "active"
  | "syncLost"
  | "recovering"
  | "released";

export type AnchorProvenance = "autoOnActivation" | "operatorCalibrated";

export type SyncLossCause = "transientModeZero" | "oscDisconnected" | "packetTimeout";

export type AnchorAuditEntry = {
  timestampMs: number;
  fromState: AnchorLifecycleState;
  toState: AnchorLifecycleState;
  mode: number;
  reason: string;
};

export type AnchorSnapshot = {
  state: AnchorLifecycleState;
  anchor: Anchor | null;
  provenance: AnchorProvenance | null;
  cameraMode: number;
  lastActiveMode: number;
  acquisitionSeq: number;
  modeSwitchCount: number;
  syncLossCount: number;
  recoveryCount: number;
  lookAtMeActive: boolean;
  syncLossCause: SyncLossCause | null;
  auditLog: AnchorAuditEntry[];
};

export type CameraSnapshot = {
  open: boolean;
  mode?: number;
  rxAvailable?: boolean;
  pose: Pose | null;
  poseFresh: boolean;
  poseSeq: number;
  poseEpochMs: number | null;
  zoom: number | null;
  exposure: number | null;
  focalDistance: number | null;
  aperture: number | null;
  lookAtMe?: boolean | null;
  smoothMovement?: boolean | null;
  streaming?: boolean | null;
};

export type Look = {
  zoom: number | null;
  exposure: number | null;
  focus: "auto" | "semi" | "manual" | null;
  focalDistance: number | null;
  aperture: number | null;
  autoLevelPitch?: boolean | null;
  autoLevelRoll?: boolean | null;
  lookAtMe?: boolean | null;
};

export type Frame = {
  distance: number;
  height: number;
  yaw: number;
  pitch: number;
  roll: number;
};

export type Preset = {
  schemaVersion?: number;
  name: string;
  category: "retrato" | "cuerpo" | "ambiente" | "movimiento" | "video" | "ref_sheet";
  modifier: boolean;
  frame: Frame | null;
  look: Look;
};

export type Motion = {
  name: string;
  duration: number;
  loop: "once" | "repeat" | "pingPong";
  handheld: boolean;
};

export type MotionStatus = { name: string | null; state: "playing" | "paused" | "stopped"; error: string | null };
export type MotionCmd = "play" | "pause" | "reverse" | { loop: boolean } | { speed: number };
export type TurnaroundProgress = { done: number; total: number; finished: boolean; error: string | null };

export type Status = {
  connected: boolean;
  open: boolean;
  anchored: boolean;
  anchor: Anchor | null;
  anchorState?: AnchorSnapshot;
  camera: CameraSnapshot | null;
};

// ---------- Iris Studio Types ----------

export type ReferenceSpace = "anchor" | "world";
export type DurationUnit = "seconds" | "minutes";
export type DurationInputMode = "valueUnit" | "mmSs";
export type Transition = "linear" | "smooth" | "slowIn" | "slowOut" | "smoothInOut";
export type KnotMode = "passThrough" | "stop";
export type CinematicProfile = "cinematic" | "precision" | "organic";

export type CinematicConfig = {
  profile: CinematicProfile;
  inertiaTau: number;
  maxLinearSpeed: number;
  maxLinearAccel: number;
  maxAngularSpeed: number;
  organicVariation: number;
  organicSeed: number;
};

export type CalibrationConfig = {
  enabled: boolean;
  autoCorrect: boolean;
  posOffset: [number, number, number];
  yawOffsetDeg: number;
  pitchOffsetDeg: number;
  compositionX: number;
  compositionY: number;
  leadRoom: number;
};

export type DataCertainty =
  | { kind: "measured"; seq: number; ageMs: number }
  | { kind: "estimated"; confidence: number; reason: string }
  | { kind: "unknown"; reason: string };

export type ControlLoopMode =
  | { mode: "closedLoop" }
  | { mode: "openLoop"; reason: string };

export type CalibrationLogEntry = {
  timestampMs: number;
  category: string;
  deltaPos: [number, number, number];
  deltaRot: [number, number, number];
  confidence: number;
  reason: string;
};

export type TelemetryReport = {
  certainty: DataCertainty;
  loopMode: ControlLoopMode;
  confidence: number;
  estimatedLatencyMs: number;
  posErrorM: number | null;
  rotErrorDeg: number | null;
  activePosCorrection: [number, number, number];
  activeRotCorrection: [number, number, number];
  visualFramingOffsetDeg: [number, number];
  log: CalibrationLogEntry[];
};

export type LookTrackPoint = {
  zoom: number | null;
  exposure: number | null;
  focalDistance: number | null;
  aperture: number | null;
};

export type Keyframe = {
  id: string;
  label: string;
  time: number;
  rawPose: Pose;
  localPose: Pose | null;
  capturedAtMs: number;
  transition: Transition;
  customCurve?: [number, number, number, number];
  knotMode?: KnotMode;
  look: LookTrackPoint;
};

export type EffectKind =
  | "dollyIn"
  | "dollyOut"
  | "orbit"
  | "truck"
  | "crane"
  | "pan"
  | "tilt"
  | "reveal"
  | "microShake"
  | "impactShake"
  | "handheld"
  | "cameraDrift"
  | "cameraBreathing"
  | "smoothZoom"
  | "crashZoom"
  | "focusPull"
  | "apertureShift"
  | "dollyZoom";

export type EffectClip = {
  id: string;
  kind: EffectKind;
  track: number;
  startTime: number;
  duration: number;
  intensity: number;
  speed: number;
  paramA: number | null;
  paramB: number | null;
  easing: Transition;
  customCurve?: [number, number, number, number];
  enabled: boolean;
};

export type TargetSourceKind =
  | "staticCalibratedPoint"
  | "calibratedAnchor"
  | "manualReference"
  | "companionAvatarOsc"
  | "companionUdonBridge";

export type TargetConfig = {
  enabled: boolean;
  source?: TargetSourceKind;
  worldPos: [number, number, number];
  blend: number;
  confidence?: number;
  capturedAtMs?: number;
};

export type RecordedSample = {
  time: number;
  pose: Pose;
  zoom: number | null;
};

export type SequenceProject = {
  schemaVersion: number;
  id: string;
  name: string;
  duration: number;
  durationUnit?: DurationUnit;
  durationInputMode?: DurationInputMode;
  space: ReferenceSpace;
  anchorSnapshot: Anchor | null;
  trajectoryScale: number;
  target: TargetConfig | null;
  cinematic?: CinematicConfig;
  calibration?: CalibrationConfig;
  keyframes: Keyframe[];
  effects: EffectClip[];
  rawRecording: RecordedSample[];
};

export type EvaluatedFrame = {
  time: number;
  basePose: Pose;
  finalPose: Pose;
  zoom: number | null;
  exposure: number | null;
  focalDistance: number | null;
  aperture: number | null;
  linearSpeedMps?: number;
  angularSpeedDps?: number;
};

export type SpatialNode = {
  id: string;
  label: string;
  time: number;
  worldPos: [number, number, number];
  worldRot: [number, number, number];
  forward: [number, number, number];
};

export type TrajectoryPreview = {
  nodes: SpatialNode[];
  basePath: [number, number, number][];
  finalPath: [number, number, number][];
  segmentDistances: number[];
  totalDistance: number;
  anchorPos: [number, number, number] | null;
  anchorYaw: number | null;
  targetPos: [number, number, number] | null;
};

export type StudioPlaybackStatus = {
  state: "playing" | "paused" | "stopped";
  currentTime: number;
  duration: number;
  looping: boolean;
  speed: number;
  liveOsc: boolean;
  frame: EvaluatedFrame | null;
  error: string | null;
};

export type StudioControlCmd =
  | "play"
  | "pause"
  | { seek: number }
  | { loop: boolean }
  | { speed: number };

export type RecordingStatus = {
  active: boolean;
  elapsed: number;
  sampleCount: number;
};

export type FinishedRecording = {
  duration: number;
  rawSamples: RecordedSample[];
  keyframes: Keyframe[];
};

export type VisualizerSnapshot = {
  enabled: boolean;
  anchorPosition: [number, number, number] | null;
  anchorYaw: number | null;
  hideCam: boolean;
  lastCamPose: Pose | null;
  lastCamNormalizedPos: [number, number, number] | null;
  lastCamNormalizedRot: [number, number, number] | null;
  lastWpSamples: [number, number, number][] | null;
  isSyncingPath: boolean;
  lastError: string | null;
  nudgePos: [number, number, number];
  nudgeYaw: number;
  diag: VizDiag;
};

export type VizDiag = {
  poseEchoes: number;
  framesSent: number;
  skipped: { disabled: number; noAnchor: number; externalControl: number; deadband: number; busy: number };
  lastError: string | null;
  avatarEcho: { name: string; value: number; ageMs: number }[];
};

export const api = {
  connect: () => invoke<void>("connect"),
  status: () => invoke<Status>("status"),
  calibrate: () => invoke<void>("calibrate"),
  resetDefaults: () => invoke<void>("reset_defaults"),
  setCameraToggle: (name: string, enabled: boolean) => invoke<void>("set_camera_toggle", { name, enabled }),
  listPresets: () => invoke<{ presets: Preset[]; warnings: string[] }>("list_presets"),
  applyPreset: (name: string) => invoke<void>("apply_preset", { name }),
  saveUserPreset: (preset: Preset) => invoke<void>("save_user_preset", { preset }),
  importPresetJson: (raw: string) => invoke<Preset>("import_preset_json", { raw }),
  listMotions: () => invoke<Motion[]>("list_motions"),
  playMotion: (name: string) => invoke<void>("play_motion", { name }),
  motionControl: (cmd: MotionCmd) => invoke<void>("motion_control", { cmd }),
  motionStatus: () => invoke<MotionStatus>("motion_status"),
  stopMotion: () => invoke<void>("stop_motion"),
  panicStop: () => invoke<void>("panic_stop"),
  startTurnaround: (fourViews: boolean) => invoke<void>("start_turnaround", { fourViews }),
  turnaroundStatus: () => invoke<TurnaroundProgress | null>("turnaround_status"),

  // Iris Studio
  captureCameraPoint: (index: number, time: number, captureOptics: boolean) =>
    invoke<Keyframe>("capture_camera_point", { index, time, captureOptics }),
  captureTargetPoint: (blend: number) =>
    invoke<TargetConfig>("capture_target_point", { blend }),
  evaluateSequence: (project: SequenceProject, time: number, sendToCamera: boolean) =>
    invoke<EvaluatedFrame>("evaluate_sequence", { project, time, sendToCamera }),
  previewTrajectory: (project: SequenceProject, steps: number) =>
    invoke<TrajectoryPreview>("preview_trajectory", { project, steps }),
  playSequence: (project: SequenceProject, startTime: number, looping: boolean, liveOsc: boolean) =>
    invoke<void>("play_sequence", { project, startTime, looping, liveOsc }),
  studioControl: (cmd: StudioControlCmd) => invoke<void>("studio_control", { cmd }),
  studioStatus: () => invoke<StudioPlaybackStatus>("studio_status"),
  stopStudio: () => invoke<void>("stop_studio"),

  // Continuous Recording
  startRecording: () => invoke<void>("start_recording"),
  recordingStatus: () => invoke<RecordingStatus>("recording_status"),
  stopRecording: (posTolM: number, rotTolDeg: number, smoothingPasses: number) =>
    invoke<FinishedRecording>("stop_recording", { posTolM, rotTolDeg, smoothingPasses }),
  resimplifyRecording: (
    rawSamples: RecordedSample[],
    posTolM: number,
    rotTolDeg: number,
    smoothingPasses: number,
  ) => invoke<Keyframe[]>("resimplify_recording", { rawSamples, posTolM, rotTolDeg, smoothingPasses }),

  // Sequence Library
  listSequences: () => invoke<{ sequences: SequenceProject[]; warnings: string[] }>("list_sequences"),
  saveSequence: (project: SequenceProject) => invoke<SequenceProject>("save_sequence_cmd", { project }),
  deleteSequence: (id: string) => invoke<void>("delete_sequence_cmd", { id }),
  importSequence: (raw: string) => invoke<SequenceProject>("import_sequence_cmd", { raw }),

  // Cinematic Profiles, Calibration Telemetry & VRChat Camera Dolly Adapter
  telemetryStatus: (project: SequenceProject) => invoke<TelemetryReport>("telemetry_status", { project }),
  resetCalibration: () => invoke<TelemetryReport>("reset_calibration_cmd"),
  loadMotionAsSequence: (name: string) => invoke<SequenceProject>("load_motion_as_sequence", { name }),
  importVrcDolly: (raw: string) => invoke<SequenceProject>("import_vrc_dolly_cmd", { raw }),
  exportVrcDolly: (project: SequenceProject, bakeEffects: boolean) =>
    invoke<string>("export_vrc_dolly_cmd", { project, bakeEffects }),

  // Visualizador OSC / Rig de Avatar (Avatars 3.0 / Unity)
  visualizerStatus: () => invoke<VisualizerSnapshot>("visualizer_status"),
  visualizerSetAnchor: (anchorPos?: [number, number, number]) =>
    invoke<[number, number, number]>("visualizer_set_anchor", { anchorPos }),
  visualizerActivate: () => invoke<void>("visualizer_activate"),
  visualizerSetHideCam: (hidden: boolean) => invoke<void>("visualizer_set_hide_cam", { hidden }),
  visualizerResetAnchor: () => invoke<void>("visualizer_reset_anchor"),
  visualizerNudge: (dx: number, dy: number, dz: number, dyaw: number) =>
    invoke<void>("visualizer_nudge", { dx, dy, dz, dyaw }),
  visualizerResetNudge: () => invoke<void>("visualizer_reset_nudge"),
  visualizerSyncPath: (project: SequenceProject) =>
    invoke<void>("visualizer_sync_path", { project }),
  visualizerSyncCamera: (pose: Pose) =>
    invoke<void>("visualizer_sync_camera", { pose }),
  avatarStatus: () => invoke<VisualizerSnapshot>("visualizer_status"),
  fitKeyframesToDuration: (project: SequenceProject, targetDuration: number) =>
    invoke<SequenceProject>("fit_keyframes_to_duration", { project, targetDuration }),
  sampleBasePoseAt: (project: SequenceProject, time: number) =>
    invoke<Pose>("sample_base_pose_at", { project, time }),
};
