export interface ShortcutAction {
  id: string;
  category: "playback" | "navigation" | "editing" | "timeline" | "general";
  labelKey: string;
  defaultKey: string;
  currentKey: string;
}

export const PREMIERE_DEFAULTS: Record<string, string> = {
  playPause: "Space",
  shuttleBack: "j",
  shuttleStop: "k",
  shuttleFwd: "l",
  stepPrev: "ArrowLeft",
  stepNext: "ArrowRight",
  jumpPrev1s: "Shift+ArrowLeft",
  jumpNext1s: "Shift+ArrowRight",
  prevKeyframe: "ArrowUp",
  nextKeyframe: "ArrowDown",
  addKeyframe: "c",
  selectTool: "v",
  copySelected: "Control+c",
  pasteSelected: "Control+v",
  duplicateSelected: "Control+d",
  deleteSelected: "Delete",
  toggleSnap: "s",
  zoomIn: "=",
  zoomOut: "-",
  homeStart: "Home",
  endFinish: "End",
  openKeybindings: "?",
};

export const SHORTCUT_ACTIONS: Omit<ShortcutAction, "currentKey">[] = [
  // Reproducción
  { id: "playPause", category: "playback", labelKey: "shortcuts.playPause", defaultKey: "Space" },
  { id: "shuttleBack", category: "playback", labelKey: "shortcuts.shuttleBack", defaultKey: "j" },
  { id: "shuttleStop", category: "playback", labelKey: "shortcuts.shuttleStop", defaultKey: "k" },
  { id: "shuttleFwd", category: "playback", labelKey: "shortcuts.shuttleFwd", defaultKey: "l" },

  // Navegación
  { id: "stepPrev", category: "navigation", labelKey: "shortcuts.stepPrev", defaultKey: "ArrowLeft" },
  { id: "stepNext", category: "navigation", labelKey: "shortcuts.stepNext", defaultKey: "ArrowRight" },
  { id: "jumpPrev1s", category: "navigation", labelKey: "shortcuts.jumpPrev1s", defaultKey: "Shift+ArrowLeft" },
  { id: "jumpNext1s", category: "navigation", labelKey: "shortcuts.jumpNext1s", defaultKey: "Shift+ArrowRight" },
  { id: "prevKeyframe", category: "navigation", labelKey: "shortcuts.prevKeyframe", defaultKey: "ArrowUp" },
  { id: "nextKeyframe", category: "navigation", labelKey: "shortcuts.nextKeyframe", defaultKey: "ArrowDown" },
  { id: "homeStart", category: "navigation", labelKey: "shortcuts.homeStart", defaultKey: "Home" },
  { id: "endFinish", category: "navigation", labelKey: "shortcuts.endFinish", defaultKey: "End" },

  // Edición
  { id: "addKeyframe", category: "editing", labelKey: "shortcuts.addKeyframe", defaultKey: "c" },
  { id: "selectTool", category: "editing", labelKey: "shortcuts.selectTool", defaultKey: "v" },
  { id: "copySelected", category: "editing", labelKey: "shortcuts.copySelected", defaultKey: "Control+c" },
  { id: "pasteSelected", category: "editing", labelKey: "shortcuts.pasteSelected", defaultKey: "Control+v" },
  { id: "duplicateSelected", category: "editing", labelKey: "shortcuts.duplicateSelected", defaultKey: "Control+d" },
  { id: "deleteSelected", category: "editing", labelKey: "shortcuts.deleteSelected", defaultKey: "Delete" },

  // Timeline
  { id: "toggleSnap", category: "timeline", labelKey: "shortcuts.toggleSnap", defaultKey: "s" },
  { id: "zoomIn", category: "timeline", labelKey: "shortcuts.zoomIn", defaultKey: "=" },
  { id: "zoomOut", category: "timeline", labelKey: "shortcuts.zoomOut", defaultKey: "-" },

  // General
  { id: "openKeybindings", category: "general", labelKey: "shortcuts.openKeybindings", defaultKey: "?" },
];

class ShortcutManager {
  bindings = $state<Record<string, string>>({ ...PREMIERE_DEFAULTS });
  private handlers = new Map<string, () => void>();

  constructor() {
    this.loadFromStorage();
  }

  private loadFromStorage() {
    if (typeof localStorage === "undefined") return;
    try {
      const saved = localStorage.getItem("velza_iris_keybindings");
      if (saved) {
        const parsed = JSON.parse(saved);
        this.bindings = { ...PREMIERE_DEFAULTS, ...parsed };
      }
    } catch {
      this.bindings = { ...PREMIERE_DEFAULTS };
    }
  }

  saveToStorage() {
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.setItem("velza_iris_keybindings", JSON.stringify(this.bindings));
    } catch {
      /* ignorar fallos de storage */
    }
  }

  setBinding(actionId: string, combo: string) {
    this.bindings[actionId] = combo;
    this.saveToStorage();
  }

  resetDefaults() {
    this.bindings = { ...PREMIERE_DEFAULTS };
    this.saveToStorage();
  }

  registerAction(actionId: string, handler: () => void): () => void {
    this.handlers.set(actionId, handler);
    return () => {
      this.handlers.delete(actionId);
    };
  }

  /**
   * Serializa un KeyboardEvent a string normalizado (ej. "Control+d", "Shift+ArrowLeft", "Space").
   */
  eventToCombo(e: KeyboardEvent): string {
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Control");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey && e.key !== "Shift") parts.push("Shift");
    if (e.metaKey) parts.push("Meta");

    let key = e.key;
    if (key === " ") key = "Space";
    if (key.length === 1) {
      key = key.toLowerCase();
    }
    if (!["Control", "Alt", "Shift", "Meta"].includes(e.key)) {
      parts.push(key);
    }
    return parts.join("+");
  }

  /**
   * Evalúa si un KeyboardEvent coincide con alguna acción registrada.
   */
  handleKeyEvent(e: KeyboardEvent): boolean {
    // Si el foco está en un input editable o textarea, no interceptar salvo Escape
    const target = e.target as HTMLElement | null;
    if (
      target &&
      (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)
    ) {
      return false;
    }

    const combo = this.eventToCombo(e);
    for (const [actionId, boundCombo] of Object.entries(this.bindings)) {
      if (boundCombo.toLowerCase() === combo.toLowerCase()) {
        const handler = this.handlers.get(actionId);
        if (handler) {
          e.preventDefault();
          handler();
          return true;
        }
      }
    }
    return false;
  }
}

export const shortcutManager = new ShortcutManager();
