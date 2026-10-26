<script lang="ts">
  import { onMount, tick } from "svelte";

  export interface ContextMenuItem {
    id?: string;
    label?: string;
    icon?: any;
    shortcut?: string;
    disabled?: boolean;
    danger?: boolean;
    separator?: boolean;
    action?: () => void;
  }

  interface Props {
    open: boolean;
    x: number;
    y: number;
    items: ContextMenuItem[];
    onclose: () => void;
  }

  let { open = $bindable(false), x, y, items = [], onclose }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  let adjustedX = $state(0);
  let adjustedY = $state(0);

  $effect(() => {
    if (open) {
      adjustedX = x;
      adjustedY = y;
      void tick().then(reposition);
    }
  });

  function reposition() {
    if (!menuEl) return;
    const rect = menuEl.getBoundingClientRect();
    const pad = 10;
    let nextX = x;
    let nextY = y;

    if (x + rect.width > window.innerWidth - pad) {
      nextX = Math.max(pad, window.innerWidth - rect.width - pad);
    }
    if (y + rect.height > window.innerHeight - pad) {
      nextY = Math.max(pad, window.innerHeight - rect.height - pad);
    }
    adjustedX = nextX;
    adjustedY = nextY;
  }

  function handleAction(item: ContextMenuItem) {
    if (item.disabled || item.separator) return;
    onclose();
    item.action?.();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    }
  }

  onMount(() => {
    function handlePointerDown(e: PointerEvent) {
      if (!open || !menuEl) return;
      if (!menuEl.contains(e.target as Node)) {
        onclose();
      }
    }
    window.addEventListener("pointerdown", handlePointerDown, { capture: true });
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, { capture: true });
      window.removeEventListener("keydown", handleKeyDown);
    };
  });
</script>

{#if open}
  <div
    bind:this={menuEl}
    class="iris-ctx-menu"
    style="left: {adjustedX}px; top: {adjustedY}px;"
    role="menu"
    tabindex="-1"
  >
    {#each items as item, idx (idx)}
      {#if item.separator}
        <div class="ctx-sep" role="separator"></div>
      {:else}
        {@const Icon = item.icon}
        <button
          type="button"
          class="ctx-item"
          class:danger={item.danger}
          disabled={item.disabled}
          role="menuitem"
          onclick={() => handleAction(item)}
        >
          <div class="ctx-left">
            {#if Icon}
              <span class="ctx-icon"><Icon size={14} strokeWidth={1.75} /></span>
            {/if}
            <span class="ctx-label">{item.label}</span>
          </div>

          {#if item.shortcut}
            <kbd class="ctx-shortcut">{item.shortcut}</kbd>
          {/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .iris-ctx-menu {
    position: fixed;
    z-index: 10000;
    min-width: 190px;
    max-width: 280px;
    padding: 5px;
    background: var(--select-surface, rgba(26, 18, 44, 0.92));
    backdrop-filter: blur(20px) saturate(180%);
    -webkit-backdrop-filter: blur(20px) saturate(180%);
    border-radius: 12px;
    box-shadow:
      0 0 0 1px rgba(255, 255, 255, 0.08),
      0 12px 28px -6px rgba(0, 0, 0, 0.65),
      0 24px 48px -12px rgba(0, 0, 0, 0.85);
    display: flex;
    flex-direction: column;
    gap: 2px;
    animation: ctxFadeIn 140ms cubic-bezier(0.16, 1, 0.3, 1);
    font-family: inherit;
    user-select: none;
  }

  @keyframes ctxFadeIn {
    from {
      opacity: 0;
      transform: scale(0.95);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  .ctx-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 6px 9px;
    background: transparent;
    border: 0;
    border-radius: 7px;
    color: var(--text);
    font-size: 0.8rem;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    transition: background 120ms ease, color 120ms ease;
  }

  .ctx-item:hover:not(:disabled) {
    background: var(--select-surface-hover, rgba(255, 255, 255, 0.08));
    color: var(--text, #ffffff);
  }

  .ctx-item.danger {
    color: #f87171;
  }

  .ctx-item.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
  }

  .ctx-item:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .ctx-left {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .ctx-icon {
    display: inline-flex;
    align-items: center;
    color: var(--text-2);
    flex-shrink: 0;
  }

  .ctx-item.danger .ctx-icon {
    color: inherit;
  }

  .ctx-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ctx-shortcut {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.7rem;
    color: var(--text-2);
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    padding: 1px 4px;
    flex-shrink: 0;
  }

  .ctx-sep {
    height: 1px;
    margin: 4px 2px;
    background: var(--surface-hover);
  }
</style>
