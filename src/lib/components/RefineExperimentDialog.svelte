<script lang="ts">
  /**
   * Modal for forking an experiment with a refine prompt. Opened from
   * the runner toolbar's "✎ Refine with prompt" button. The actual fork
   * call lives in the parent so we can route the resulting summary into
   * the experiments store + autoselect it.
   *
   * Note: an "inherit current input values" toggle used to live here, but
   * it was dead UI — the parent never plumbed the boolean through. A
   * fork starts with the model-emitted defaults; the user can re-tune
   * after the new version lands. Re-add the toggle only when paired
   * with the matching plumbing in ExperimentTab/Runner.
   */

  interface Props {
    open: boolean;
    parentTitle: string;
    parentVersion: number;
    busy?: boolean;
    onSubmit: (prompt: string) => void;
    onCancel: () => void;
  }

  let {
    open,
    parentTitle,
    parentVersion,
    busy = false,
    onSubmit,
    onCancel,
  }: Props = $props();

  let prompt = $state("");

  function reset() {
    prompt = "";
  }

  // When the dialog re-opens for a new parent, clear the previous draft
  // — refine prompts are intentionally one-shot, not persistent.
  $effect(() => {
    if (open) {
      // Reset on open: fresh draft each time.
      prompt = "";
    }
  });

  function submit() {
    const trimmed = prompt.trim();
    if (!trimmed || busy) return;
    onSubmit(trimmed);
  }

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape" && !busy) {
      onCancel();
      reset();
    }
    // Cmd/Ctrl+Enter submits — Enter alone respects newlines in textarea.
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      submit();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if open}
  <div
    class="scrim"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget && !busy) onCancel();
    }}
  >
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="refine-title">
      <header>
        <div class="eyebrow">Refine experiment</div>
        <h3 id="refine-title">Fork “{parentTitle}” v{parentVersion}</h3>
        <p class="hint">
          Describe the change. The model will generate a new versioned copy;
          the original stays intact.
        </p>
      </header>

      <textarea
        bind:value={prompt}
        rows="5"
        placeholder="What should the model adjust? e.g. “use Adam instead of SGD” or “reduce the learning rate to 1e-4”"
        disabled={busy}
        aria-label="Refine prompt"
      ></textarea>

      <div class="actions">
        <button
          type="button"
          class="btn ghost"
          disabled={busy}
          onclick={() => {
            onCancel();
            reset();
          }}
        >
          Cancel
        </button>
        <button
          type="button"
          class="btn primary"
          disabled={busy || prompt.trim().length === 0}
          onclick={submit}
        >
          {busy ? "Forking…" : "Fork experiment"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 9999;
    background: rgba(8, 4, 18, 0.62);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    display: grid;
    place-items: center;
    animation: fade 0.14s ease;
  }

  .dialog {
    width: min(520px, calc(100vw - 32px));
    padding: 22px;
    border-radius: var(--radius-lg);
    background: linear-gradient(160deg, var(--bg-panel), var(--bg-elevated));
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-lg), 0 0 40px -12px var(--violet-glow);
    color: var(--text-hi);
    display: flex;
    flex-direction: column;
    gap: 14px;
    animation: rise 0.18s var(--ease);
  }

  header {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .eyebrow {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: var(--violet-200);
  }

  h3 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .hint {
    margin: 0;
    color: var(--text-mid);
    font-size: var(--text-xs);
    line-height: 1.55;
  }

  textarea {
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    color: var(--text-hi);
    font: inherit;
    font-size: var(--text-sm);
    line-height: 1.5;
    resize: vertical;
    min-height: 96px;
  }

  textarea:focus {
    outline: none;
    border-color: var(--violet-400);
    box-shadow: 0 0 0 3px rgba(154, 107, 255, 0.18);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .btn {
    padding: 8px 16px;
    font-size: var(--text-sm);
    border-radius: var(--radius-pill);
    border: 1px solid transparent;
    color: white;
    cursor: pointer;
    transition:
      transform var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease),
      background var(--dur) var(--ease);
  }

  .btn.primary {
    background: var(--violet-500);
  }
  .btn.primary:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 8px 20px -8px rgba(106, 45, 255, 0.7);
  }

  .btn.ghost {
    background: transparent;
    color: var(--text-mid);
    border-color: var(--border);
  }
  .btn.ghost:hover:not(:disabled) {
    color: var(--text-hi);
    border-color: var(--border-strong);
    background: rgba(255, 255, 255, 0.04);
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: progress;
  }

  @keyframes fade {
    from { opacity: 0; }
    to   { opacity: 1; }
  }

  @keyframes rise {
    from { opacity: 0; transform: translateY(8px) scale(0.98); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }
</style>
