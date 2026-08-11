<script lang="ts">
  /**
   * Pure renderer that turns an `InputSpec[]` into a panel of controls.
   * Two-way binds via `bind:values` — each control writes back into the
   * `Record<string, unknown>` keyed by `name`. No Tauri / no fetch.
   *
   * The component is intentionally dumb: callers seed `values` from the
   * spec defaults, react to changes themselves, and pass to the runner.
   */
  import type { InputSpec } from "$lib/ipc";

  interface Props {
    schema: InputSpec[];
    values: Record<string, unknown>;
  }

  let { schema, values = $bindable() }: Props = $props();

  function setValue(name: string, value: unknown) {
    // Reassign the whole object so Svelte 5's `$bindable` reactivity
    // triggers downstream `$derived` lookups in the parent runner.
    values = { ...values, [name]: value };
  }

  function readNumber(spec: Extract<InputSpec, { kind: "number" | "int" }>): number {
    const v = values[spec.name];
    if (typeof v === "number" && Number.isFinite(v)) return v;
    return spec.default;
  }

  function readChoice(spec: Extract<InputSpec, { kind: "choice" }>): string {
    const v = values[spec.name];
    return typeof v === "string" ? v : spec.default;
  }

  function readText(spec: Extract<InputSpec, { kind: "text" }>): string {
    const v = values[spec.name];
    return typeof v === "string" ? v : spec.default;
  }

  function readVector(spec: Extract<InputSpec, { kind: "vector" }>): number[] {
    const v = values[spec.name];
    if (Array.isArray(v) && v.every((x) => typeof x === "number")) {
      return v as number[];
    }
    return spec.default;
  }

  function vectorWrite(name: string, idx: number, value: number, current: number[]) {
    const next = current.slice();
    next[idx] = Number.isFinite(value) ? value : 0;
    setValue(name, next);
  }
</script>

<div class="controls">
  {#if schema.length === 0}
    <p class="empty">No inputs — this experiment runs without parameters.</p>
  {:else}
    {#each schema as spec (spec.name)}
      <div class="row">
        <label for="exp-input-{spec.name}">
          <span class="label">{spec.label || spec.name}</span>
          {#if spec.help}
            <span class="help">{spec.help}</span>
          {/if}
        </label>

        {#if spec.kind === "number" || spec.kind === "int"}
          {@const value = readNumber(spec)}
          {@const min = spec.min ?? (spec.kind === "int" ? 0 : 0)}
          {@const max = spec.max ?? (spec.kind === "int" ? 100 : 1)}
          {@const step = spec.step ?? (spec.kind === "int" ? 1 : 0.01)}
          <div class="number">
            <input
              id="exp-input-{spec.name}"
              type="range"
              {min}
              {max}
              {step}
              {value}
              oninput={(e) =>
                setValue(
                  spec.name,
                  Number((e.target as HTMLInputElement).value),
                )}
            />
            <input
              type="number"
              {min}
              {max}
              {step}
              {value}
              oninput={(e) =>
                setValue(
                  spec.name,
                  Number((e.target as HTMLInputElement).value),
                )}
              aria-label="{spec.label} numeric value"
            />
          </div>
        {:else if spec.kind === "choice"}
          {@const value = readChoice(spec)}
          <select
            id="exp-input-{spec.name}"
            {value}
            onchange={(e) =>
              setValue(spec.name, (e.target as HTMLSelectElement).value)}
          >
            {#each spec.options as opt (opt)}
              <option value={opt} selected={opt === value}>{opt}</option>
            {/each}
          </select>
        {:else if spec.kind === "text"}
          {@const value = readText(spec)}
          <textarea
            id="exp-input-{spec.name}"
            rows={value.length > 80 ? 4 : 1}
            {value}
            oninput={(e) =>
              setValue(spec.name, (e.target as HTMLTextAreaElement).value)}
          ></textarea>
        {:else if spec.kind === "vector"}
          {@const arr = readVector(spec)}
          <div class="vector">
            {#each Array(spec.length) as _, i (i)}
              <input
                type="number"
                value={arr[i] ?? 0}
                step="0.01"
                oninput={(e) =>
                  vectorWrite(
                    spec.name,
                    i,
                    Number((e.target as HTMLInputElement).value),
                    arr,
                  )}
                aria-label="{spec.label} item {i + 1}"
              />
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .controls {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .empty {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--text-xs);
    font-style: italic;
  }

  .row {
    display: grid;
    grid-template-columns: minmax(120px, 220px) 1fr;
    gap: 14px;
    align-items: start;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-top: 4px;
  }

  .label {
    font-size: var(--text-sm);
    color: var(--text-hi);
    font-weight: 500;
  }

  .help {
    font-size: var(--text-xs);
    color: var(--text-dim);
    line-height: 1.4;
  }

  .number {
    display: grid;
    grid-template-columns: 1fr 90px;
    gap: 10px;
    align-items: center;
  }

  .vector {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(72px, 1fr));
    gap: 6px;
  }

  input[type="range"] {
    accent-color: var(--violet-500);
    width: 100%;
  }

  input[type="number"],
  textarea,
  select {
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    color: var(--text-hi);
    font: inherit;
    font-size: var(--text-xs);
    width: 100%;
  }

  textarea {
    resize: vertical;
    font-family: var(--font-mono);
    line-height: 1.5;
  }

  input[type="number"]:focus,
  textarea:focus,
  select:focus {
    outline: none;
    border-color: var(--violet-400);
    box-shadow: 0 0 0 3px rgba(154, 107, 255, 0.18);
  }

  @media (max-width: 720px) {
    .row {
      grid-template-columns: 1fr;
      gap: 6px;
    }
  }
</style>
