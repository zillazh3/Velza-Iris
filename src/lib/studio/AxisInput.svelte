<script lang="ts">
  interface Props {
    /** Letra corta que se muestra a la izquierda (X, Y, Z, P, R, f, v…). */
    label: string;
    /** Eje cuyo color semántico lleva la etiqueta; `none` usa un gris neutro (lente, velocidad…). */
    axis: "x" | "y" | "z" | "none";
    value: number | null;
    step?: number;
    min?: number;
    max?: number;
    title?: string;
    oninput?: () => void;
    /** Modo lectura: no es un input, solo muestra el valor (`null` = «—»). */
    readonly?: boolean;
    /** Decimales en modo lectura. */
    digits?: number;
    /** Unidad en gris tras el valor (modo lectura). */
    unit?: string;
  }

  let {
    label,
    axis,
    value = $bindable(0),
    step = 0.05,
    min,
    max,
    title,
    oninput,
    readonly = false,
    digits = 2,
    unit,
  }: Props = $props();

  const axisColor = $derived(axis === "none" ? "var(--text-2)" : `var(--axis-${axis})`);
</script>

{#if readonly}
  <span class="axis ro" style="--axis: {axisColor}" {title}>
    <span class="tag" aria-hidden="true">{label}</span>
    <span class="val" class:empty={value == null}>{value == null ? "—" : value.toFixed(digits)}</span>
    {#if unit}<span class="unit">{unit}</span>{/if}
  </span>
{:else}
  <label class="axis" style="--axis: {axisColor}" {title}>
    <span class="tag" aria-hidden="true">{label}</span>
    <input type="number" {step} {min} {max} bind:value {oninput} aria-label={title ?? label} />
  </label>
{/if}

<style>
  .axis {
    display: flex;
    align-items: stretch;
    min-width: 0;
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    overflow: hidden;
  }
  .axis:focus-within {
    outline: 2px solid var(--accent);
  }
  .tag {
    display: grid;
    place-items: center;
    width: 20px;
    flex: none;
    font-size: 0.68rem;
    font-weight: 700;
    color: var(--axis);
    background: color-mix(in srgb, var(--axis) 12%, transparent);
  }
  input {
    flex: 1;
    min-width: 0;
    padding: 6px 8px;
    border: 0;
    background: transparent;
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 0.82rem;
    appearance: textfield;
    -moz-appearance: textfield;
  }
  input:focus {
    outline: none;
  }
  input::-webkit-inner-spin-button,
  input::-webkit-outer-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }

  /* Lectura: misma celda que el input, pero compacta y con ancho fijo para que el número no baile */
  .ro {
    align-items: center;
    cursor: default;
  }
  .ro .tag {
    align-self: stretch;
    width: 18px;
  }
  .val {
    min-width: 5.2ch;
    padding: 3px 6px;
    text-align: right;
    font-family: var(--font-mono);
    font-size: 0.76rem;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }
  .val.empty {
    color: var(--text-muted);
  }
  .unit {
    padding-right: 6px;
    font-size: 0.66rem;
    color: var(--text-muted);
  }
</style>
