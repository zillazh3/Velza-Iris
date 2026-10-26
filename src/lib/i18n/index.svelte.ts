import es from "./es-MX.json";
import en from "./en.json";

export type Dict = typeof es;
export type Key = keyof typeof es | keyof typeof en;
const dicts: Record<string, Record<string, string>> = { "es-MX": es, en };
export type Locale = "es-MX" | "en";

let locale = $state<Locale>("es-MX");
let initialized = false;

function applyHtmlLang(loc: Locale) {
  if (typeof document !== "undefined") {
    document.documentElement.lang = loc;
  }
}

export const i18n = {
  get locale() {
    return locale;
  },
  setLocale(newLocale: Locale) {
    if (newLocale !== "es-MX" && newLocale !== "en") return;
    locale = newLocale;
    if (typeof localStorage !== "undefined") {
      try {
        localStorage.setItem("velza-locale", newLocale);
      } catch {
        /* no-op */
      }
    }
    applyHtmlLang(newLocale);
  },
  toggle() {
    const next: Locale = locale === "es-MX" ? "en" : "es-MX";
    this.setLocale(next);
  },
  init() {
    if (initialized) return;
    initialized = true;
    if (typeof localStorage !== "undefined") {
      try {
        const stored = localStorage.getItem("velza-locale") as Locale | null;
        if (stored === "es-MX" || stored === "en") {
          locale = stored;
        }
      } catch {
        /* no-op */
      }
    }
    applyHtmlLang(locale);
  },
};

export function t(key: Key | string, params: Record<string, string | number> = {}): string {
  const currentDict = dicts[locale];
  const fallbackDict = dicts[locale === "es-MX" ? "en" : "es-MX"];

  const raw = currentDict?.[key] ?? fallbackDict?.[key] ?? key;
  return Object.entries(params).reduce(
    (s, [k, v]) => s.replaceAll(`{${k}}`, String(v)),
    raw
  );
}

/**
 * Retorna el nombre de un punto de keyframe según el idioma activo ("Punto A", "Point A", etc.)
 */
export function pointLabel(idx: number): string {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
  const letter = alphabet[idx % 26];
  const suffix = idx < 26 ? letter : `${letter}${Math.floor(idx / 26) + 1}`;
  return locale === "en" ? `Point ${suffix}` : `Punto ${suffix}`;
}

/**
 * Retorna el nombre formateado de un keyframe adaptado dinámicamente al idioma actual.
 * Si el proyecto guardó "Punto A", en inglés se muestra "Point A".
 * Si guardó "Point A", en español se muestra "Punto A".
 */
export function formatKeyframeLabel(label: string | null | undefined): string {
  if (!label) return "";
  if (locale === "en") {
    return label.replace(/^Punto\s+([A-Z0-9]+)/i, "Point $1");
  } else {
    return label.replace(/^Point\s+([A-Z0-9]+)/i, "Punto $1");
  }
}

type Rule = { re: RegExp; key: string; params?: (m: RegExpMatchArray) => Record<string, string> };

/**
 * Errores conocidos del backend (Rust) en español → llave `err.*`. Específicos antes que genéricos.
 */
const ERRORS: Rule[] = [
  { re: /sin conexión con VRChat|No hay conexión activa con VRChat/i, key: "err.notConnected" },
  { re: /Se perdió la conexión con VRChat/, key: "err.connectionLost" },
  { re: /VRChat no apareció por OSCQuery/, key: "err.oscQueryNotFound" },
  { re: /cámara cerrada: ábrela en VRChat|cámara de VRChat está cerrada/, key: "err.cameraClosed" },
  { re: /Puerto de escucha UDP 9001|Puerto UDP 9001 no disponible/, key: "err.rxUnavailable" },
  { re: /sin ancla: calibra primero/, key: "err.noAnchor" },
  {
    re: /aún no reporta la pose|Aún no recibimos la posición actual|aún no reporta una posición reciente|No pudimos leer la posición actual/,
    key: "err.poseStale",
  },
  { re: /Mueve ligeramente tu cámara en VRChat sobre el objetivo/, key: "err.targetStale" },
  { re: /Agrega al menos un punto en la línea de tiempo/, key: "err.needPointToPlay" },
  { re: /Abre la cámara en VRChat para reproducir en vivo/, key: "err.cameraClosedPlay" },
  { re: /Abre la cámara en VRChat antes de iniciar la grabación/, key: "err.cameraClosedRecord" },
  { re: /No se recibieron movimientos de la cámara durante la grabación/, key: "err.noRecordedMovements" },
  { re: /Detén el movimiento o secuencia/, key: "err.externalControlCalibrate" },
  { re: /corresponde a (?:un movimiento comandado|un encuadre aplicado)/, key: "err.commandedEchoCalibrate" },
  { re: /Conecta con VRChat antes de reproducir/, key: "err.connectToPlay" },
  { re: /Conecta con VRChat antes de grabar/, key: "err.connectToRecord" },
  { re: /^cámara ocupada/, key: "err.cameraBusy" },
  { re: /^cámara cerrada/, key: "err.cameraClosed" },
  { re: /No hay secuencia reproduciéndose/, key: "err.noSequencePlaying" },
  { re: /no hay movimiento en curso/, key: "err.noMotionRunning" },
  { re: /No hay grabación activa/, key: "err.noRecording" },
  { re: /el preset no existe/, key: "err.presetNotFound" },
  { re: /el movimiento (?:predefinido solicitado )?no existe/i, key: "err.motionNotFound" },
  { re: /El rig está apagado/, key: "err.rigOff" },
  { re: /^Error en el bus de red: (.*)$/, key: "err.netBus", params: (m) => ({ detail: m[1] }) },
  { re: /valores no finitos|Ancla no finita/, key: "err.nonFinite" },
  { re: /El nombre de la secuencia no puede estar vacío/, key: "err.sequenceNameEmpty" },
  { re: /^name vacío/, key: "err.nameEmpty" },
  { re: /La secuencia no tiene puntos para exportar/, key: "err.nothingToExport" },
  {
    re: /^Demasiados fotogramas clave \((\d+), máximo (\d+)\)/,
    key: "err.tooManyKeyframes",
    params: (m) => ({ count: m[1], max: m[2] }),
  },
  {
    re: /^Demasiados efectos \((\d+), máximo (\d+)\)/,
    key: "err.tooManyEffects",
    params: (m) => ({ count: m[1], max: m[2] }),
  },
  {
    re: /^(.+): tiempo (\S+) fuera de la duración/,
    key: "err.keyframeOutOfRange",
    params: (m) => ({ label: m[1], time: m[2] }),
  },
  {
    re: /^Efecto (.+): inicio (\S+) fuera de la duración/,
    key: "err.effectStartOutOfRange",
    params: (m) => ({ id: m[1], start: m[2] }),
  },
  {
    re: /^Efecto (.+): duración inválida \((.*)\)/,
    key: "err.effectInvalidDuration",
    params: (m) => ({ id: m[1], duration: m[2] }),
  },
  { re: /^schemaVersion (\S+) no soportado/, key: "err.unsupportedSchema", params: (m) => ({ version: m[1] }) },
  { re: /^JSON inválido: (.*)$/s, key: "err.invalidJson", params: (m) => ({ detail: m[1] }) },
  { re: /El archivo Camera Dolly no contiene puntos/, key: "err.dollyEmpty" },
  { re: /No se encontró la lista 'Points'/, key: "err.dollyNoPoints" },
  {
    re: /^Estructura de puntos Camera Dolly inválida: (.*)$/s,
    key: "err.dollyInvalid",
    params: (m) => ({ detail: m[1] }),
  },
  { re: /Rotation\[\d\] inválido|Falta Rotation\.[XYZ]|Formato de Rotation en Camera Dolly/, key: "err.dollyRotation" },
  {
    re: /^(.+) no es escribible \((.*)\); puedes elegir/s,
    key: "err.dirNotWritable",
    params: (m) => ({ dir: m[1], detail: m[2] }),
  },
  { re: /^(.+): zoom (\S+) fuera de rango/, key: "err.zoomOutOfRange", params: (m) => ({ name: m[1], zoom: m[2] }) },
  { re: /^(.+): duración fuera de rango/, key: "err.durationOutOfRange", params: (m) => ({ name: m[1] }) },
  { re: /^(.+): distancia debe ser > 0/, key: "err.distancePositive", params: (m) => ({ name: m[1] }) },
  { re: /^(.+) fuera de rango: (.+)$/, key: "err.outOfRange", params: (m) => ({ field: m[1], value: m[2] }) },
];

function localizeLine(line: string): string {
  const trimmed = line.trim();
  if (trimmed.startsWith("err.")) return t(trimmed);
  // Las advertencias de carga vienen como "archivo.json: mensaje": el nombre se conserva tal cual.
  const file = trimmed.match(/^\S+\.json: /)?.[0] ?? "";
  const s = trimmed.slice(file.length);
  for (const { re, key, params } of ERRORS) {
    const m = s.match(re);
    if (m) return file + t(key, params ? params(m) : {});
  }
  return line;
}

/**
 * Localiza mensajes de error técnicos retornados por el backend Tauri u operaciones de herramientas.
 * Las advertencias de carga llegan unidas por saltos de línea: se traduce cada línea.
 */
export function localizeError(err: unknown): string {
  if (!err) return "";
  const raw = typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
  return raw.split("\n").map(localizeLine).join("\n").trim();
}

/**
 * Localiza los motivos (`reason`) de telemetría/calibración que genera el backend en español.
 * Las frases con valores se parsean con regex; lo desconocido se muestra tal cual.
 */
const REASONS: { re: RegExp; key: string; params?: (m: RegExpMatchArray) => Record<string, string> }[] = [
  { re: /^Esperando conexión y apertura de cámara/, key: "reason.waitingConnection" },
  { re: /^Sin lecturas recientes de \/usercamera\/Pose/, key: "reason.noRecentPose" },
  { re: /^Ancla en transición/, key: "reason.anchorTransition" },
  { re: /^Corrección adaptativa en pausa/, key: "reason.correctionPaused" },
  { re: /^Sin conexión activa con VRChat/, key: "reason.noConnection" },
  { re: /^Desconectado de VRChat/, key: "reason.disconnectedPreview" },
  { re: /^Puerto UDP 9001 ocupado/, key: "reason.portBusy" },
  { re: /^Modo Control Abierto/, key: "reason.openLoopMode" },
  { re: /^Cámara cerrada en VRChat/, key: "reason.cameraClosed" },
  { re: /^Cámara cerrada: VRChat no procesa/, key: "reason.cameraClosedNoPose" },
  { re: /^Cámara abierta, pero VRChat aún no/, key: "reason.noFirstPose" },
  { re: /^Sin lectura inicial de Pose/, key: "reason.noInitialPose" },
  { re: /^Cámara estacionaria \(último eco hace ([\d.]+) s\)/, key: "reason.stationary", params: (m) => ({ s: m[1] }) },
  { re: /^En espera de trayectoria activa/, key: "reason.waitingTrajectory" },
  { re: /^Discrepancia externa detectada/, key: "reason.externalDiscrepancy" },
  { re: /^Corrección congelada por discrepancia/, key: "reason.correctionFrozen" },
  { re: /^Discrepancia alta \(([\d.]+) m\)/, key: "reason.highDiscrepancy", params: (m) => ({ m: m[1] }) },
  {
    re: /^Ajuste suave por residuo medido \(([\d.]+) mm, ([\d.]+)°\)/,
    key: "reason.softAdjust",
    params: (m) => ({ mm: m[1], deg: m[2] }),
  },
  { re: /^Calibración desactivada por el usuario/, key: "reason.calibrationOff" },
  { re: /^Offsets técnicos\/visuales activos/, key: "reason.offsetsActive" },
  { re: /^Restaur(?:ación|ado) manual/, key: "reason.manualReset" },
];

export function localizeReason(raw: string | null | undefined): string {
  if (!raw) return "";
  for (const { re, key, params } of REASONS) {
    const m = raw.match(re);
    if (m) return t(key, params ? params(m) : {});
  }
  return raw;
}

/**
 * Traduce nombres de presets integrados (seeds.json) manteniendo el identificador interno si es personalizado.
 */
export function localizePresetName(name: string): string {
  const key = `preset.name.${slugify(name)}`;
  const translated = t(key);
  return translated !== key ? translated : name;
}

/**
 * Traduce nombres de movimientos integrados (motions.json) manteniendo el identificador si es personalizado.
 */
export function localizeMotionName(name: string): string {
  const key = `motion.name.${slugify(name)}`;
  const translated = t(key);
  return translated !== key ? translated : name;
}

function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/¾/g, "3-4")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}
