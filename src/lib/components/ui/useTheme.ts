// @ts-nocheck
import { useState, useEffect, useCallback } from "react";

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

function disableTransitions() {
  if (typeof document === "undefined") return () => {};
  const style = document.createElement("style");
  style.appendChild(
    document.createTextNode("*, *::before, *::after { transition: none !important; }")
  );
  document.head.appendChild(style);
  return () => {
    void window.getComputedStyle(style).opacity;
    document.head.removeChild(style);
  };
}

export function useTheme() {
  const [mode, setModeState] = useState<ThemeMode>("system");
  const [resolved, setResolvedState] = useState<ResolvedTheme>("dark");
  const [accent, setAccentState] = useState<AccentPreset>("electric");
  const [customAccent, setCustomAccentState] = useState<string | null>(null);

  const applyTheme = useCallback(
    (currentMode: ThemeMode, currentAccent: AccentPreset, customHex: string | null) => {
      if (typeof document === "undefined") return;

      const restoreTransitions = disableTransitions();
      const root = document.documentElement;

      // Determine active theme based on mode or system preference
      let effectiveTheme: ResolvedTheme = "dark";
      if (currentMode === "system") {
        const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
        effectiveTheme = prefersDark ? "dark" : "light";
      } else {
        effectiveTheme = currentMode;
      }

      setResolvedState(effectiveTheme);
      root.setAttribute("data-theme", effectiveTheme);
      root.style.colorScheme = effectiveTheme;

      // Apply dynamic accent variables
      if (customHex) {
        root.removeAttribute("data-accent");
        const rgb = hexToRgb(customHex) ?? "61, 90, 254";
        root.style.setProperty("--accent-color", customHex);
        root.style.setProperty("--accent-color-rgb", rgb);
        root.style.setProperty("--accent-glow", `rgba(${rgb}, 0.32)`);
        root.style.setProperty("--accent-subtle", `rgba(${rgb}, 0.12)`);
        root.style.setProperty("--accent-subtle-hover", `rgba(${rgb}, 0.18)`);
        root.style.setProperty("--accent-foreground", "#ffffff");
      } else {
        root.setAttribute("data-accent", currentAccent);
        root.style.removeProperty("--accent-color");
        root.style.removeProperty("--accent-color-rgb");
        root.style.removeProperty("--accent-glow");
        root.style.removeProperty("--accent-subtle");
        root.style.removeProperty("--accent-subtle-hover");
        root.style.removeProperty("--accent-foreground");
      }

      requestAnimationFrame(() => {
        restoreTransitions();
      });
    },
    []
  );

  useEffect(() => {
    if (typeof window === "undefined") return;

    const storedMode = localStorage.getItem("velza-theme-mode") as ThemeMode | null;
    const initialMode = storedMode && ["system", "light", "dark"].includes(storedMode) ? storedMode : "system";
    setModeState(initialMode);

    const storedCustomAccent = localStorage.getItem("velza-custom-accent");
    const storedAccent = localStorage.getItem("velza-accent") as AccentPreset | null;
    const initialAccent = storedAccent && ACCENT_PRESETS[storedAccent] ? storedAccent : "electric";

    setCustomAccentState(storedCustomAccent);
    setAccentState(initialAccent);

    applyTheme(initialMode, initialAccent, storedCustomAccent);

    // Watch system changes
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => {
      const activeStoredMode = (localStorage.getItem("velza-theme-mode") as ThemeMode) || "system";
      if (activeStoredMode === "system") {
        applyTheme("system", initialAccent, storedCustomAccent);
      }
    };

    if (mediaQuery.addEventListener) {
      mediaQuery.addEventListener("change", handler);
    } else {
      mediaQuery.addListener(handler);
    }

    return () => {
      if (mediaQuery.removeEventListener) {
        mediaQuery.removeEventListener("change", handler);
      } else {
        mediaQuery.removeListener(handler);
      }
    };
  }, [applyTheme]);

  const setMode = (newMode: ThemeMode) => {
    setModeState(newMode);
    localStorage.setItem("velza-theme-mode", newMode);
    applyTheme(newMode, accent, customAccent);
  };

  const setAccent = (newAccent: AccentPreset) => {
    setAccentState(newAccent);
    setCustomAccentState(null);
    localStorage.setItem("velza-accent", newAccent);
    localStorage.removeItem("velza-custom-accent");
    applyTheme(mode, newAccent, null);
  };

  const setCustomAccent = (hex: string) => {
    setCustomAccentState(hex);
    localStorage.setItem("velza-custom-accent", hex);
    applyTheme(mode, accent, hex);
  };

  return {
    mode,
    resolved,
    accent,
    customAccent,
    isDark: resolved === "dark",
    setMode,
    setAccent,
    setCustomAccent,
    presets: ACCENT_PRESETS,
  };
}
