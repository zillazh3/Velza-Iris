import type { EffectClip, EffectKind } from "$lib/api";
import type { Key } from "$lib/i18n/index.svelte";

export type EffectCategory = "movement" | "micro" | "optics";

export type EffectCatalogEntry = {
  kind: EffectKind;
  category: EffectCategory;
  labelKey: Key;
  shortDesc: string;
  defaultDuration: number;
  defaultParamA: number | null;
  defaultParamB: number | null;
  paramALabel?: string;
  paramBLabel?: string;
};

export const EFFECT_CATALOG: EffectCatalogEntry[] = [
  // Movimientos
  {
    kind: "dollyIn",
    category: "movement",
    labelKey: "studio.effect.dollyIn",
    shortDesc: "Acerca la cámara suavemente hacia adelante sin alterar los puntos base.",
    defaultDuration: 3.0,
    defaultParamA: 0.8,
    defaultParamB: null,
    paramALabel: "Distancia (m)",
  },
  {
    kind: "dollyOut",
    category: "movement",
    labelKey: "studio.effect.dollyOut",
    shortDesc: "Aleja la cámara hacia atrás para revelar el entorno.",
    defaultDuration: 3.5,
    defaultParamA: 1.2,
    defaultParamB: null,
    paramALabel: "Distancia (m)",
  },
  {
    kind: "orbit",
    category: "movement",
    labelKey: "studio.effect.orbit",
    shortDesc: "Rodea parcialmente el ancla o punto de interés.",
    defaultDuration: 4.0,
    defaultParamA: 45.0,
    defaultParamB: null,
    paramALabel: "Ángulo (°)",
  },
  {
    kind: "truck",
    category: "movement",
    labelKey: "studio.effect.truck",
    shortDesc: "Desplaza la cámara lateralmente (izquierda/derecha).",
    defaultDuration: 3.0,
    defaultParamA: 0.8,
    defaultParamB: null,
    paramALabel: "Desplazamiento (m)",
  },
  {
    kind: "crane",
    category: "movement",
    labelKey: "studio.effect.crane",
    shortDesc: "Eleva o desciende la cámara verticalmente manteniendo el encuadre.",
    defaultDuration: 3.5,
    defaultParamA: 0.6,
    defaultParamB: null,
    paramALabel: "Altura (m)",
  },
  {
    kind: "pan",
    category: "movement",
    labelKey: "studio.effect.pan",
    shortDesc: "Gira la mirada de la cámara horizontalmente.",
    defaultDuration: 2.5,
    defaultParamA: 25.0,
    defaultParamB: null,
    paramALabel: "Giro (°)",
  },
  {
    kind: "tilt",
    category: "movement",
    labelKey: "studio.effect.tilt",
    shortDesc: "Inclina la mirada de la cámara hacia arriba o abajo.",
    defaultDuration: 2.5,
    defaultParamA: -15.0,
    defaultParamB: null,
    paramALabel: "Inclinación (°)",
  },
  {
    kind: "reveal",
    category: "movement",
    labelKey: "studio.effect.reveal",
    shortDesc: "Combina retroceso, desplazamiento lateral y giro para revelar la escena.",
    defaultDuration: 4.0,
    defaultParamA: 1.2,
    defaultParamB: null,
    paramALabel: "Recorrido (m)",
  },

  // Microefectos
  {
    kind: "microShake",
    category: "micro",
    labelKey: "studio.effect.microShake",
    shortDesc: "Vibración sutil de alta frecuencia para añadir energía o tensión.",
    defaultDuration: 2.0,
    defaultParamA: null,
    defaultParamB: null,
  },
  {
    kind: "impactShake",
    category: "micro",
    labelKey: "studio.effect.impactShake",
    shortDesc: "Sacudida breve con amortiguación exponencial para golpes o drops.",
    defaultDuration: 0.8,
    defaultParamA: null,
    defaultParamB: null,
  },
  {
    kind: "handheld",
    category: "micro",
    labelKey: "studio.effect.handheld",
    shortDesc: "Movimiento orgánico de cámara en mano sin rigidez robótica.",
    defaultDuration: 5.0,
    defaultParamA: null,
    defaultParamB: null,
  },
  {
    kind: "cameraDrift",
    category: "micro",
    labelKey: "studio.effect.cameraDrift",
    shortDesc: "Flotación onírica lenta en patrón suave para planos contemplativos.",
    defaultDuration: 6.0,
    defaultParamA: null,
    defaultParamB: null,
  },
  {
    kind: "cameraBreathing",
    category: "micro",
    labelKey: "studio.effect.cameraBreathing",
    shortDesc: "Oscilación suave al ritmo de una respiración tranquila.",
    defaultDuration: 6.0,
    defaultParamA: null,
    defaultParamB: null,
  },

  // Óptica
  {
    kind: "smoothZoom",
    category: "optics",
    labelKey: "studio.effect.smoothZoom",
    shortDesc: "Transición progresiva de distancia focal (mm).",
    defaultDuration: 4.0,
    defaultParamA: 35.0,
    defaultParamB: 65.0,
    paramALabel: "Focal inicial (mm)",
    paramBLabel: "Focal final (mm)",
  },
  {
    kind: "crashZoom",
    category: "optics",
    labelKey: "studio.effect.crashZoom",
    shortDesc: "Acercamiento óptico rápido de estilo videoclip.",
    defaultDuration: 0.9,
    defaultParamA: 30.0,
    defaultParamB: 95.0,
    paramALabel: "Focal inicial (mm)",
    paramBLabel: "Focal final (mm)",
  },
  {
    kind: "focusPull",
    category: "optics",
    labelKey: "studio.effect.focusPull",
    shortDesc: "Traslada el plano de enfoque entre dos distancias (requiere enfoque Manual en VRChat).",
    defaultDuration: 3.0,
    defaultParamA: 3.0,
    defaultParamB: 1.1,
    paramALabel: "Distancia A (m)",
    paramBLabel: "Distancia B (m)",
  },
  {
    kind: "apertureShift",
    category: "optics",
    labelKey: "studio.effect.apertureShift",
    shortDesc: "Abre o cierra el diafragma para desenfocar el fondo gradualmente.",
    defaultDuration: 3.5,
    defaultParamA: 11.0,
    defaultParamB: 2.0,
    paramALabel: "Apertura inicial (f/)",
    paramBLabel: "Apertura final (f/)",
  },
  {
    kind: "dollyZoom",
    category: "optics",
    labelKey: "studio.effect.dollyZoom",
    shortDesc: "Retrocede mientras cierra la focal para mantener el tamaño del sujeto (Efecto Vértigo).",
    defaultDuration: 4.0,
    defaultParamA: 1.5,
    defaultParamB: 22.0,
    paramALabel: "Retroceso (m)",
    paramBLabel: "Escala focal (mm/m)",
  },
];

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
