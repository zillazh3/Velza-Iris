<script lang="ts">
  interface Props {
    /** Letra corta que se muestra a la izquierda (X, Y, Z, P, R…). */
    label: string;
    /** Eje cuyo color semántico lleva la etiqueta. */
    axis: "x" | "y" | "z";
    value: number;
    step?: number;
    min?: number;
    max?: number;
    title?: string;
    oninput?: () => void;
  }

  let { label, axis, value = $bindable(0), step = 0.05, min, max, title, oninput }: Props = $props();
</script>

<label class="axis" style="--axis: var(--axis-{axis})" {title}>
  <span class="tag" aria-hidden="true">{label}</span>
  <input type="number" {step} {min} {max} bind:value {oninput} aria-label={title ?? label} />
</label>

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
</style>
