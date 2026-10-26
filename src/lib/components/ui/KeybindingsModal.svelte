<script lang="ts">
  import { onMount } from "svelte";
  import { X, Keyboard, RotateCcw, Search, Check, AlertCircle } from "@lucide/svelte";
  import {
    shortcutManager,
    SHORTCUT_ACTIONS,
    PREMIERE_DEFAULTS,
    type ShortcutAction,
  } from "$lib/shortcuts/shortcuts.svelte";
  import { t, type Key } from "$lib/i18n/index.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open = $bindable(false), onclose }: Props = $props();

  let searchFilter = $state("");
  let recordingActionId = $state<string | null>(null);
  let selectedCategory = $state<string>("all");

  const categories = $derived([
    { id: "all", label: t("nav.all") },
    { id: "playback", label: t("keybindings.tab.playback") },
    { id: "navigation", label: t("keybindings.tab.navigation") },
    { id: "editing", label: t("keybindings.tab.editing") },
    { id: "timeline", label: "Timeline" },
  ]);

  const filteredActions = $derived(
    SHORTCUT_ACTIONS.filter((act) => {
      const matchCat = selectedCategory === "all" || act.category === selectedCategory;
      const label = t(act.labelKey as Key).toLowerCase();
      const matchSearch =
        searchFilter.trim() === "" ||
        label.includes(searchFilter.toLowerCase()) ||
        act.id.toLowerCase().includes(searchFilter.toLowerCase());
      return matchCat && matchSearch;
    })
  );

  function startRecording(actionId: string) {
    recordingActionId = actionId;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      if (recordingActionId) {
        recordingActionId = null;
        e.stopPropagation();
      } else {
        onclose();
      }
      return;
    }

    if (recordingActionId) {
      e.preventDefault();
      e.stopPropagation();
      const combo = shortcutManager.eventToCombo(e);
      // Validar que no sea solo modifier key
      if (!["Control", "Alt", "Shift", "Meta"].includes(combo)) {
        shortcutManager.setBinding(recordingActionId, combo);
        recordingActionId = null;
      }
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    return () => {
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
    };
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="kb-backdrop" onclick={onclose}>
    <div
      class="kb-modal card"
      onclick={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Atajos de Teclado"
    >
      <div class="kb-head">
        <div class="kb-title-wrap">
          <Keyboard size={18} class="accent-icon" />
          <div>
            <h3>{t("keybindings.title")} (Premiere Pro)</h3>
            <p class="kb-subtitle">{t("keybindings.subtitle")}</p>
          </div>
        </div>
        <button type="button" class="ghost icon-btn" onclick={onclose} aria-label="Cerrar">
          <X size={16} />
        </button>
      </div>

      <div class="kb-toolbar">
        <div class="kb-search">
          <Search size={14} class="search-icon" />
          <input
            type="text"
            placeholder={t("keybindings.searchPlaceholder")}
            bind:value={searchFilter}
          />
        </div>

        <div class="kb-tabs">
          {#each categories as cat (cat.id)}
            <button
              type="button"
              class="kb-tab"
              class:active={selectedCategory === cat.id}
              onclick={() => (selectedCategory = cat.id)}
            >
              {cat.label}
            </button>
          {/each}
        </div>
      </div>

      <div class="kb-list">
        {#each filteredActions as act (act.id)}
          {@const currentCombo = shortcutManager.bindings[act.id] ?? act.defaultKey}
          {@const isRecording = recordingActionId === act.id}
          {@const isModified = currentCombo !== act.defaultKey}
          <div class="kb-row" class:recording={isRecording}>
            <div class="kb-action-info">
              <span class="kb-action-title">{t(act.labelKey as Key)}</span>
              <span class="kb-action-cat">{act.category}</span>
            </div>

            <div class="kb-action-binding">
              {#if isRecording}
                <span class="recording-badge">{t("keybindings.recordingBadge")}</span>
              {:else}
                <button
                  type="button"
                  class="kbd-btn"
                  class:modified={isModified}
                  title={t("keybindings.remapHelp")}
                  onclick={() => startRecording(act.id)}
                >
                  <kbd>{currentCombo}</kbd>
                </button>
              {/if}
            </div>
          </div>
        {/each}

        {#if filteredActions.length === 0}
          <div class="kb-empty">
            <span>{t("presets.empty")}</span>
          </div>
        {/if}
      </div>

      <div class="kb-footer">
        <button
          type="button"
          class="ghost sm"
          onclick={() => shortcutManager.resetDefaults()}
          title={t("keybindings.resetTooltip")}
        >
          <RotateCcw size={14} /> {t("keybindings.resetBtn")}
        </button>

        <button type="button" class="primary sm" onclick={onclose}>
          <Check size={14} /> OK
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .kb-backdrop {
    position: fixed;
    inset: 0;
    z-index: 10002;
    background: var(--surface);
    backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    animation: kbFadeIn 150ms ease-out;
  }

  @keyframes kbFadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .kb-modal {
    width: 100%;
    max-width: 660px;
    max-height: 85vh;
    background: var(--surface, #1a1230);
    border-radius: 18px;
    box-shadow:
      0 0 0 1px rgba(255, 255, 255, 0.08),
      0 24px 50px -12px rgba(0, 0, 0, 0.8);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .kb-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 18px 20px 14px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .kb-title-wrap {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  :global(.accent-icon) {
    color: var(--accent);
    margin-top: 2px;
    flex-shrink: 0;
  }

  .kb-title-wrap h3 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
    color: var(--text);
  }

  .kb-subtitle {
    margin: 3px 0 0;
    font-size: 0.78rem;
    color: var(--text-2);
  }

  .kb-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-subtle);
    flex-wrap: wrap;
  }

  .kb-search {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--surface-raised);
    border-radius: 9px;
    padding: 5px 10px;
    flex: 1;
    min-width: 180px;
  }

  :global(.search-icon) {
    color: var(--text-2);
    flex-shrink: 0;
  }

  .kb-search input {
    border: 0;
    background: transparent;
    color: var(--text);
    font-size: 0.8rem;
    width: 100%;
    outline: none;
  }

  .kb-tabs {
    display: flex;
    gap: 4px;
  }

  .kb-tab {
    padding: 4px 9px;
    font-size: 0.74rem;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
    border: 1px solid transparent;
    cursor: pointer;
  }

  .kb-tab:hover {
    color: var(--text, #ffffff);
    background: var(--surface-hover);
  }

  .kb-tab.active {
    color: var(--text, #ffffff);
    background: var(--accent-subtle);
  }

  .kb-list {
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    max-height: 400px;
    padding: 6px 12px;
  }

  .kb-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px;
    border-radius: 8px;
    transition: background 120ms ease;
  }

  .kb-row:hover {
    background: var(--border-subtle);
  }

  .kb-row.recording {
    background: rgba(var(--accent-color-rgb), 0.15);
    border: 1px dashed var(--accent);
  }

  .kb-action-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .kb-action-title {
    font-size: 0.82rem;
    font-weight: 500;
    color: var(--text);
  }

  .kb-action-cat {
    font-size: 0.68rem;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .kbd-btn {
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
  }

  .kbd-btn kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.78rem;
    color: var(--text);
    background: var(--surface);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    border-radius: 6px;
    padding: 3px 8px;
    display: inline-block;
    transition: border-color 140ms ease, background 140ms ease;
  }

  .kbd-btn:hover kbd {
    background: rgba(var(--accent-color-rgb), 0.12);
  }

  .kbd-btn.modified kbd {
    border-color: var(--text-2));
    color: var(--text-2));
  }

  .recording-badge {
    font-size: 0.74rem;
    color: var(--accent);
    animation: blink 1s infinite alternate;
  }

  @keyframes blink {
    from { opacity: 0.6; }
    to { opacity: 1; }
  }

  .kb-empty {
    padding: 24px;
    text-align: center;
    color: var(--text-2);
    font-size: 0.82rem;
  }

  .kb-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px;
    border-top: 1px solid var(--border-subtle);
    background: var(--surface-raised);
  }
</style>
