<script lang="ts">
  import { tick } from "svelte";

  interface Props {
    open: boolean;
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    busy?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let {
    open,
    title,
    message,
    confirmLabel = "Confirm",
    cancelLabel = "Cancel",
    danger = false,
    busy = false,
    onConfirm,
    onCancel,
  }: Props = $props();

  const dialogId = $props.id();
  let dialog = $state<HTMLDivElement | undefined>();
  let cancelButton = $state<HTMLButtonElement | undefined>();

  $effect(() => {
    if (!open) return;
    const previousFocus = document.activeElement as HTMLElement | null;
    void tick().then(() => cancelButton?.focus());
    return () => previousFocus?.focus();
  });

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape" && !busy) {
      e.preventDefault();
      onCancel();
      return;
    }
    if (e.key !== "Tab" || !dialog) return;

    const focusable = Array.from(
      dialog.querySelectorAll<HTMLElement>("button:not([disabled])"),
    );
    if (focusable.length === 0) return;
    const first = focusable[0]!;
    const last = focusable[focusable.length - 1]!;
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if open}
  <div
    class="scrim"
    role="presentation"
    onclick={(e) => {
      if (!busy && e.target === e.currentTarget) onCancel();
    }}
  >
    <div
      bind:this={dialog}
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby={`${dialogId}-title`}
      aria-describedby={`${dialogId}-message`}
    >
      <h3 id={`${dialogId}-title`}>{title}</h3>
      <p id={`${dialogId}-message`}>{message}</p>
      <div class="actions">
        <button
          bind:this={cancelButton}
          class="btn ghost"
          type="button"
          disabled={busy}
          onclick={onCancel}
        >
          {cancelLabel}
        </button>
        <button
          class="btn"
          class:danger
          type="button"
          disabled={busy}
          onclick={onConfirm}
        >
          {busy ? "Working…" : confirmLabel}
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
    display: grid;
    place-items: center;
    animation: fade 0.14s ease;
  }

  .dialog {
    width: min(420px, calc(100vw - 32px));
    padding: 22px 22px 18px;
    border-radius: var(--radius-lg);
    background: linear-gradient(160deg, var(--bg-panel), var(--bg-elevated));
    border: 1px solid var(--border-strong);
    box-shadow:
      var(--shadow-lg),
      0 0 40px -12px var(--violet-glow);
    color: var(--text-hi);
    animation: rise 0.16s var(--ease, cubic-bezier(.2,.8,.2,1));
  }

  h3 {
    margin: 0 0 6px;
    font-size: var(--text-lg);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  p {
    margin: 0 0 18px;
    font-size: var(--text-sm);
    line-height: 1.5;
    color: var(--text-mid);
  }

  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .btn {
    padding: 8px 14px;
    font-size: var(--text-sm);
    border-radius: var(--radius-pill);
    border: 1px solid transparent;
    background: var(--violet-500);
    color: white;
    cursor: pointer;
    transition: transform var(--dur) var(--ease),
                box-shadow var(--dur) var(--ease),
                background var(--dur) var(--ease);
  }

  .btn:hover:not(:disabled) {
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

  .btn.danger {
    background: linear-gradient(160deg, #c8334a, #8e1f33);
    box-shadow: 0 0 0 1px rgba(255, 138, 138, 0.3) inset;
  }

  .btn.danger:hover:not(:disabled) {
    box-shadow:
      0 0 0 1px rgba(255, 138, 138, 0.5) inset,
      0 8px 20px -8px rgba(200, 51, 74, 0.7);
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
