<script lang="ts">
  /**
   * Typed output renderer. Each output spec maps to a small component
   * style depending on `kind`:
   *   - number → big violet chip (precision-aware)
   *   - text   → preformatted block
   *   - table  → styled HTML table
   *   - series → tiny SVG line chart matching `Smilie.plot` style
   *
   * Pure render; the parent owns the `values` dict and refreshes on every
   * `result` postMessage from the runner iframe.
   */
  import type { OutputSpec } from "$lib/ipc";

  interface Props {
    schema: OutputSpec[];
    values: Record<string, unknown> | null;
  }

  let { schema, values }: Props = $props();

  function formatNumber(v: unknown, precision: number | undefined): string {
    if (typeof v !== "number" || !Number.isFinite(v)) return "—";
    if (typeof precision === "number") {
      return v.toFixed(precision);
    }
    // Heuristic: integers stay unformatted, floats trim to 4 sig figs.
    if (Number.isInteger(v)) return String(v);
    return v.toPrecision(4);
  }

  function readSeries(v: unknown): { x: number[]; y: number[] } | null {
    if (!v || typeof v !== "object") return null;
    const o = v as { x?: unknown; y?: unknown };
    if (!Array.isArray(o.x) || !Array.isArray(o.y)) return null;
    const x = o.x.filter((n): n is number => typeof n === "number");
    const y = o.y.filter((n): n is number => typeof n === "number");
    if (x.length === 0 || y.length === 0) return null;
    return { x, y };
  }

  function readTable(v: unknown): unknown[][] | null {
    if (!Array.isArray(v)) return null;
    return v.map((row) => (Array.isArray(row) ? row : [row]));
  }

  function pathForSeries(
    x: number[],
    y: number[],
    w: number,
    h: number,
  ): string {
    const padX = 24;
    const padY = 16;
    const minX = Math.min(...x);
    const maxX = Math.max(...x);
    const minY = Math.min(...y);
    const maxY = Math.max(...y);
    const dx = maxX - minX || 1;
    const dy = maxY - minY || 1;
    const points = x.map((xv, i) => {
      const px = padX + ((xv - minX) / dx) * (w - padX * 2);
      const py = h - padY - ((y[i] - minY) / dy) * (h - padY * 2);
      return `${px.toFixed(1)},${py.toFixed(1)}`;
    });
    return `M ${points.join(" L ")}`;
  }
</script>

<div class="outputs">
  {#if schema.length === 0}
    <p class="empty">This experiment has no declared outputs.</p>
  {:else if !values}
    <p class="empty">Run the experiment to see results here.</p>
  {:else}
    {#each schema as spec (spec.name)}
      <div class="output-block">
        <div class="output-label">{spec.label || spec.name}</div>

        {#if spec.kind === "number"}
          <div class="number-chip">
            {formatNumber(values[spec.name], spec.precision)}
          </div>
        {:else if spec.kind === "text"}
          <pre class="text-block">{String(values[spec.name] ?? "")}</pre>
        {:else if spec.kind === "table"}
          {@const rows = readTable(values[spec.name])}
          {#if rows && rows.length > 0}
            <div class="table-wrap">
              <table>
                <thead>
                  <tr>
                    {#each spec.columns as col (col)}
                      <th>{col}</th>
                    {/each}
                  </tr>
                </thead>
                <tbody>
                  {#each rows as row, ri (ri)}
                    <tr>
                      {#each spec.columns as _col, ci (ci)}
                        <td>{String(row[ci] ?? "")}</td>
                      {/each}
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {:else}
            <div class="placeholder">no rows</div>
          {/if}
        {:else if spec.kind === "series"}
          {@const series = readSeries(values[spec.name])}
          {#if series}
            {@const w = 360}
            {@const h = 160}
            <svg
              class="series"
              viewBox="0 0 {w} {h}"
              preserveAspectRatio="none"
              role="img"
              aria-label="{spec.label} series"
            >
              <line
                x1="24"
                y1={h - 16}
                x2={w - 16}
                y2={h - 16}
                stroke="rgba(164,152,196,0.4)"
                stroke-width="1"
              />
              <line
                x1="24"
                y1="16"
                x2="24"
                y2={h - 16}
                stroke="rgba(164,152,196,0.4)"
                stroke-width="1"
              />
              <path
                d={pathForSeries(series.x, series.y, w, h)}
                stroke="var(--violet-glow)"
                stroke-width="2"
                fill="none"
                stroke-linejoin="round"
                stroke-linecap="round"
              />
            </svg>
            {#if spec.x_label || spec.y_label}
              <div class="series-axes">
                {#if spec.y_label}<span>↑ {spec.y_label}</span>{/if}
                {#if spec.x_label}<span>→ {spec.x_label}</span>{/if}
              </div>
            {/if}
          {:else}
            <div class="placeholder">no series data</div>
          {/if}
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .outputs {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .empty,
  .placeholder {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--text-xs);
    font-style: italic;
  }

  .output-block {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .output-label {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--violet-200);
  }

  .number-chip {
    align-self: flex-start;
    padding: 10px 18px;
    border-radius: var(--radius-md);
    background: linear-gradient(
      160deg,
      rgba(154, 107, 255, 0.14),
      rgba(106, 45, 255, 0.06)
    );
    border: 1px solid var(--border-strong);
    color: var(--text-hi);
    font-family: var(--font-mono);
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .text-block {
    margin: 0;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.18);
    border: 1px solid var(--border-subtle);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-hi);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 240px;
    overflow: auto;
  }

  .table-wrap {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    overflow: auto;
    max-height: 320px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-xs);
  }

  th,
  td {
    padding: 6px 10px;
    text-align: left;
    border-bottom: 1px solid var(--border-subtle);
  }

  th {
    font-weight: 600;
    color: var(--violet-100);
    background: rgba(154, 107, 255, 0.06);
    position: sticky;
    top: 0;
  }

  td {
    font-family: var(--font-mono);
    color: var(--text-mid);
  }

  tr:last-child td {
    border-bottom: 0;
  }

  .series {
    display: block;
    width: 100%;
    height: auto;
    max-height: 220px;
    background: rgba(0, 0, 0, 0.18);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }

  .series-axes {
    display: flex;
    gap: 14px;
    margin-top: 4px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-dim);
  }
</style>
