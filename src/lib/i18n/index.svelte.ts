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

/**
 * Localiza mensajes de error técnicos retornados por el backend Tauri u operaciones de herramientas.
 */
export function localizeError(err: unknown): string {
  if (!err) return "";
  const raw = typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
  const trimmed = raw.trim();

  // Errores conocidos del backend
  if (trimmed.includes("sin conexión con VRChat")) return t("err.notConnected");
  if (trimmed.includes("cámara cerrada: ábrela en VRChat") || trimmed.includes("cámara de VRChat está cerrada")) {
    return t("err.cameraClosed");
  }
  if (trimmed.includes("Puerto de escucha UDP 9001") || trimmed.includes("Puerto UDP 9001 no disponible")) {
    return t("err.rxUnavailable");
  }
  if (trimmed.includes("sin ancla: calibra primero")) return t("err.noAnchor");
  if (
    trimmed.includes("aún no reporta la pose") ||
    trimmed.includes("Aún no recibimos la posición actual") ||
    trimmed.includes("aún no reporta una posición reciente")
  ) {
    return t("err.poseStale");
  }
  if (trimmed.includes("Mueve ligeramente tu cámara en VRChat sobre el objetivo")) {
    return t("err.targetStale");
  }
  if (trimmed.includes("Agrega al menos un punto en la línea de tiempo")) {
    return t("err.needPointToPlay");
  }
  if (trimmed.includes("Abre la cámara en VRChat para reproducir en vivo")) {
    return t("err.cameraClosedPlay");
  }
  if (trimmed.includes("Abre la cámara en VRChat antes de iniciar la grabación")) {
    return t("err.cameraClosedRecord");
  }
  if (trimmed.includes("No se recibieron movimientos de la cámara durante la grabación")) {
    return t("err.noRecordedMovements");
  }
  if (trimmed.includes("Detén el movimiento o secuencia en curso")) {
    return t("err.externalControlCalibrate");
  }
  if (trimmed.includes("corresponde a un movimiento comandado")) {
    return t("err.commandedEchoCalibrate");
  }
  if (trimmed.startsWith("err.")) {
    return t(trimmed);
  }

  return raw;
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
