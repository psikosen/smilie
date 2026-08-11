<script module lang="ts">
  export function createPdfRequestSequence() {
    let current = 0;
    return {
      next: () => ++current,
      isCurrent: (request: number) => request === current,
    };
  }
</script>

<script lang="ts">
  /**
   * PDF tab — renders the original PDF for a paper.
   *
   * macOS WKWebView (Tauri's webview) doesn't render PDFs inside an
   * `<iframe>` reliably — the frame stays blank. `<embed>` invokes the
   * platform's PDF plugin path and works on Chromium-based webviews;
   * on WKWebView results vary by macOS version. We render `<embed>`
   * inside `<object>` with an HTML fallback so the user always sees
   * either the inline preview OR a clear "Open in default app" CTA
   * pointing at /usr/bin/open. PDF.js vendoring is the bulletproof
   * follow-up if inline preview turns out to need full reliability.
   *
   * Tauri-only resolution; in plain-browser dev mode we surface an
   * empty state so the route still renders without IPC.
   */
  import { ipc, type PaperId } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { toast } from "$lib/stores/toast";

  interface Props {
    paperId: PaperId;
  }
  let { paperId }: Props = $props();

  let url = $state<string | null>(null);
  let absolutePath = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let openingInApp = $state(false);
  const pdfRequests = createPdfRequestSequence();

  // Route navigation can reuse this component. A request token prevents
  // an older paper from replacing a newer PDF if responses arrive out of order.
  $effect(() => {
    const targetPaperId = paperId;
    void resolve(targetPaperId);
    return () => {
      pdfRequests.next();
    };
  });

  async function resolve(targetPaperId = paperId) {
    const request = pdfRequests.next();
    loading = true;
    error = null;
    url = null;
    absolutePath = null;
    if (!isTauri()) {
      if (pdfRequests.isCurrent(request)) loading = false;
      return;
    }
    try {
      const path = await ipc.getPaperPdfUrl(targetPaperId);
      if (!pdfRequests.isCurrent(request)) return;
      absolutePath = path;
      url = convertFileSrc(path, "asset");
    } catch (err) {
      if (!pdfRequests.isCurrent(request)) return;
      error = String(err);
    } finally {
      if (pdfRequests.isCurrent(request)) loading = false;
    }
  }

  async function openInDefaultApp() {
    if (openingInApp) return;
    if (!isTauri()) return;
    openingInApp = true;
    try {
      // Dedicated Rust command — bypasses the deprecated
      // tauri-plugin-shell `open` and the JS-side wrapper that was
      // failing silently. Shells out to /usr/bin/open on macOS,
      // xdg-open on Linux, `cmd /C start` on Windows.
      await ipc.openPaperInDefaultApp(paperId);
    } catch (err) {
      toast.error(`Could not open PDF: ${String(err)}`);
    } finally {
      openingInApp = false;
    }
  }
</script>

<section class="pdf-tab">
  <header class="chrome">
    <div class="title-wrap">
      <h1 class="title">Original PDF</h1>
      <span class="local-badge">Local source</span>
    </div>
    <div class="actions">
      <button
        class="chip"
        type="button"
        onclick={openInDefaultApp}
        disabled={!absolutePath || openingInApp || !isTauri()}
        aria-label="Open the PDF in the default app"
      >
        {openingInApp ? "Opening…" : "↗ Open in default app"}
      </button>
      <span class="open-status" role="status" aria-live="polite">
        {openingInApp ? "Opening PDF in the default app." : ""}
      </span>
    </div>
  </header>

  <div class="stage" aria-busy={loading}>
    {#if loading}
      <div class="empty" role="status" aria-live="polite">
        <div class="empty-title">Loading local source</div>
        <p>Resolving the permanently saved paper PDF…</p>
      </div>
    {:else if error}
      <div class="empty err" role="alert">
        <div class="empty-title">Could not load the PDF</div>
        <p>{error}</p>
        <button type="button" class="chip" onclick={() => resolve(paperId)}>Retry</button>
      </div>
    {:else if url}
      {#key url}
        <!--
          `<object>` lets WKWebView delegate PDF rendering to the
          platform plugin instead of showing a blank `<iframe>`. The
          inner fallback markup is what the user sees if rendering
          fails — clear hand-off to the system viewer.
        -->
        <object
          title="Original PDF"
          data={url}
          type="application/pdf"
          class="pdf-embed"
        >
          <div class="empty">
            <div class="empty-title">Inline preview not available</div>
            <p>
              This webview can't render PDFs inline. Open the file in your
              default viewer instead.
            </p>
            <button
              type="button"
              class="chip primary"
              onclick={openInDefaultApp}
              disabled={openingInApp}
            >
              {openingInApp ? "Opening…" : "↗ Open in default app"}
            </button>
          </div>
        </object>
      {/key}
    {:else}
      <div class="empty">
        <div class="empty-title">PDF preview unavailable</div>
        <p>Open the desktop app to view the original PDF here.</p>
      </div>
    {/if}
  </div>
</section>

<style>
  .pdf-tab {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-void);
  }

  .chrome {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    background:
      linear-gradient(115deg, rgba(255, 255, 255, 0.04), transparent 28%),
      rgba(9, 8, 18, 0.96);
  }

  .title-wrap {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .title {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-hi);
    min-width: 0;
  }

  .local-badge {
    padding: 3px 7px;
    border: 1px solid rgba(93, 255, 160, 0.22);
    border-radius: var(--radius-sm);
    color: var(--status-ok);
    background: rgba(93, 255, 160, 0.055);
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
  }

  .actions {
    display: flex;
    gap: 6px;
    flex: none;
  }

  .open-status {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
  }

  .chip {
    min-height: 36px;
    padding: 6px 10px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: var(--bg-elevated);
    color: var(--text-mid);
    font-size: var(--text-xs);
    cursor: pointer;
    transition:
      color var(--dur) var(--ease),
      background var(--dur) var(--ease),
      border-color var(--dur) var(--ease);
  }
  .chip:hover:not(:disabled) {
    color: var(--violet-100);
    border-color: var(--border-strong);
  }
  .chip:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .stage .pdf-embed {
    flex: 1;
    width: 100%;
    height: 100%;
    border: 0;
    background: var(--bg-void);
  }

  .chip.primary {
    background: var(--violet-500);
    color: white;
    border-color: transparent;
  }
  .chip.primary:hover:not(:disabled) {
    box-shadow: 0 6px 16px -8px rgba(106, 45, 255, 0.6);
  }

  .empty {
    margin: auto;
    max-width: 440px;
    padding: 32px;
    text-align: center;
    color: var(--text-mid);
  }
  .empty.err { color: var(--status-err); }
  .empty-title {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text-hi);
    margin-bottom: 8px;
  }
  .empty p {
    margin: 0 0 12px;
    font-size: var(--text-sm);
    line-height: 1.55;
  }

  .chip:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 2px;
  }

  @media (max-width: 560px) {
    .chrome {
      align-items: stretch;
      flex-direction: column;
      padding: 10px;
    }

    .actions {
      width: 100%;
    }

    .chip {
      min-height: 44px;
    }

    .empty {
      padding: 24px 18px;
    }
  }
</style>
