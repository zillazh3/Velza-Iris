<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ChevronDown, Check } from "@lucide/svelte";
  import { t } from "$lib/i18n/index.svelte";

  export interface SelectOption<T = string> {
    value: T;
    label: string;
    icon?: any;
    description?: string;
    shortcut?: string;
    disabled?: boolean;
  }

  interface Props<T = string> {
    items: SelectOption<T>[];
    value?: T;
    placeholder?: string;
    size?: "sm" | "md" | "lg";
    disabled?: boolean;
    name?: string;
    id?: string;
    ariaLabel?: string;
    class?: string;
    style?: string;
    glowOnActive?: boolean;
    onchange?: (value: T) => void;
  }

  let {
    items = [],
    value = $bindable(),
    placeholder,
    size = "md",
    disabled = false,
    name,
    id,
    ariaLabel,
    class: className = "",
    style = "",
    glowOnActive = true,
    onchange,
  }: Props = $props();

  const resolvedPlaceholder = $derived(placeholder ?? t("select.placeholder"));

  let isOpen = $state(false);
  let placement = $state<"bottom" | "top">("bottom");
  let highlightedIndex = $state<number>(-1);

  let rootEl = $state<HTMLDivElement | null>(null);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let listboxEl = $state<HTMLUListElement | null>(null);

  // Typeahead state
  let searchBuffer = "";
  let searchTimeout: ReturnType<typeof setTimeout> | null = null;

  const autoId = `iris-select-${Math.random().toString(36).slice(2, 9)}`;
  const instanceId = $derived(id ?? autoId);
  const listboxId = $derived(`${instanceId}-listbox`);

  const selectedItem = $derived(
    items.find((item) => item.value === value) ?? null
  );

  function calculatePlacement() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const spaceBelow = window.innerHeight - rect.bottom;
    const minSpaceRequired = 220;
    if (spaceBelow < minSpaceRequired && rect.top > minSpaceRequired) {
      placement = "top";
    } else {
      placement = "bottom";
    }
  }

  async function openDropdown() {
    if (disabled) return;
    calculatePlacement();
    isOpen = true;

    // Set initial highlighted index to current value or first non-disabled item
    const selectedIdx = items.findIndex((it) => it.value === value);
    if (selectedIdx >= 0 && !items[selectedIdx].disabled) {
      highlightedIndex = selectedIdx;
    } else {
      highlightedIndex = items.findIndex((it) => !it.disabled);
    }

    await tick();
    scrollHighlightedIntoView();
  }

  function closeDropdown() {
    isOpen = false;
    highlightedIndex = -1;
  }

  function toggleDropdown() {
    if (isOpen) {
      closeDropdown();
    } else {
      openDropdown();
    }
  }

  function selectOption(option: SelectOption) {
    if (option.disabled) return;
    value = option.value;
    onchange?.(option.value);
    closeDropdown();
    triggerEl?.focus();
  }

  function scrollHighlightedIntoView() {
    if (!listboxEl || highlightedIndex < 0) return;
    const highlightedEl = listboxEl.children[highlightedIndex] as HTMLElement | undefined;
    highlightedEl?.scrollIntoView({ block: "nearest" });
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (disabled) return;

    if (!isOpen) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) {
        event.preventDefault();
        openDropdown();
      }
      return;
    }

    switch (event.key) {
      case "Escape":
        event.preventDefault();
        closeDropdown();
        triggerEl?.focus();
        break;

      case "Tab":
        closeDropdown();
        break;

      case "ArrowDown": {
        event.preventDefault();
        let nextIdx = highlightedIndex + 1;
        while (nextIdx < items.length && items[nextIdx].disabled) {
          nextIdx++;
        }
        if (nextIdx < items.length) {
          highlightedIndex = nextIdx;
          scrollHighlightedIntoView();
        }
        break;
      }

      case "ArrowUp": {
        event.preventDefault();
        let prevIdx = highlightedIndex - 1;
        while (prevIdx >= 0 && items[prevIdx].disabled) {
          prevIdx--;
        }
        if (prevIdx >= 0) {
          highlightedIndex = prevIdx;
          scrollHighlightedIntoView();
        }
        break;
      }

      case "Home": {
        event.preventDefault();
        const firstEnabled = items.findIndex((it) => !it.disabled);
        if (firstEnabled >= 0) {
          highlightedIndex = firstEnabled;
          scrollHighlightedIntoView();
        }
        break;
      }

      case "End": {
        event.preventDefault();
        let lastEnabled = items.length - 1;
        while (lastEnabled >= 0 && items[lastEnabled].disabled) {
          lastEnabled--;
        }
        if (lastEnabled >= 0) {
          highlightedIndex = lastEnabled;
          scrollHighlightedIntoView();
        }
        break;
      }

      case "Enter":
      case " ": {
        event.preventDefault();
        if (highlightedIndex >= 0 && highlightedIndex < items.length) {
          const opt = items[highlightedIndex];
          if (!opt.disabled) {
            selectOption(opt);
          }
        }
        break;
      }

      default: {
        // Typeahead navigation (1 alphanumeric character)
        if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
          event.preventDefault();
          if (searchTimeout) clearTimeout(searchTimeout);
          searchBuffer += event.key.toLowerCase();
          searchTimeout = setTimeout(() => {
            searchBuffer = "";
          }, 600);

          const matchIdx = items.findIndex(
            (it, idx) =>
              !it.disabled &&
              (idx > highlightedIndex ? idx : items.length + idx) &&
              it.label.toLowerCase().startsWith(searchBuffer)
          );

          if (matchIdx >= 0) {
            highlightedIndex = matchIdx;
            scrollHighlightedIntoView();
          } else {
            // Check from start
            const fromStartIdx = items.findIndex(
              (it) => !it.disabled && it.label.toLowerCase().startsWith(searchBuffer)
            );
            if (fromStartIdx >= 0) {
              highlightedIndex = fromStartIdx;
              scrollHighlightedIntoView();
            }
          }
        }
        break;
      }
    }
  }

  onMount(() => {
    function handlePointerDownOutside(e: PointerEvent) {
      if (!isOpen || !rootEl) return;
      if (!rootEl.contains(e.target as Node)) {
        closeDropdown();
      }
    }

    function handleResizeOrScroll() {
      if (isOpen) {
        calculatePlacement();
      }
    }

    window.addEventListener("pointerdown", handlePointerDownOutside, { capture: true });
    window.addEventListener("resize", handleResizeOrScroll, { passive: true });
    window.addEventListener("scroll", handleResizeOrScroll, { passive: true, capture: true });

    return () => {
      window.removeEventListener("pointerdown", handlePointerDownOutside, { capture: true });
      window.removeEventListener("resize", handleResizeOrScroll);
      window.removeEventListener("scroll", handleResizeOrScroll, { capture: true });
    };
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  bind:this={rootEl}
  class="iris-select-root size-{size} {className}"
  class:is-open={isOpen}
  class:is-disabled={disabled}
  class:has-glow={glowOnActive && isOpen}
  {style}
  role="group"
  onkeydown={handleKeyDown}
>
  {#if name}
    <input type="hidden" {name} {value} />
  {/if}

  <!-- Trigger Button -->
  <button
    bind:this={triggerEl}
    type="button"
    class="iris-trigger"
    id={instanceId}
    role="combobox"
    aria-haspopup="listbox"
    aria-expanded={isOpen}
    aria-controls={listboxId}
    aria-label={ariaLabel ?? resolvedPlaceholder}
    aria-activedescendant={isOpen && highlightedIndex >= 0 ? `${instanceId}-opt-${highlightedIndex}` : undefined}
    {disabled}
    onclick={toggleDropdown}
  >
    <div class="iris-trigger-content">
      {#if selectedItem?.icon}
        {@const SelectedIcon = selectedItem.icon}
        <span class="iris-opt-icon">
          <SelectedIcon size={size === "sm" ? 14 : 16} />
        </span>
      {/if}

      <span class="iris-trigger-label" class:is-placeholder={!selectedItem}>
        {selectedItem ? selectedItem.label : resolvedPlaceholder}
      </span>
    </div>

    <!-- Chevron minimalista 1.5px con física elástica de rotación -->
    <span class="iris-chevron-wrapper" class:rotated={isOpen}>
      <ChevronDown size={size === "sm" ? 14 : 16} strokeWidth={1.5} />
    </span>
  </button>

  <!-- Flyout Menu Popover -->
  {#if isOpen}
    <div
      class="iris-flyout-container placement-{placement}"
      role="presentation"
    >
      <ul
        bind:this={listboxEl}
        id={listboxId}
        class="iris-listbox"
        role="listbox"
        tabindex="-1"
        aria-label={ariaLabel ?? placeholder}
      >
        {#each items as item, index (item.value)}
          {@const isSelected = item.value === value}
          {@const isHighlighted = index === highlightedIndex}
          <li
            id="{instanceId}-opt-{index}"
            class="iris-option"
            class:selected={isSelected}
            class:highlighted={isHighlighted}
            class:disabled={item.disabled}
            role="option"
            tabindex="-1"
            aria-selected={isSelected}
            aria-disabled={item.disabled}
            onpointerenter={() => {
              if (!item.disabled) highlightedIndex = index;
            }}
            onclick={() => selectOption(item)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                selectOption(item);
              }
            }}
          >
            <!-- Highlight floating pill background -->
            <span class="iris-option-pill"></span>

            <div class="iris-option-left">
              {#if item.icon}
                {@const ItemIcon = item.icon}
                <span class="iris-opt-icon">
                  <ItemIcon size={size === "sm" ? 14 : 16} />
                </span>
              {/if}

              <div class="iris-option-text">
                <span class="iris-option-label">{item.label}</span>
                {#if item.description}
                  <span class="iris-option-desc">{item.description}</span>
                {/if}
              </div>
            </div>

            <div class="iris-option-right">
              {#if item.shortcut}
                <kbd class="iris-option-shortcut">{item.shortcut}</kbd>
              {/if}

              <!-- Checkmark animado minimalista -->
              <span class="iris-checkmark" class:active={isSelected}>
                <Check size={14} strokeWidth={2} />
              </span>
            </div>
          </li>
        {/each}

        {#if items.length === 0}
          <li class="iris-empty-state">
            <span>{t("select.noOptions")}</span>
          </li>
        {/if}
      </ul>
    </div>
  {/if}
</div>

<style>
  .iris-select-root {
    position: relative;
    display: inline-block;
    width: 100%;
    font-family: inherit;
    user-select: none;
    --ease-spring: cubic-bezier(0.16, 1, 0.3, 1);
  }

  /* Trigger Button */
  .iris-trigger {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    background: var(--select-bg, rgba(22, 16, 36, 0.82));
    border: 1px solid var(--select-border, rgba(184, 169, 255, 0.14));
    box-shadow: var(--select-hairline, inset 0 1px 0 0 rgba(255, 255, 255, 0.08));
    color: var(--select-text-primary, #ece8ff);
    font-family: inherit;
    font-size: 0.875rem;
    font-weight: 500;
    line-height: 1.2;
    cursor: pointer;
    text-align: left;
    outline: none;
    box-sizing: border-box;
    transition:
      border-color 160ms var(--ease-spring),
      background-color 160ms var(--ease-spring),
      box-shadow 200ms var(--ease-spring),
      transform 120ms var(--ease-spring);
  }

  /* Tamaños del Trigger */
  .size-sm .iris-trigger {
    height: 32px;
    padding: 0 10px;
    border-radius: 9px;
    font-size: 0.8rem;
  }
  .size-md .iris-trigger {
    height: 38px;
    padding: 0 12px;
    border-radius: 11px;
    font-size: 0.875rem;
  }
  .size-lg .iris-trigger {
    height: 44px;
    padding: 0 16px;
    border-radius: 13px;
    font-size: 0.95rem;
  }

  /* Hover & Focus State */
  .iris-trigger:hover:not(:disabled) {
    border-color: var(--select-border-hover, rgba(184, 169, 255, 0.28));
    background: var(--select-surface, rgba(30, 22, 48, 0.88));
  }

  .iris-select-root.has-glow .iris-trigger,
  .iris-trigger:focus-visible {
    border-color: var(--accent-color, #3d5afe);
    box-shadow:
      0 0 0 2px var(--accent-glow, rgba(61, 90, 254, 0.28)),
      var(--select-hairline, inset 0 1px 0 0 rgba(255, 255, 255, 0.1));
  }

  /* Micro-presión elástica táctil */
  .iris-trigger:active:not(:disabled) {
    transform: scale(0.98);
  }

  .is-disabled .iris-trigger {
    opacity: 0.45;
    cursor: not-allowed;
    filter: grayscale(0.5);
  }

  /* Contenido interno del Trigger */
  .iris-trigger-content {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }

  .iris-trigger-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 500;
  }

  .iris-trigger-label.is-placeholder {
    color: var(--select-text-muted, #736a93);
    font-weight: 400;
  }

  .iris-opt-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--select-text-secondary, #a59cc7);
    flex-shrink: 0;
  }

  /* Chevron minimalista con física elástica de 180° */
  .iris-chevron-wrapper {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--select-text-secondary, #a59cc7);
    flex-shrink: 0;
    transition:
      transform 260ms var(--ease-spring),
      color 180ms ease;
  }

  .iris-trigger:hover:not(:disabled) .iris-chevron-wrapper {
    color: var(--accent-color, #3d5afe);
  }

  .iris-chevron-wrapper.rotated {
    transform: rotate(180deg);
    color: var(--accent-color, #3d5afe);
  }

  /* Flyout Menu Container */
  .iris-flyout-container {
    position: absolute;
    left: 0;
    right: 0;
    z-index: 9999;
    min-width: 100%;
    margin-top: 6px;
    animation-duration: 200ms;
    animation-timing-function: var(--ease-spring);
    animation-fill-mode: forwards;
  }

  .iris-flyout-container.placement-bottom {
    top: 100%;
    transform-origin: top center;
    animation-name: irisFlyoutInBottom;
  }

  .iris-flyout-container.placement-top {
    bottom: 100%;
    top: auto;
    margin-top: 0;
    margin-bottom: 6px;
    transform-origin: bottom center;
    animation-name: irisFlyoutInTop;
  }

  @keyframes irisFlyoutInBottom {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @keyframes irisFlyoutInTop {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  /* Listbox Flotante Glassmorphic */
  .iris-listbox {
    margin: 0;
    padding: 5px;
    list-style: none;
    background: var(--select-surface, rgba(28, 20, 44, 0.86));
    backdrop-filter: var(--select-backdrop-blur, blur(20px) saturate(180%));
    -webkit-backdrop-filter: var(--select-backdrop-blur, blur(20px) saturate(180%));
    border: 1px solid var(--select-border, rgba(184, 169, 255, 0.16));
    border-radius: 13px;
    box-shadow: var(--select-shadow-flyout);
    max-height: 280px;
    overflow-y: auto;
    overscroll-behavior: contain;
    outline: none;
    box-sizing: border-box;
  }

  /* Scrollbar estilizada ultra-fina */
  .iris-listbox::-webkit-scrollbar {
    width: 5px;
  }
  .iris-listbox::-webkit-scrollbar-track {
    background: transparent;
  }
  .iris-listbox::-webkit-scrollbar-thumb {
    background: rgba(184, 169, 255, 0.18);
    border-radius: 999px;
  }
  :root[data-theme="light"] .iris-listbox::-webkit-scrollbar-thumb {
    background: rgba(0, 0, 0, 0.12);
  }

  /* Opciones de la lista */
  .iris-option {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 7px 10px;
    border-radius: 8px;
    cursor: pointer;
    font-size: 0.85rem;
    color: var(--select-text-primary, #ece8ff);
    transition: color 140ms ease;
  }

  .size-sm .iris-option {
    padding: 5px 8px;
    font-size: 0.78rem;
  }
  .size-lg .iris-option {
    padding: 9px 12px;
    font-size: 0.9rem;
  }

  /* Píldora flotante animada en hover / highlight */
  .iris-option-pill {
    position: absolute;
    inset: 0;
    border-radius: 8px;
    pointer-events: none;
    background: transparent;
    transition:
      background-color 150ms var(--ease-spring),
      transform 150ms var(--ease-spring);
    z-index: 0;
  }

  .iris-option.highlighted .iris-option-pill {
    background: var(--select-surface-hover, rgba(255, 255, 255, 0.06));
  }

  .iris-option.selected .iris-option-pill {
    background: var(--accent-subtle, rgba(61, 90, 254, 0.12));
  }

  .iris-option.selected.highlighted .iris-option-pill {
    background: var(--accent-subtle-hover, rgba(61, 90, 254, 0.18));
  }

  .iris-option.selected {
    color: var(--accent-color, #3d5afe);
    font-weight: 600;
  }

  .iris-option.selected .iris-opt-icon {
    color: var(--accent-color, #3d5afe);
  }

  .iris-option.disabled {
    opacity: 0.38;
    cursor: not-allowed;
  }

  /* Contenido interno de la opción */
  .iris-option-left {
    position: relative;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }

  .iris-option-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .iris-option-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .iris-option-desc {
    font-size: 0.75rem;
    color: var(--select-text-muted, #736a93);
    font-weight: 400;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .iris-option.selected .iris-option-desc {
    color: var(--accent-color, #3d5afe);
    opacity: 0.8;
  }

  .iris-option-right {
    position: relative;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  /* Atajo de teclado estilo kbd Linear/Raycast */
  .iris-option-shortcut {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.72rem;
    color: var(--select-text-muted, #736a93);
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 5px;
    padding: 1px 5px;
  }

  :root[data-theme="light"] .iris-option-shortcut {
    background: rgba(0, 0, 0, 0.04);
    border-color: rgba(0, 0, 0, 0.08);
  }

  /* Checkmark animado (scale + fade) */
  .iris-checkmark {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    color: var(--accent-color, #3d5afe);
    opacity: 0;
    transform: scale(0.65);
    transition:
      opacity 180ms var(--ease-spring),
      transform 220ms var(--ease-spring);
  }

  .iris-checkmark.active {
    opacity: 1;
    transform: scale(1);
  }

  .iris-empty-state {
    padding: 16px;
    text-align: center;
    font-size: 0.82rem;
    color: var(--select-text-muted, #736a93);
  }
</style>
