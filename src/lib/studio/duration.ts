import type {
  DurationInputMode,
  DurationUnit,
  EffectClip,
  Keyframe,
  SequenceProject,
} from "$lib/api";

export const DEFAULT_DURATION_SEC = 10;
export const MIN_DURATION_SEC = 0.5;
export const MAX_DURATION_SEC = 3600; // 60 minutos

export type DurationValidationErrorCode = "invalidNumber" | "secondsRange" | "range";

export type DurationParseResult =
  | { ok: true; seconds: number }
  | { ok: false; code: DurationValidationErrorCode };

export type DurationChangeResolution = "preserve" | "clamp" | "scale";

export type OutOfRangeConflict = {
  keyframes: Keyframe[];
  effects: EffectClip[];
  hasConflicts: boolean;
};

export type TimelineTick = {
  time: number;
  leftPct: number;
  label: string;
};

/**
 * Redondea artefactos de punto flotante IEEE-754 sin truncar decimales significativos.
 */
export function cleanFloat(val: number, maxDecimals = 6): number {
  if (!Number.isFinite(val)) return NaN;
  const factor = 10 ** maxDecimals;
  return Math.round((val + Number.EPSILON) * factor) / factor;
}

/**
 * Convierte segundos internos al valor numérico mostrado en la unidad seleccionada.
 * Ejemplo: 60 s -> 1 min; 90 s -> 1.5 min; 47.5 s -> 47.5 s.
 */
export function secondsToUnitValue(seconds: number, unit: DurationUnit): number {
  if (!Number.isFinite(seconds)) return DEFAULT_DURATION_SEC;
  if (unit === "minutes") {
    return cleanFloat(seconds / 60, 6);
  }
  return cleanFloat(seconds, 4);
}

/**
 * Convierte un valor numérico en la unidad dada a la representación interna única en segundos.
 * Ejemplo: 1 min -> 60 s; 1.5 min -> 90 s; 47.5 s -> 47.5 s.
 */
export function unitValueToSeconds(value: number, unit: DurationUnit): number {
  if (!Number.isFinite(value)) return NaN;
  if (unit === "minutes") {
    return cleanFloat(value * 60, 4);
  }
  return cleanFloat(value, 4);
}

/**
 * Valida que una duración en segundos sea finita, mayor que cero y dentro de los límites técnicos reales.
 */
export function validateDurationSeconds(seconds: number): DurationParseResult {
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return { ok: false, code: "invalidNumber" };
  }
  if (seconds < MIN_DURATION_SEC || seconds > MAX_DURATION_SEC) {
    return { ok: false, code: "range" };
  }
  return { ok: true, seconds: cleanFloat(seconds, 4) };
}

/**
 * Evalúa una entrada en modalidad "Valor y unidad" (segundos o minutos, admitiendo decimales).
 */
export function parseValueUnitInput(
  rawValue: string | number,
  unit: DurationUnit,
): DurationParseResult {
  const str = typeof rawValue === "number" ? String(rawValue) : rawValue.trim();
  if (!str) {
    return { ok: false, code: "invalidNumber" };
  }
  const num = Number(str);
  if (!Number.isFinite(num) || num <= 0) {
    return { ok: false, code: "invalidNumber" };
  }
  const seconds = unitValueToSeconds(num, unit);
  return validateDurationSeconds(seconds);
}

/**
 * Evalúa una entrada en modalidad "minutos:segundos" (`MM:SS` o `MM:SS.d`).
 * Valida estrictamente que los segundos estén entre 0 y 59 (menores a 60) y los minutos sean >= 0.
 */
export function parseMmSsInput(raw: string): DurationParseResult {
  const trimmed = raw.trim();
  const match = /^(\d+):(\d{1,2}(?:\.\d+)?)$/.exec(trimmed);
  if (!match) {
    return { ok: false, code: "invalidNumber" };
  }
  const mins = Number(match[1]);
  const secs = Number(match[2]);
  if (!Number.isFinite(mins) || !Number.isFinite(secs) || mins < 0 || secs < 0) {
    return { ok: false, code: "invalidNumber" };
  }
  if (secs >= 60) {
    return { ok: false, code: "secondsRange" };
  }
  const totalSeconds = cleanFloat(mins * 60 + secs, 4);
  return validateDurationSeconds(totalSeconds);
}

/**
 * Formatea una duración en segundos como `MM:SS` (o `MM:SS.d` si incluye fracción decimal).
 * Ejemplos: 83 -> "01:23", 330 -> "05:30", 47.5 -> "00:47.5", 10 -> "00:10".
 */
export function formatSecondsToMmSs(seconds: number): string {
  const safe = Number.isFinite(seconds) && seconds >= 0 ? cleanFloat(seconds, 2) : 0;
  let mins = Math.floor(safe / 60);
  let rem = cleanFloat(safe - mins * 60, 2);
  if (rem >= 60) {
    mins += 1;
    rem = 0;
  }
  const minPart = String(mins).padStart(2, "0");
  const isInt = Math.abs(rem - Math.round(rem)) < 1e-4;
  if (isInt) {
    return `${minPart}:${String(Math.round(rem)).padStart(2, "0")}`;
  }
  const intSec = Math.floor(rem);
  const decPart = String(cleanFloat(rem - intSec, 2)).slice(2);
  return `${minPart}:${String(intSec).padStart(2, "0")}.${decPart}`;
}

/**
 * Formatea un resumen legible de equivalencia entre segundos, minutos y MM:SS.
 */
export function formatDurationEquivalence(
  seconds: number,
  unit: DurationUnit,
): {
  secondsText: string;
  minutesText: string;
  mmSsText: string;
  otherUnitText: string;
} {
  const secVal = cleanFloat(seconds, 2);
  const minVal = cleanFloat(seconds / 60, 4);
  const secondsText = `${secVal} s`;
  const minutesText = `${minVal} min`;
  const mmSsText = formatSecondsToMmSs(seconds);
  const otherUnitText = unit === "seconds" ? `${minutesText} (${mmSsText})` : `${secondsText} (${mmSsText})`;
  return {
    secondsText,
    minutesText,
    mmSsText,
    otherUnitText,
  };
}

/**
 * Formatea un instante o duración para cabeceras y tarjetas (ej. "10.0s" o "01:30 (90.0s)").
 */
export function formatTimeReadout(seconds: number, totalDurationSec: number): string {
  if (totalDurationSec >= 60) {
    return `${formatSecondsToMmSs(seconds)} (${seconds.toFixed(1)}s)`;
  }
  return `${seconds.toFixed(1)}s`;
}

/**
 * Detecta keyframes y clips de movimiento/efecto que quedarían fuera del nuevo límite temporal.
 */
export function detectOutOfRangeElements(
  project: Pick<SequenceProject, "keyframes" | "effects">,
  newDurationSec: number,
): OutOfRangeConflict {
  const eps = 1e-3;
  const keyframes = project.keyframes.filter((k) => k.time > newDurationSec + eps);
  const effects = project.effects.filter(
    (e) => e.startTime > newDurationSec + eps || e.startTime + e.duration > newDurationSec + eps,
  );
  return {
    keyframes,
    effects,
    hasConflicts: keyframes.length > 0 || effects.length > 0,
  };
}

/**
 * Aplica un cambio de duración sobre un `SequenceProject` respetando la estrategia elegida.
 * - `preserve`: mantiene intactos todos los tiempos de keyframes y duraciones de clips.
 * - `clamp`: ajusta al borde únicamente los elementos que excedan `newDurationSec`.
 * - `scale`: reescala proporcionalmente los tiempos de keyframes y clips.
 */
export function applyProjectDurationChange(
  project: SequenceProject,
  newDurationSec: number,
  unit: DurationUnit,
  inputMode: DurationInputMode,
  resolution: DurationChangeResolution,
): SequenceProject {
  const validDur = cleanFloat(
    Math.max(MIN_DURATION_SEC, Math.min(MAX_DURATION_SEC, newDurationSec)),
    4,
  );
  const oldDur = Math.max(0.1, project.duration);
  const ratio = validDur / oldDur;

  let keyframes = project.keyframes.map((k) => ({ ...k }));
  let effects = project.effects.map((e) => ({ ...e }));

  if (resolution === "clamp") {
    keyframes = keyframes.map((k) => ({
      ...k,
      time: k.time > validDur ? validDur : k.time,
    }));
    effects = effects.map((e) => {
      let startTime = e.startTime;
      let duration = e.duration;
      if (startTime >= validDur) {
        startTime = cleanFloat(Math.max(0, validDur - Math.min(duration, validDur)), 2);
      }
      if (startTime + duration > validDur) {
        duration = cleanFloat(Math.max(0.1, validDur - startTime), 2);
      }
      return {
        ...e,
        startTime,
        duration,
      };
    });
  } else if (resolution === "scale" && Math.abs(validDur - oldDur) > 1e-4) {
    keyframes = keyframes.map((k) => ({
      ...k,
      time: cleanFloat(Math.min(validDur, k.time * ratio), 2),
    }));
    effects = effects.map((e) => {
      const startTime = cleanFloat(Math.min(validDur, e.startTime * ratio), 2);
      const maxRemaining = Math.max(0.1, validDur - startTime);
      const duration = cleanFloat(Math.max(0.1, Math.min(maxRemaining, e.duration * ratio)), 2);
      return {
        ...e,
        startTime,
        duration,
      };
    });
  }

  keyframes.sort((a, b) => a.time - b.time);

  return {
    ...project,
    duration: validDur,
    durationUnit: unit,
    durationInputMode: inputMode,
    keyframes,
    effects,
  };
}

/**
 * Genera marcas adaptativas y legibles para la regla del timeline tanto en duraciones cortas (5s)
 * como en duraciones largas (30s, 1 min, 5 min, 60 min).
 */
export function generateTimelineTicks(durationSec: number, zoom = 1): TimelineTick[] {
  const dur = Math.max(MIN_DURATION_SEC, Number.isFinite(durationSec) ? durationSec : DEFAULT_DURATION_SEC);
  const targetTickCount = Math.max(5, Math.min(24, Math.round(7 * Math.max(1, zoom))));
  const rawStep = dur / targetTickCount;

  const candidateSteps = [
    0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600,
  ];
  const step = candidateSteps.find((s) => s >= rawStep) ?? 600;

  const formatTick = (t: number): string => {
    if (dur >= 60) {
      return formatSecondsToMmSs(t);
    }
    if (step < 1) {
      return `${cleanFloat(t, 2)}s`;
    }
    return `${cleanFloat(t, 1)}s`;
  };

  const ticks: TimelineTick[] = [];
  for (let t = 0; t < dur - step * 0.35; t = cleanFloat(t + step, 4)) {
    ticks.push({
      time: t,
      leftPct: cleanFloat((t / dur) * 100, 4),
      label: formatTick(t),
    });
  }

  ticks.push({
    time: dur,
    leftPct: 100,
    label: formatTick(dur),
  });

  return ticks;
}

/**
 * Calcula el paso de imantado (snap) adaptativo según la duración y el zoom del timeline.
 */
export function adaptiveSnapStep(durationSec: number, zoom = 1): number {
  const effectiveSpan = durationSec / Math.max(1, zoom);
  if (effectiveSpan <= 8) return 0.1;
  if (effectiveSpan <= 20) return 0.25;
  if (effectiveSpan <= 60) return 0.5;
  if (effectiveSpan <= 180) return 1.0;
  return 2.0;
}
