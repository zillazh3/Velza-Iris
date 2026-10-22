import es from "./es-MX.json";
import en from "./en.json";

export type Key = keyof typeof es;
const dicts = { "es-MX": es, en } as const;
export type Locale = keyof typeof dicts;

// ponytail: el idioma no se guarda entre sesiones; persistirlo requiere un ajuste en VelzaIris-data.
let locale = $state<Locale>("es-MX");

export const i18n = {
  get locale() {
    return locale;
  },
  toggle() {
    locale = locale === "es-MX" ? "en" : "es-MX";
  },
};

export function t(key: Key, params: Record<string, string | number> = {}): string {
  return Object.entries(params).reduce((s, [k, v]) => s.replaceAll(`{${k}}`, String(v)), dicts[locale][key]);
}
