<script lang="ts">
  import { onMount } from "svelte";
  import {
    ipc,
    type RuntimeStatus,
    type RuntimeStatusReport,
    type Settings,
  } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";
  import { toast } from "$lib/stores/toast";

  const RUNTIME_ROLES = ["ocr", "reasoning", "generator"] as const;

  let settings = $state<Settings | null>(null);
  let runtime = $state<RuntimeStatusReport | null>(null);
  let loading = $state(true);
  let pendingSaves = $state(0);
  let saving = $derived(pendingSaves > 0);
  let rechecking = $state(false);
  let revealing = $state(false);
  let loadError = $state<string | null>(null);
  let saveError = $state<string | null>(null);

  onMount(async () => {
    if (!isTauri()) {
      loading = false;
      return;
    }
    try {
      settings = await ipc.getSettings();
      runtime = await ipc.listRuntimeStatus();
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  });

  function preview(patch: Partial<Settings>) {
    if (settings) settings = { ...settings, ...patch };
  }

  async function commit(patch: Partial<Settings>) {
    if (!settings) return;
    preview(patch);
    saveError = null;
    pendingSaves += 1;
    try {
      await ipc.updateSettings(patch);
    } catch (e) {
      saveError = `Could not save settings: ${String(e)}`;
    } finally {
      pendingSaves -= 1;
    }
  }

  function runtimeLabel(s: RuntimeStatus): string {
    switch (s.kind) {
      case "ready": return `Ready · ${s.model}`;
      case "missing_weights": return `Missing: ${s.expected_path}`;
      case "error": return `Error: ${s.message}`;
    }
  }

  function runtimeClass(s: RuntimeStatus): string {
    switch (s.kind) {
      case "ready": return "ok";
      case "missing_weights": return "warn";
      case "error": return "err";
    }
  }

  async function revealModels() {
    if (revealing) return;
    revealing = true;
    try {
      const path = await ipc.revealModelsFolder();
      toast.info(`Opened ${path}`);
    } catch (e) {
      toast.error(`Could not open models folder: ${String(e)}`);
    } finally {
      revealing = false;
    }
  }

  async function recheck() {
    if (rechecking) return;
    rechecking = true;
    try {
      runtime = await ipc.recheckRuntimeStatus();
      toast.success("Runtimes re-checked");
    } catch (e) {
      toast.error(`Re-check failed: ${String(e)}`);
    } finally {
      rechecking = false;
    }
  }
</script>

<div class="settings noise-bg tokyo-grid fade-rise">
  <header>
    <h1>Settings</h1>
    <p>Configure runtimes, generation parameters, and storage.</p>
  </header>

  {#if loading}
    <div class="loading">Loading…</div>
  {:else if !isTauri()}
    <div class="notice">Open in the desktop app to edit settings.</div>
  {:else if loadError}
    <div class="notice err" role="alert">{loadError}</div>
  {:else if settings}
    <section class="card">
      <h2 class="section-title">Runtime status</h2>
      <div class="runtime-actions">
        <button
          type="button"
          class="action-btn"
          onclick={revealModels}
          disabled={revealing}
          title="Open the assets/models folder in the OS file manager"
        >
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" aria-hidden="true">
            <path d="M3.5 6.5h6l2 2h9v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-11z" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" />
          </svg>
          {revealing ? "Opening…" : "Open models folder"}
        </button>
        <button
          type="button"
          class="action-btn"
          onclick={recheck}
          disabled={rechecking}
          title="Re-evaluate runtime status without restarting"
        >
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" aria-hidden="true">
            <path d="M19 8a7.5 7.5 0 1 0 .3 7.5M19 4v4h-4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
          {rechecking ? "Re-checking…" : "Re-check"}
        </button>
      </div>
      <div class="runtime-grid">
        {#each RUNTIME_ROLES as role}
          {#if runtime}
            {@const s = runtime[role]}
            <div class="role-row {runtimeClass(s)}">
              <span class="role">{role}</span>
              <span class="kind">{s.kind.replace("_", " ")}</span>
              <span class="detail">{runtimeLabel(s)}</span>
            </div>
          {/if}
        {/each}
      </div>
      <div class="hint">
        Drop weights into <code>assets/models/&lt;role&gt;/</code> and click
        <em>Re-check</em> — or edit <code>assets/models/manifest.json</code>
        to point at a custom path.
      </div>
    </section>

    <section class="card">
      <h2 class="section-title">Generation</h2>
      <div class="row">
        <label>
          Temperature
          <input
            type="range"
            min="0"
            max="2"
            step="0.05"
            value={settings.temperature}
            oninput={(e) => preview({ temperature: Number((e.target as HTMLInputElement).value) })}
            onchange={(e) => commit({ temperature: Number((e.target as HTMLInputElement).value) })}
          />
          <span class="mono">{settings.temperature.toFixed(2)}</span>
        </label>
      </div>
      <div class="row">
        <label>
          Max tokens
          <input
            type="number"
            min="256"
            max="32768"
            step="256"
            value={settings.max_tokens}
            onchange={(e) => commit({ max_tokens: Number((e.target as HTMLInputElement).value) })}
          />
        </label>
        <label>
          Context length
          <input
            type="number"
            min="1024"
            max="131072"
            step="1024"
            value={settings.context_length}
            onchange={(e) => commit({ context_length: Number((e.target as HTMLInputElement).value) })}
          />
        </label>
      </div>
      <div class="row">
        <label class="check">
          <input
            type="checkbox"
            checked={settings.thinking_mode}
            onchange={(e) => commit({ thinking_mode: (e.target as HTMLInputElement).checked })}
          />
          Thinking mode (if supported)
        </label>
        <label class="check">
          <input
            type="checkbox"
            checked={settings.offline_only}
            onchange={(e) => commit({ offline_only: (e.target as HTMLInputElement).checked })}
          />
          Offline-only
        </label>
      </div>
      <div class="row">
        <label>
          Interactive aggressiveness
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={settings.interactive_aggressiveness}
            oninput={(e) => preview({ interactive_aggressiveness: Number((e.target as HTMLInputElement).value) })}
            onchange={(e) => commit({ interactive_aggressiveness: Number((e.target as HTMLInputElement).value) })}
          />
          <span class="mono">{settings.interactive_aggressiveness.toFixed(2)}</span>
        </label>
      </div>
    </section>

    <section class="card">
      <h2 class="section-title">About</h2>
      <div class="about">
        <div><strong>Smilie</strong> · scientific paper → interactive learning pages</div>
        <div class="dim">Runtime backend: <code>{settings.runtime_backend}</code></div>
      </div>
    </section>

    {#if saveError}
      <div class="notice err save-status" role="alert">{saveError}</div>
    {:else if saving}
      <div class="saving save-status" role="status">Saving…</div>
    {/if}
  {/if}
</div>

<style>
  .settings {
    position: relative;
    padding: 48px;
    max-width: 760px;
    margin: 0 auto;
    overflow-y: auto;
    height: calc(100vh - 56px);
    height: calc(100dvh - 56px);
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .settings::before {
    content: "";
    position: fixed;
    inset: 56px 0 0;
    pointer-events: none;
    background:
      radial-gradient(circle at 78% 18%, rgba(154, 107, 255, 0.12), transparent 34%),
      radial-gradient(circle at 16% 78%, rgba(59, 221, 255, 0.07), transparent 28%);
  }

  .settings > * {
    position: relative;
    z-index: 1;
  }

  header h1 {
    margin: 0;
    font-size: var(--text-2xl);
    font-weight: 600;
    letter-spacing: -0.02em;
  }

  header p {
    margin: 4px 0 0;
    color: var(--text-mid);
    font-size: var(--text-sm);
  }

  .card {
    padding: 22px;
    border-radius: var(--radius-lg);
    background:
      linear-gradient(120deg, rgba(255, 255, 255, 0.035), transparent 32%),
      linear-gradient(160deg, rgba(14, 12, 25, 0.96), rgba(7, 6, 13, 0.98));
    border: 1px solid var(--border);
    box-shadow: var(--shadow-md);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .section-title {
    margin: 0;
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: var(--violet-200);
  }

  .runtime-grid {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .runtime-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: rgba(154, 107, 255, 0.06);
    color: var(--text-hi);
    font: inherit;
    font-size: var(--text-xs);
    cursor: pointer;
    transition: transform var(--dur) var(--ease), border-color var(--dur) var(--ease), background var(--dur) var(--ease);
  }
  .action-btn:hover:not(:disabled) {
    border-color: var(--violet-400);
    background: rgba(154, 107, 255, 0.14);
    transform: translateY(-1px);
  }
  .action-btn:disabled {
    opacity: 0.55;
    cursor: progress;
  }

  .role-row {
    display: grid;
    grid-template-columns: 110px 110px 1fr;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--border-subtle);
    font-size: var(--text-sm);
  }

  .role-row .role {
    font-weight: 600;
    text-transform: capitalize;
    color: var(--text-hi);
  }
  .role-row .kind {
    font-family: var(--font-mono);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-mid);
  }
  .role-row .detail {
    color: var(--text-mid);
    font-size: var(--text-xs);
    word-break: break-all;
  }
  .role-row.ok .kind  { color: var(--status-ok); }
  .role-row.warn .kind { color: var(--status-warn); }
  .role-row.err .kind  { color: var(--status-err); }

  .hint {
    font-size: var(--text-xs);
    color: var(--text-dim);
    line-height: 1.6;
  }

  .hint code, .about code {
    font-family: var(--font-mono);
    color: var(--violet-100);
    background: rgba(154, 107, 255, 0.06);
    padding: 1px 5px;
    border-radius: 4px;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }

  .row label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: var(--text-xs);
    color: var(--text-mid);
  }

  .row label.check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    color: var(--text-hi);
    font-size: var(--text-sm);
  }

  input[type="range"] { accent-color: var(--violet-500); }
  input[type="number"] {
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    color: var(--text-hi);
    font: inherit;
  }
  input[type="number"]:focus {
    outline: none;
    border-color: var(--violet-400);
    box-shadow: 0 0 0 3px rgba(154, 107, 255, 0.18);
  }

  .mono {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--violet-200);
  }

  .loading,
  .notice,
  .saving {
    padding: 14px 16px;
    border-radius: var(--radius-md);
    border: 1px dashed var(--border);
    color: var(--text-mid);
    font-size: var(--text-sm);
  }

  .save-status {
    position: sticky;
    bottom: 0;
    background: rgba(8, 7, 16, 0.94);
    backdrop-filter: blur(16px);
  }

  .notice.err {
    color: var(--status-err);
    border-color: rgba(255, 107, 123, 0.3);
  }

  .about {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--text-sm);
  }
  .about .dim {
    color: var(--text-dim);
    font-size: var(--text-xs);
  }

  @media (max-width: 720px) {
    .settings {
      height: calc(100dvh - 56px);
      padding: 24px 16px max(32px, env(safe-area-inset-bottom));
      gap: 16px;
    }

    .card {
      padding: 18px 16px;
    }

    .row {
      grid-template-columns: 1fr;
    }

    .role-row {
      grid-template-columns: 1fr auto;
      gap: 6px 12px;
    }

    .role-row .detail {
      grid-column: 1 / -1;
    }
  }
</style>
