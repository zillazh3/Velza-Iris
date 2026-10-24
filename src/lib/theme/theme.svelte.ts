export type ThemeMode = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
export type AccentPreset = "electric" | "violet" | "emerald" | "rose" | "amber" | "cyan";

export interface AccentDefinition {
  id: AccentPreset;
  name: string;
  hex: string;
  rgb: string;
}

export const ACCENT_PRESETS: Record<AccentPreset, AccentDefinition> = {
  electric: { id: "electric", name: "Azul Eléctrico", hex: "#3d5afe", rgb: "61, 90, 254" },
  violet: { id: "violet", name: "Violeta Neón", hex: "#8b5cf6", rgb: "139, 92, 246" },
  emerald: { id: "emerald", name: "Esmeralda", hex: "#10b981", rgb: "16, 185, 129" },
  rose: { id: "rose", name: "Coral / Rosa", hex: "#f43f5e", rgb: "244, 63, 94" },
  amber: { id: "amber", name: "Ámbar Cálido", hex: "#f59e0b", rgb: "245, 158, 11" },
  cyan: { id: "cyan", name: "Cian Cibernético", hex: "#06b6d4", rgb: "6, 182, 212" },
};

let mode = $state<ThemeMode>("system");
let resolved = $state<ResolvedTheme>("dark");
let accent = $state<AccentPreset>("electric");
let customAccent = $state<string | null>(null);
let initialized = false;

function hexToRgb(hex: string): string | null {
  const clean = hex.replace("#", "").trim();
  if (clean.length === 3) {
    const r = parseInt(clean[0] + clean[0], 16);
    const g = parseInt(clean[1] + clean[1], 16);
    const b = parseInt(clean[2] + clean[2], 16);
    return `${r}, ${g}, ${b}`;
  }
  if (clean.length === 6) {
    const r = parseInt(clean.substring(0, 2), 16);
    const g = parseInt(clean.substring(2, 4), 16);
    const b = parseInt(clean.substring(4, 6), 16);
    return `${r}, ${g}, ${b}`;
  }
  return null;
}

function disableTransitionsTemporarily() {
  if (typeof document === "undefined") return;
  const style = document.createElement("style");
  style.appendChild(
    document.createTextNode(
      "*, *::before, *::after { transition: none !important; }"
    )
  );
  document.head.appendChild(style);
  return () => {
    // Force reflow
    void window.getComputedStyle(style).opacity;
    document.head.removeChild(style);
  };
}

function applyToDOM() {
  if (typeof document === "undefined") return;

  const enableTransitions = disableTransitionsTemporarily();
  const root = document.documentElement;

  // Resolve theme
  let effectiveTheme: ResolvedTheme = "dark";
  if (mode === "system") {
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    effectiveTheme = prefersDark ? "dark" : "light";
  } else {
    effectiveTheme = mode;
  }
  resolved = effectiveTheme;

  root.setAttribute("data-theme", effectiveTheme);
  root.style.colorScheme = effectiveTheme;

  // Resolve accent
  if (customAccent) {
    root.removeAttribute("data-accent");
    const rgb = hexToRgb(customAccent) ?? "61, 90, 254";
    root.style.setProperty("--accent-color", customAccent);
    root.style.setProperty("--accent-color-rgb", rgb);
    root.style.setProperty("--accent-glow", `rgba(${rgb}, 0.32)`);
    root.style.setProperty("--accent-subtle", `rgba(${rgb}, 0.12)`);
    root.style.setProperty("--accent-subtle-hover", `rgba(${rgb}, 0.18)`);
    root.style.setProperty("--accent-foreground", "#ffffff");
  } else {
    root.setAttribute("data-accent", accent);
    const preset = ACCENT_PRESETS[accent] || ACCENT_PRESETS.electric;
    root.style.removeProperty("--accent-color");
    root.style.removeProperty("--accent-color-rgb");
    root.style.removeProperty("--accent-glow");
    root.style.removeProperty("--accent-subtle");
    root.style.removeProperty("--accent-subtle-hover");
    root.style.removeProperty("--accent-foreground");
  }

  if (enableTransitions) {
    requestAnimationFrame(() => {
      enableTransitions();
    });
  }
}

export const themeManager = {
  get mode() {
    return mode;
  },
  get resolved() {
    return resolved;
  },
  get accent() {
    return accent;
  },
  get customAccent() {
    return customAccent;
  },
  get isDark() {
    return resolved === "dark";
  },

  setMode(newMode: ThemeMode) {
    mode = newMode;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("velza-theme-mode", newMode);
    }
    applyToDOM();
  },

  setAccent(newAccent: AccentPreset) {
    accent = newAccent;
    customAccent = null;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("velza-accent", newAccent);
      localStorage.removeItem("velza-custom-accent");
    }
    applyToDOM();
  },

  setCustomAccent(hex: string) {
    customAccent = hex;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("velza-custom-accent", hex);
    }
    applyToDOM();
  },

  init() {
    if (initialized || typeof window === "undefined") return;
    initialized = true;

    // Load persisted settings
    const storedMode = localStorage.getItem("velza-theme-mode") as ThemeMode | null;
    if (storedMode && ["system", "light", "dark"].includes(storedMode)) {
      mode = storedMode;
    }

    const storedCustomAccent = localStorage.getItem("velza-custom-accent");
    if (storedCustomAccent) {
      customAccent = storedCustomAccent;
    } else {
      const storedAccent = localStorage.getItem("velza-accent") as AccentPreset | null;
      if (storedAccent && Object.keys(ACCENT_PRESETS).includes(storedAccent)) {
        accent = storedAccent;
      }
    }

    // Apply immediate state
    applyToDOM();

    // Listen to OS system color-scheme changes
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const listener = () => {
      if (mode === "system") {
        applyToDOM();
      }
    };

    if (typeof mediaQuery.addEventListener === "function") {
      mediaQuery.addEventListener("change", listener);
    } else {
      // Legacy fallback
      mediaQuery.addListener(listener);
    }
  },
};
