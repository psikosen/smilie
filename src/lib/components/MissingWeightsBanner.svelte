<script lang="ts">
  /**
   * Soft-warning banner for the paper-detail page when the active job
   * failed because a runtime is missing model weights. Renders nothing
   * on its own — the parent route decides when to mount it based on the
   * job error message.
   */
  import { ipc } from "$lib/ipc";
  import { toast } from "$lib/stores/toast";

  interface Props {
    expectedPath: string | null;
    errorMessage: string | null;
    onSettings: () => void;
  }

  const { expectedPath, errorMessage, onSettings }: Props = $props();

  let revealing = $state(false);

  async function reveal() {
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
</script>

<div class="banner" role="status" aria-live="polite">
  <div class="banner-icon" aria-hidden="true">⚠</div>
  <div class="banner-body">
    <div class="banner-title">Models not yet installed</div>
    <p class="banner-text">
      A runtime is missing weights{#if expectedPath}
        at <code>{expectedPath}</code>{/if}. Drop the model into that folder,
      then re-check from Settings.
    </p>
    {#if errorMessage}
      <details class="banner-details">
        <summary>Error detail</summary>
        <pre>{errorMessage}</pre>
      </details>
    {/if}
  </div>
  <div class="banner-actions">
    <button type="button" class="banner-btn primary" onclick={onSettings}>
      Open Settings
    </button>
    <button
      type="button"
      class="banner-btn"
      onclick={reveal}
      disabled={revealing}
    >
      {revealing ? "Opening…" : "Reveal folder"}
    </button>
  </div>
</div>

<style>
  .banner {
    width: 100%;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: start;
    gap: 16px;
    padding: 14px 18px;
    margin: 12px 16px 0;
    border-radius: var(--radius-lg);
    border: 1px dashed rgba(154, 107, 255, 0.55);
    background: linear-gradient(
      160deg,
      rgba(154, 107, 255, 0.10),
      rgba(106, 45, 255, 0.04)
    );
    color: var(--text-hi);
    box-shadow: 0 8px 24px -16px rgba(106, 45, 255, 0.4);
    box-sizing: border-box;
  }

  .banner-icon {
    font-size: 18px;
    line-height: 1;
    color: var(--status-warn, #ffc94a);
    margin-top: 2px;
  }

  .banner-body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .banner-title {
    font-weight: 600;
    font-size: var(--text-sm);
    letter-spacing: -0.01em;
  }

  .banner-text {
    margin: 0;
    color: var(--text-mid);
    font-size: var(--text-xs);
    line-height: 1.55;
  }

  .banner-text code {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--violet-100);
    background: rgba(154, 107, 255, 0.10);
    padding: 1px 5px;
    border-radius: 4px;
    word-break: break-all;
  }

  .banner-details {
    font-size: var(--text-xs);
    color: var(--text-dim);
  }
  .banner-details summary {
    cursor: pointer;
    color: var(--text-mid);
  }
  .banner-details pre {
    margin: 6px 0 0;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.18);
    border: 1px solid var(--border-subtle);
    font-family: var(--font-mono);
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .banner-actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-self: center;
  }

  .banner-btn {
    padding: 6px 14px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-hi);
    font: inherit;
    font-size: var(--text-xs);
    cursor: pointer;
    transition: transform var(--dur) var(--ease), border-color var(--dur) var(--ease), background var(--dur) var(--ease);
    white-space: nowrap;
  }
  .banner-btn:hover:not(:disabled) {
    border-color: var(--violet-400);
    background: rgba(154, 107, 255, 0.16);
    transform: translateY(-1px);
  }
  .banner-btn.primary {
    background: var(--violet-500);
    border-color: transparent;
    color: white;
  }
  .banner-btn.primary:hover:not(:disabled) {
    box-shadow: 0 8px 20px -8px rgba(106, 45, 255, 0.7);
    background: var(--violet-500);
  }
  .banner-btn:disabled {
    opacity: 0.55;
    cursor: progress;
  }

  @media (max-width: 640px) {
    .banner {
      grid-template-columns: auto 1fr;
      margin: 12px 12px 0;
    }
    .banner-actions {
      grid-column: 1 / -1;
      flex-direction: row;
    }
  }
</style>
