<script lang="ts">
  /**
   * Right-pane renderer for a selected comparison. Mirrors the
   * PageViewer iframe-rendering pattern: the backend writes a regular
   * structured-block HTML file at `html_path`, we just point an iframe
   * at it via `convertFileSrc`.
   *
   * Status banner uses the same animated-sweep + stage-pulse vocabulary
   * as ExperimentStatusBanner so the two screens feel consistent.
   */
  import type { ComparisonDetail } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";
  import { convertFileSrc } from "@tauri-apps/api/core";

  interface Props {
    comparison: ComparisonDetail | null;
    /** Re-trigger the analysis pipeline. Backend is idempotent on the
     *  (a, b) pair, so this serves both "Refine" + "Retry". */
    onRefine: () => void;
    refineBusy?: boolean;
  }
  let { comparison, onRefine, refineBusy = false }: Props = $props();

  const iframeSrc = $derived.by(() => {
    if (!comparison || comparison.status !== "ready" || !comparison.html_path) {
      return null;
    }
    return isTauri() ? convertFileSrc(comparison.html_path, "asset") : null;
  });
</script>

<section class="viewer">
  {#if !comparison}
    <div class="empty">
      <div class="empty-title">Pick a comparison</div>
      <p>
        Select an existing comparison from the list, or click
        <strong>+ New comparison</strong> to start one.
      </p>
    </div>
  {:else}
    <header class="chrome">
      <div class="title" title={comparison.paper_b_title}>
        vs. {comparison.paper_b_title}
      </div>
      <div class="actions">
        {#if comparison.status === "failed"}
          <button
            class="chip violet"
            type="button"
            onclick={onRefine}
            disabled={refineBusy}
          >
            {refineBusy ? "Retrying…" : "↻ Refine"}
          </button>
        {/if}
      </div>
    </header>

    {#if comparison.status === "analyzing"}
      <div class="banner banner-info" role="status" aria-live="polite">
        <div class="banner-head">
          <div class="banner-title">Analyzing comparison…</div>
        </div>
        <div class="bar" role="progressbar" aria-valuetext="working">
          <div class="bar-track"></div>
          <div class="bar-sweep"></div>
        </div>
        <p class="banner-body-text">
          Reading both papers and writing a structured diff. This can take a
          minute or two — the page will update when it lands.
        </p>
      </div>
    {:else if comparison.status === "failed"}
      <div class="banner banner-error" role="alert">
        <div class="banner-head">
          <div class="banner-title">Comparison failed</div>
        </div>
        {#if comparison.error_message}
          <pre class="banner-body">{comparison.error_message}</pre>
        {/if}
        <button
          type="button"
          class="banner-cta"
          onclick={onRefine}
          disabled={refineBusy}
        >
          {refineBusy ? "Retrying…" : "Refine"}
        </button>
      </div>
    {/if}

    <div class="stage">
      {#if iframeSrc}
        {#key iframeSrc}
          <iframe
            title={`Comparison vs. ${comparison.paper_b_title}`}
            src={iframeSrc}
            sandbox="allow-scripts"
            loading="lazy"
            referrerpolicy="no-referrer"
          ></iframe>
        {/key}
      {:else if comparison.status === "ready"}
        <div class="empty">
          <p>
            {#if !isTauri()}
              Preview only available in the desktop app.
            {:else}
              Comparison file missing — try Refine to regenerate.
            {/if}
          </p>
        </div>
      {:else if comparison.status === "analyzing"}
        <div class="empty">
          <p>The diff will render here when analysis completes.</p>
        </div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .viewer {
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
    background: var(--bg-panel);
  }

  .title {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-hi);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .actions {
    display: flex;
    gap: 6px;
    flex: none;
  }

  .chip {
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
  .chip:disabled { opacity: 0.55; cursor: progress; }
  .chip.violet {
    background: linear-gradient(
      160deg,
      rgba(154, 107, 255, 0.18),
      rgba(106, 45, 255, 0.10)
    );
    border-color: var(--violet-400);
    color: var(--text-hi);
  }

  /* Banner shape mirrors ExperimentStatusBanner. */
  .banner {
    margin: 12px 14px 0;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .banner-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .banner-title {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-hi);
  }
  .banner-info {
    border-color: var(--border-strong);
    background: linear-gradient(
      160deg,
      rgba(154, 107, 255, 0.10),
      rgba(106, 45, 255, 0.04)
    );
  }
  .banner-error {
    border-color: rgba(255, 107, 123, 0.4);
    background: rgba(255, 107, 123, 0.08);
  }
  .banner-body {
    margin: 0;
    padding: 6px 8px;
    background: rgba(0, 0, 0, 0.2);
    border-radius: var(--radius-xs);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-mid);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 200px;
    overflow: auto;
  }
  .banner-body-text {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-mid);
    line-height: 1.55;
  }
  .banner-cta {
    align-self: flex-start;
    padding: 4px 10px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border-strong);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-hi);
    font-size: var(--text-xs);
    cursor: pointer;
    transition: background var(--dur) var(--ease), border-color var(--dur) var(--ease);
  }
  .banner-cta:hover:not(:disabled) {
    background: rgba(154, 107, 255, 0.10);
    border-color: var(--violet-300);
  }
  .banner-cta:disabled { opacity: 0.55; cursor: progress; }

  .bar {
    position: relative;
    height: 3px;
    border-radius: var(--radius-pill);
    overflow: hidden;
  }
  .bar-track {
    position: absolute;
    inset: 0;
    background: rgba(154, 107, 255, 0.10);
  }
  .bar-sweep {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 32%;
    background: linear-gradient(
      90deg,
      transparent 0%,
      var(--violet-400) 50%,
      transparent 100%
    );
    box-shadow: 0 0 10px var(--violet-glow);
    animation: sweep 1.6s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite;
  }
  @keyframes sweep {
    0%   { transform: translateX(-120%); }
    100% { transform: translateX(420%); }
  }
  @media (prefers-reduced-motion: reduce) {
    .bar-sweep {
      animation: none;
      opacity: 0.6;
      transform: translateX(0);
      width: 100%;
    }
  }

  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .stage iframe {
    flex: 1;
    border: 0;
    background: var(--bg-void);
  }

  .empty {
    margin: auto;
    max-width: 440px;
    padding: 32px;
    text-align: center;
    color: var(--text-mid);
  }
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
  .empty strong { color: var(--violet-100); }
</style>
