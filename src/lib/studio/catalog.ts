import type { Anchor, CalibrationConfig, CinematicConfig, EffectClip, EffectKind, Keyframe, KnotMode, SequenceProject } from "$lib/api";
import { t, pointLabel, type Key } from "$lib/i18n/index.svelte";

export type EffectCategory = "movement" | "micro" | "optics";

export type EffectCatalogEntry = {
  kind: EffectKind;
  category: EffectCategory;
  labelKey: Key;
  descKey: Key;
  defaultDuration: number;
  defaultParamA: number | null;
  defaultParamB: number | null;
  paramAKey?: Key;
  paramBKey?: Key;
};

export const EFFECT_CATALOG: EffectCatalogEntry[] = [
  // Movimientos
  {
    kind: "dollyIn",
    category: "movement",
    labelKey: "studio.effect.dollyIn",
    descKey: "effect.dollyIn.desc",
    defaultDuration: 3.0,
    defaultParamA: 0.8,
    defaultParamB: null,
    paramAKey: "effect.dollyIn.paramA",
  },
  {
    kind: "dollyOut",
    category: "movement",
    labelKey: "studio.effect.dollyOut",
    descKey: "effect.dollyOut.desc",
    defaultDuration: 3.5,
    defaultParamA: 1.2,
    defaultParamB: null,
    paramAKey: "effect.dollyOut.paramA",
  },
  {
    kind: "orbit",
    category: "movement",
    labelKey: "studio.effect.orbit",
    descKey: "effect.orbit.desc",
    defaultDuration: 4.0,
    defaultParamA: 45.0,
    defaultParamB: null,
    paramAKey: "effect.orbit.paramA",
  },
  {
    kind: "truck",
    category: "movement",
    labelKey: "studio.effect.truck",
    descKey: "effect.truck.desc",
    defaultDuration: 3.0,
    defaultParamA: 0.8,
    defaultParamB: null,
    paramAKey: "effect.truck.paramA",
  },
  {
    kind: "crane",
    category: "movement",
    labelKey: "studio.effect.crane",
    descKey: "effect.crane.desc",
    defaultDuration: 3.5,
    defaultParamA: 0.6,
    defaultParamB: null,
    paramAKey: "effect.crane.paramA",
  },
  {
    kind: "pan",
    category: "movement",
    labelKey: "studio.effect.pan",
    descKey: "effect.pan.desc",
    defaultDuration: 2.5,
    defaultParamA: 25.0,
    defaultParamB: null,
    paramAKey: "effect.pan.paramA",
  },
  {
    kind: "tilt",
    category: "movement",
    labelKey: "studio.effect.tilt",
    descKey: "effect.tilt.desc",
    defaultDuration: 2.5,
    defaultParamA: -15.0,
    defaultParamB: null,
    paramAKey: "effect.tilt.paramA",
  },
  {
    kind: "reveal",
    category: "movement",
    labelKey: "studio.effect.reveal",
    descKey: "effect.reveal.desc",
    defaultDuration: 4.0,
    defaultParamA: 1.2,
    defaultParamB: null,
    paramAKey: "effect.reveal.paramA",
  },

  // Microefectos
  {
    kind: "microShake",
    category: "micro",
    labelKey: "studio.effect.microShake",
    descKey: "effect.microShake.desc",
    defaultDuration: 2.0,
    defaultParamA: null,
    defaultParamB: null,
  },
  {
    kind: "impactShake",
    category: "micro",
    labelKey: "studio.effect.impactShake",
    descKey: "effect.impactShake.desc",
    defaultDuration: 0.8,
    defaultParamA: null,
    defaultParamB: null,
  },
  {
    kind: "handheld",
    category: "micro",
    labelKey: "studio.effect.handheld",
    descKey: "effect.handheld.desc",
    defaultDuration: 5.0,
    defaultParamA: null,
    defaultParamB: null,
  },
  {
    kind: "cameraDrift",
    category: "micro",
    labelKey: "studio.effect.cameraDrift",
    descKey: "effect.cameraDrift.desc",
    defaultDuration: 6.0,
    defaultParamA: null,
    defaultParamB: null,
  },
  {
    kind: "cameraBreathing",
    category: "micro",
    labelKey: "studio.effect.cameraBreathing",
    descKey: "effect.cameraBreathing.desc",
    defaultDuration: 6.0,
    defaultParamA: null,
    defaultParamB: null,
  },

  // Óptica
  {
    kind: "smoothZoom",
    category: "optics",
    labelKey: "studio.effect.smoothZoom",
    descKey: "effect.smoothZoom.desc",
    defaultDuration: 4.0,
    defaultParamA: 35.0,
    defaultParamB: 65.0,
    paramAKey: "effect.smoothZoom.paramA",
    paramBKey: "effect.smoothZoom.paramB",
  },
  {
    kind: "crashZoom",
    category: "optics",
    labelKey: "studio.effect.crashZoom",
    descKey: "effect.crashZoom.desc",
    defaultDuration: 0.9,
    defaultParamA: 30.0,
    defaultParamB: 95.0,
    paramAKey: "effect.crashZoom.paramA",
    paramBKey: "effect.crashZoom.paramB",
  },
  {
    kind: "focusPull",
    category: "optics",
    labelKey: "studio.effect.focusPull",
    descKey: "effect.focusPull.desc",
    defaultDuration: 3.0,
    defaultParamA: 3.0,
    defaultParamB: 1.1,
    paramAKey: "effect.focusPull.paramA",
    paramBKey: "effect.focusPull.paramB",
  },
  {
    kind: "apertureShift",
    category: "optics",
    labelKey: "studio.effect.apertureShift",
    descKey: "effect.apertureShift.desc",
    defaultDuration: 3.5,
    defaultParamA: 11.0,
    defaultParamB: 2.0,
    paramAKey: "effect.apertureShift.paramA",
    paramBKey: "effect.apertureShift.paramB",
  },
  {
    kind: "dollyZoom",
    category: "optics",
    labelKey: "studio.effect.dollyZoom",
    descKey: "effect.dollyZoom.desc",
    defaultDuration: 4.0,
    defaultParamA: 1.5,
    defaultParamB: 22.0,
    paramAKey: "effect.dollyZoom.paramA",
    paramBKey: "effect.dollyZoom.paramB",
  },
];

export function getEffectDesc(entry: EffectCatalogEntry): string {
  return t(entry.descKey);
}

export function getEffectParamALabel(entry: EffectCatalogEntry): string | undefined {
  return entry.paramAKey ? t(entry.paramAKey) : undefined;
}

export function getEffectParamBLabel(entry: EffectCatalogEntry): string | undefined {
  return entry.paramBKey ? t(entry.paramBKey) : undefined;
}

export function createEffectClip(
  kind: EffectKind,
  startTime: number,
  maxDuration: number,
  track = 0,
): EffectClip {
  const entry = EFFECT_CATALOG.find((e) => e.kind === kind) ?? EFFECT_CATALOG[0];
  const safeMax = Math.max(1.0, Number.isFinite(maxDuration) ? maxDuration : 10.0);
  const rawStart = Number.isFinite(startTime) ? Math.max(0, startTime) : 0;
  const targetDur = Math.min(entry.defaultDuration, safeMax);
  // Si el cursor está al final de la línea de tiempo, colocar el clip dentro de la secuencia para que sí se reproduzca
  const safeStart =
    rawStart + 0.5 > safeMax ? Math.max(0, safeMax - targetDur) : rawStart;
  const dur = Math.min(entry.defaultDuration, Math.max(0.5, safeMax - safeStart));
  return {
    id: `ef-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
    kind,
    track,
    startTime: Math.round(safeStart * 10) / 10,
    duration: Math.round(dur * 10) / 10,
    intensity: 1.0,
    speed: 1.0,
    paramA: entry.defaultParamA,
    paramB: entry.defaultParamB,
    easing: "smooth",
    enabled: true,
  };
}

export function defaultCinematicConfig(): CinematicConfig {
  return {
    profile: "cinematic",
    inertiaTau: 0.18,
    maxLinearSpeed: 4.5,
    maxLinearAccel: 8.0,
    maxAngularSpeed: 180.0,
    organicVariation: 0.0,
    organicSeed: 42,
  };
}

export function defaultCalibrationConfig(): CalibrationConfig {
  return {
    enabled: true,
    autoCorrect: false,
    posOffset: [0, 0, 0],
    yawOffsetDeg: 0,
    pitchOffsetDeg: 0,
    compositionX: 0,
    compositionY: 0,
    leadRoom: 0,
  };
}

export function createDefaultHeroicKeyframes(anchor: Anchor | null = null): Keyframe[] {
  const eye = anchor?.eye ?? [0, 1.65, 0];
  const yaw = anchor?.yaw ?? 0;
  const a = (yaw * Math.PI) / 180;
  const sin = Math.sin(a);
  const cos = Math.cos(a);

  const offsets: { localPos: [number, number, number]; rot: [number, number, number]; time: number; zoom: number; knotMode: KnotMode; label: string }[] = [
    { localPos: [-1.2, -0.35, 1.8], rot: [-11, 146, 0], time: 0, zoom: 38, knotMode: "stop", label: pointLabel(0) },
    { localPos: [0, 0.05, 1.35], rot: [2, 180, 0], time: 5, zoom: 54, knotMode: "passThrough", label: pointLabel(1) },
    { localPos: [1.2, 0.25, 1.9], rot: [7.5, -148, 0], time: 10, zoom: 45, knotMode: "stop", label: pointLabel(2) },
  ];

  return offsets.map((o, idx) => {
    const lx = o.localPos[0];
    const ly = o.localPos[1];
    const lz = o.localPos[2];
    const wx = eye[0] + lx * cos + lz * sin;
    const wy = eye[1] + ly;
    const wz = eye[2] - lx * sin + lz * cos;

    return {
      id: `kf-heroic-${idx}`,
      label: o.label,
      time: o.time,
      rawPose: { pos: [Math.round(wx * 100) / 100, Math.round(wy * 100) / 100, Math.round(wz * 100) / 100], rot: o.rot },
      localPose: { pos: o.localPos, rot: o.rot },
      capturedAtMs: 0,
      transition: "smooth",
      knotMode: o.knotMode,
      look: { zoom: o.zoom, exposure: null, focalDistance: null, aperture: null },
    };
  });
}

export function createDefaultSequenceProject(
  name = "Nueva Secuencia",
  anchor: Anchor | null = null,
  includeDefaultKeyframes = true,
): SequenceProject {
  return {
    schemaVersion: 2,
    id: `seq-${Date.now()}`,
    name,
    duration: 10,
    durationUnit: "seconds",
    durationInputMode: "valueUnit",
    space: "anchor",
    anchorSnapshot: anchor,
    trajectoryScale: 1,
    target: null,
    cinematic: defaultCinematicConfig(),
    calibration: defaultCalibrationConfig(),
    keyframes: includeDefaultKeyframes ? createDefaultHeroicKeyframes(anchor) : [],
    effects: includeDefaultKeyframes
      ? [
          {
            id: `ef-seed-${Date.now()}`,
            kind: "cameraBreathing",
            track: 0,
            startTime: 0,
            duration: 10,
            intensity: 0.8,
            speed: 1,
            paramA: null,
            paramB: null,
            easing: "smooth",
            enabled: true,
          },
        ]
      : [],
    rawRecording: [],
  };
}
