<script lang="ts">
  import { onMount } from "svelte";
  import { ipc } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";
  import { papers } from "$lib/stores/papers";
  import { viewport } from "$lib/stores/viewport";
  import { drag } from "$lib/stores/drag";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();
  let busy = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let currentFile = $state("");
  let currentIndex = $state(0);
  let totalFiles = $state(0);
  let unsubscribeDrop: (() => void) | null = null;
  let pickerUnavailable = $state(false);

  const hovering = $derived($drag.hovering);
  const activityMessage = $derived(
    busy
      ? `Importing ${currentIndex} of ${totalFiles}: ${currentFile}`
      : notice,
  );

  function fileName(path: string): string {
    return path.split(/[\\/]/).pop() || "paper.pdf";
  }

  function readableError(value: unknown): string {
    const message = value instanceof Error ? value.message : String(value);
    return message.replace(/^Error:\s*/i, "") || "Unknown import error";
  }

  async function ingest(paths: string[]) {
    if (!paths.length || busy) return;

    const pdfs = paths.filter((path) => path.toLowerCase().endsWith(".pdf"));
    const rejected = paths.filter((path) => !path.toLowerCase().endsWith(".pdf"));

    if (!pdfs.length) {
      error = rejected.length === 1
        ? `"${fileName(rejected[0])}" is not a PDF. Choose a .pdf file.`
        : `${rejected.length} files were skipped because they are not PDFs.`;
      notice = null;
      return;
    }

    busy = true;
    error = null;
    notice = null;
    totalFiles = pdfs.length;
    currentIndex = 0;
    currentFile = "";

    const failures: string[] = [];
    let imported = 0;

    try {
      for (const [index, path] of pdfs.entries()) {
        currentIndex = index + 1;
        currentFile = fileName(path);
        try {
          await ipc.ingestPdf(path);
          imported += 1;
        } catch (cause) {
          failures.push(`${currentFile}: ${readableError(cause)}`);
        }
      }

      if (imported > 0) {
        await papers.refresh();
        notice = imported === 1
          ? "Paper imported. Generation is continuing in the background."
          : `${imported} papers imported. Generation is continuing in the background.`;
      }

      const issues: string[] = [];
      if (rejected.length > 0) {
        issues.push(
          `${rejected.length} non-PDF ${rejected.length === 1 ? "file was" : "files were"} skipped.`,
        );
      }
      if (failures.length > 0) {
        issues.push(`Could not import ${failures.join("; ")}.`);
      }
      error = issues.length > 0 ? issues.join(" ") : null;
    } finally {
      busy = false;
      currentFile = "";
      currentIndex = 0;
      totalFiles = 0;
    }
  }

  async function pickFile() {
    if (busy) return;
    error = null;
    notice = null;

    if (!isTauri()) {
      pickerUnavailable = true;
      return;
    }
    try {
      const mod = await import("@tauri-apps/plugin-dialog");
      const picked = await mod.open({
        multiple: true,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      await ingest(paths);
    } catch (cause) {
      error = `Could not open the PDF picker: ${readableError(cause)}.`;
    }
  }

  onMount(() => {
    void drag.init();
    unsubscribeDrop = drag.onDrop((paths) => void ingest(paths));
    return () => {
      unsubscribeDrop?.();
      unsubscribeDrop = null;
    };
  });
</script>

<div class="drop-shell">
  <button
    class="dropzone shimmer-sweep"
    class:hovering
    class:busy
    class:compact
    onclick={pickFile}
    type="button"
    disabled={busy}
    aria-busy={busy}
    aria-label={busy
      ? `Importing ${currentIndex} of ${totalFiles}`
      : "Choose one or more scientific PDF files"}
    aria-describedby="dropzone-help"
  >
    <div class="glow" aria-hidden="true"></div>
    <div class="scan" aria-hidden="true"></div>
    <div class="inner">
      <div class="icon" aria-hidden="true">
        {#if busy}
          <span class="upload-spinner"></span>
        {:else}
          <svg viewBox="0 0 48 48" fill="none" width="44" height="44">
            <path
              d="M24 6v24m0 0-8-8m8 8 8-8"
              stroke="currentColor"
              stroke-width="2.2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
            <path
              d="M8 34v4a4 4 0 0 0 4 4h24a4 4 0 0 0 4-4v-4"
              stroke="currentColor"
              stroke-width="2.2"
              stroke-linecap="round"
            />
          </svg>
        {/if}
      </div>
      <div class="text">
        <div class="headline">
          {#if busy}
            Importing {currentIndex} of {totalFiles}
          {:else if hovering}
            Release to start pipeline
          {:else if $viewport.coarsePointer}
            Choose scientific PDFs
          {:else}
            Drop scientific PDFs
          {/if}
        </div>
        <div class="sub" id="dropzone-help">
          {#if busy}
            {currentFile}
          {:else if pickerUnavailable}
            Open the Tauri app to drop files · browser dev preview only
          {:else if $viewport.coarsePointer}
            Select one or more PDFs · pages stay available offline
          {:else}
            Vision structure → AI plan → interactive pages → permanent save
          {/if}
        </div>
        <div class="micro-flow" aria-hidden="true">
          <span>PDF</span>
          <i></i>
          <span>OCR</span>
          <i></i>
          <span>Pages</span>
        </div>
      </div>
    </div>
  </button>

  <div class="drop-feedback" aria-atomic="true">
    {#if activityMessage}
      <div class="notice" role="status" aria-live="polite">{activityMessage}</div>
    {/if}
    {#if error}
      <div class="error" role="alert">{error}</div>
    {/if}
  </div>
</div>

<style>
  .drop-shell {
    width: min(440px, 100%);
  }

  .dropzone {
    position: relative;
    display: block;
    width: 100%;
    min-height: 254px;
    padding: 28px;
    border-radius: var(--radius-xl);
    color: var(--text-hi);
    background:
      radial-gradient(
        800px circle at calc(var(--mx) * 1%) calc(var(--my) * 1%),
        rgba(154, 107, 255, 0.28),
        transparent 55%
      ),
      linear-gradient(115deg, rgba(255, 255, 255, 0.09), transparent 24%),
      linear-gradient(135deg, #05040a, #151023 54%, #05040a);
    border: 1px solid var(--border-strong);
    box-shadow:
      var(--shadow-deep),
      inset 0 1px 0 rgba(255, 255, 255, 0.08);
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    transition:
      transform var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease),
      border-color var(--dur) var(--ease);
  }

  .dropzone:hover {
    transform: translateY(-1px);
    box-shadow:
      var(--shadow-deep),
      var(--shadow-glow);
  }

  .dropzone.hovering {
    transform: scale(1.01);
    border-color: var(--violet-400);
    box-shadow:
      0 30px 90px -20px rgba(154, 107, 255, 0.7),
      0 0 0 2px rgba(154, 107, 255, 0.45) inset;
  }

  .dropzone.busy {
    opacity: 0.85;
    cursor: progress;
  }

  .dropzone.compact {
    min-height: 184px;
    padding: 20px 22px;
  }

  .dropzone.compact .inner {
    min-height: 138px;
  }

  .dropzone.compact .icon {
    width: 60px;
    height: 60px;
  }

  .dropzone:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 4px;
  }

  .dropzone:disabled {
    cursor: progress;
  }

  .glow {
    position: absolute;
    inset: -40%;
    background: radial-gradient(
      circle at 50% 50%,
      rgba(154, 107, 255, 0.18),
      transparent 60%
    );
    pointer-events: none;
    filter: blur(30px);
    opacity: 0.55;
  }

  .scan {
    position: absolute;
    inset: auto -20% 0 -20%;
    height: 44%;
    background:
      repeating-linear-gradient(
        0deg,
        rgba(79, 240, 255, 0.045) 0 1px,
        transparent 1px 7px
      ),
      linear-gradient(180deg, transparent, rgba(79, 240, 255, 0.08));
    transform: perspective(420px) rotateX(58deg);
    transform-origin: bottom;
    opacity: 0.7;
    pointer-events: none;
  }

  .inner {
    position: relative;
    display: grid;
    grid-template-columns: 78px 1fr;
    align-items: center;
    gap: 18px;
    z-index: 1;
    min-height: 178px;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 72px;
    height: 72px;
    border-radius: var(--radius-md);
    color: var(--violet-100);
    background:
      radial-gradient(
        circle at 30% 20%,
        rgba(255, 255, 255, 0.12),
        transparent 60%
      ),
      linear-gradient(135deg, #321b6b, #090415);
    border: 1px solid var(--border-strong);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.12),
      0 10px 30px -10px rgba(106, 45, 255, 0.6);
    flex: none;
  }

  .upload-spinner {
    width: 30px;
    height: 30px;
    border: 2px solid rgba(200, 168, 255, 0.25);
    border-top-color: var(--cyan-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .headline {
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--text-hi);
  }

  .sub {
    font-size: var(--text-sm);
    color: var(--text-mid);
    line-height: 1.45;
  }

  .micro-flow {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 8px;
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .micro-flow span {
    padding: 3px 6px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
    background: rgba(255, 255, 255, 0.035);
  }

  .micro-flow i {
    width: 18px;
    height: 1px;
    background: linear-gradient(90deg, var(--violet-400), var(--cyan-accent));
    opacity: 0.8;
  }

  .drop-feedback {
    display: grid;
    gap: 8px;
    margin-top: 10px;
  }

  .notice,
  .error {
    position: relative;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    line-height: 1.45;
  }

  .notice {
    background: rgba(79, 240, 255, 0.07);
    color: var(--cyan-glow);
    border: 1px solid rgba(79, 240, 255, 0.25);
  }

  .error {
    background: rgba(255, 107, 123, 0.08);
    color: var(--status-err);
    border: 1px solid rgba(255, 107, 123, 0.35);
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (max-width: 520px) {
    .dropzone {
      min-height: 224px;
      padding: 20px;
    }

    .inner {
      grid-template-columns: 1fr;
      justify-items: start;
      min-height: 0;
    }

    .icon {
      width: 58px;
      height: 58px;
    }
  }

  @media (hover: none), (pointer: coarse) {
    .dropzone:hover {
      transform: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dropzone {
      transition: none;
    }

    .dropzone:hover,
    .dropzone.hovering {
      transform: none;
    }

    .upload-spinner {
      animation-duration: 1.6s;
    }
  }
</style>
