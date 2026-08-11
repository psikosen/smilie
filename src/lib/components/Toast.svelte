<script lang="ts">
  import { toasts, toast, type ToastEntry } from "$lib/stores/toast";

  function dismiss(id: number) {
    toast.dismiss(id);
  }

  function iconFor(kind: ToastEntry["kind"]): string {
    switch (kind) {
      case "success": return "✓";
      case "error":   return "!";
      case "info":    return "i";
    }
  }
</script>

<div class="toast-stack" role="status" aria-live="polite" aria-atomic="false">
  {#each $toasts as t (t.id)}
    <div class="toast" data-kind={t.kind}>
      <span class="glyph" aria-hidden="true">{iconFor(t.kind)}</span>
      <span class="msg">{t.message}</span>
      <button
        class="close"
        type="button"
        aria-label="Dismiss"
        onclick={() => dismiss(t.id)}
      >×</button>
    </div>
  {/each}
</div>

<style>
  .toast-stack {
    position: fixed;
    right: 16px;
    bottom: max(16px, env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: var(--z-toast);
    pointer-events: none;
  }

  .toast {
    pointer-events: auto;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 10px;
    min-width: 220px;
    max-width: 360px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    background: var(--bg-glass);
    backdrop-filter: blur(20px) saturate(140%);
    -webkit-backdrop-filter: blur(20px) saturate(140%);
    border: 1px solid var(--border);
    color: var(--text-hi);
    font-size: var(--text-sm);
    box-shadow: var(--shadow-lg);
    animation: toast-in var(--dur) var(--ease);
  }

  .toast[data-kind="success"] {
    border-color: rgba(93, 255, 160, 0.35);
  }
  .toast[data-kind="success"] .glyph {
    color: var(--status-ok);
  }

  .toast[data-kind="error"] {
    border-color: rgba(255, 107, 123, 0.4);
  }
  .toast[data-kind="error"] .glyph {
    color: var(--status-err);
  }

  .toast[data-kind="info"] .glyph {
    color: var(--violet-100);
  }

  .glyph {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
  }

  .msg {
    min-width: 0;
    line-height: 1.35;
    word-break: break-word;
  }

  .close {
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    color: var(--text-dim);
    font-size: 16px;
    line-height: 1;
    transition: color var(--dur) var(--ease), background var(--dur) var(--ease);
  }

  .close:hover {
    color: var(--text-hi);
    background: rgba(255, 255, 255, 0.06);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }
</style>
