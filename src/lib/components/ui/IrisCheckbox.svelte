<script lang="ts">
  import { Check } from "@lucide/svelte";

  interface Props {
    checked?: boolean;
    label?: string;
    description?: string;
    disabled?: boolean;
    variant?: "checkbox" | "switch";
    size?: "sm" | "md";
    id?: string;
    class?: string;
    style?: string;
    onchange?: (checked: boolean) => void;
  }

  let {
    checked = $bindable(false),
    label,
    description,
    disabled = false,
    variant = "checkbox",
    size = "md",
    id,
    class: className = "",
    style = "",
    onchange,
  }: Props = $props();

  const autoId = `iris-check-${Math.random().toString(36).slice(2, 9)}`;
  const elementId = $derived(id ?? autoId);

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (disabled) return;
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      toggle();
    }
  }
</script>

<!-- svelte-ignore a11y_role_has_required_aria_props -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<label
  for={elementId}
  class="iris-checkbox-root variant-{variant} size-{size} {className}"
  class:is-checked={checked}
  class:is-disabled={disabled}
  {style}
>
  <input
    {id}
    type="checkbox"
    class="sr-only"
    bind:checked
    {disabled}
    onkeydown={handleKeyDown}
    onchange={() => onchange?.(checked)}
  />

  {#if variant === "checkbox"}
    <span
      class="iris-checkbox-control"
      role="checkbox"
      aria-checked={checked}
      aria-disabled={disabled}
      tabindex={disabled ? -1 : 0}
      onkeydown={handleKeyDown}
      onclick={toggle}
    >
      <span class="iris-check-icon" class:active={checked}>
        <Check size={size === "sm" ? 11 : 13} strokeWidth={2.75} />
      </span>
    </span>
  {:else}
    <span
      class="iris-switch-control"
      role="switch"
      aria-checked={checked}
      aria-disabled={disabled}
      tabindex={disabled ? -1 : 0}
      onkeydown={handleKeyDown}
      onclick={toggle}
    >
      <span class="iris-switch-thumb" class:active={checked}></span>
    </span>
  {/if}

  {#if label || description}
    <div class="iris-checkbox-text">
      {#if label}
        <span class="iris-checkbox-label">{label}</span>
      {/if}
      {#if description}
        <span class="iris-checkbox-desc">{description}</span>
      {/if}
    </div>
  {/if}
</label>

<style>
  .iris-checkbox-root {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    user-select: none;
    font-family: inherit;
    position: relative;
    vertical-align: middle;
    transition: opacity 150ms var(--ease, ease);
  }

  .iris-checkbox-root.is-disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border-width: 0;
  }

  /* =========================================================================
     MODERN CHECKBOX VARIANT
     ========================================================================= */
  .iris-checkbox-control {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    background: var(--select-bg, rgba(22, 16, 36, 0.85));
    border: 1px solid var(--select-border, rgba(184, 169, 255, 0.22));
    box-shadow: inset 0 1px 0 0 rgba(255, 255, 255, 0.08);
    transition:
      background-color 160ms cubic-bezier(0.16, 1, 0.3, 1),
      border-color 160ms cubic-bezier(0.16, 1, 0.3, 1),
      box-shadow 200ms cubic-bezier(0.16, 1, 0.3, 1),
      transform 120ms cubic-bezier(0.16, 1, 0.3, 1);
    flex-shrink: 0;
    outline: none;
  }

  .size-sm .iris-checkbox-control {
    width: 16px;
    height: 16px;
    border-radius: 5px;
  }

  .size-md .iris-checkbox-control {
    width: 19px;
    height: 19px;
    border-radius: 6px;
  }

  .iris-checkbox-root:hover:not(.is-disabled) .iris-checkbox-control {
    border-color: var(--lavender, rgba(184, 169, 255, 0.45));
    background: var(--select-surface-hover, rgba(255, 255, 255, 0.06));
  }

  .is-checked .iris-checkbox-control {
    background: var(--accent, #3d5afe) !important;
    border-color: var(--accent, #3d5afe) !important;
    box-shadow: 0 0 12px var(--accent-glow, rgba(61, 90, 254, 0.35)) !important;
  }

  .iris-checkbox-control:focus-visible {
    outline: 2px solid var(--accent, #3d5afe);
    outline-offset: 2px;
  }

  .iris-checkbox-control:active:not(.is-disabled) {
    transform: scale(0.92);
  }

  .iris-check-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--accent-foreground, #ffffff);
    opacity: 0;
    transform: scale(0.4) rotate(-10deg);
    transition:
      opacity 160ms cubic-bezier(0.16, 1, 0.3, 1),
      transform 200ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .iris-check-icon.active {
    opacity: 1;
    transform: scale(1) rotate(0deg);
  }

  /* =========================================================================
     MODERN SWITCH / TOGGLE VARIANT
     ========================================================================= */
  .iris-switch-control {
    position: relative;
    display: inline-flex;
    align-items: center;
    border-radius: 999px;
    background: rgba(184, 169, 255, 0.16);
    border: 1px solid rgba(184, 169, 255, 0.18);
    transition:
      background-color 200ms cubic-bezier(0.16, 1, 0.3, 1),
      border-color 200ms cubic-bezier(0.16, 1, 0.3, 1),
      box-shadow 200ms cubic-bezier(0.16, 1, 0.3, 1);
    flex-shrink: 0;
    padding: 2px;
    outline: none;
  }

  .size-sm .iris-switch-control {
    width: 30px;
    height: 18px;
  }

  .size-md .iris-switch-control {
    width: 36px;
    height: 22px;
  }

  .is-checked .iris-switch-control {
    background: var(--accent, #3d5afe);
    border-color: var(--accent, #3d5afe);
    box-shadow: 0 0 14px var(--accent-glow, rgba(61, 90, 254, 0.32));
  }

  :root[data-theme="light"] .iris-switch-control {
    background: rgba(0, 0, 0, 0.12);
    border-color: rgba(0, 0, 0, 0.12);
  }

  :root[data-theme="light"] .is-checked .iris-switch-control {
    background: var(--accent, #3352ea);
    border-color: var(--accent, #3352ea);
  }

  .iris-switch-thumb {
    display: block;
    border-radius: 50%;
    background: #ffffff;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.25);
    transition: transform 220ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .size-sm .iris-switch-thumb {
    width: 12px;
    height: 12px;
  }

  .size-md .iris-switch-thumb {
    width: 16px;
    height: 16px;
  }

  .size-sm .iris-switch-thumb.active {
    transform: translateX(12px);
  }

  .size-md .iris-switch-thumb.active {
    transform: translateX(14px);
  }

  .iris-switch-control:focus-visible {
    outline: 2px solid var(--accent, #3d5afe);
    outline-offset: 2px;
  }

  /* Textos y Etiquetas */
  .iris-checkbox-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .iris-checkbox-label {
    font-size: 0.82rem;
    font-weight: 500;
    color: var(--text, #ece8ff);
    line-height: 1.25;
  }

  .iris-checkbox-desc {
    font-size: 0.72rem;
    color: var(--text-2, #a59cc7);
    line-height: 1.2;
  }

  :root[data-theme="light"] .iris-checkbox-control {
    background: #ffffff;
    border-color: rgba(0, 0, 0, 0.18);
    box-shadow: inset 0 1px 0 0 rgba(255, 255, 255, 0.9);
  }

  :root[data-theme="light"] .iris-checkbox-root:hover:not(.is-disabled) .iris-checkbox-control {
    border-color: var(--royal, #3352ea);
    background: rgba(0, 0, 0, 0.03);
  }

  :root[data-theme="light"] .iris-checkbox-label {
    color: var(--text, #14121d);
  }

  :root[data-theme="light"] .iris-checkbox-desc {
    color: var(--text-2, #5f5975);
  }
</style>
