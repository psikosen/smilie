<script lang="ts">
  import { jobs, type LiveJob } from "$lib/stores/jobs";
  import { ipc } from "$lib/ipc";
  import { toast } from "$lib/stores/toast";

  interface Props {
    paperId: string;
  }
  let { paperId }: Props = $props();

  const STAGES = [
    "import_pdf",
    "render_pages",
    "ocr_extract",
    "structure_paper",
    "build_learning_plan",
    "generate_pages",
    "validate_pages",
    "save_pages",
    "index_pages",
    "complete",
  ] as const;

  const STAGE_LABELS: Record<(typeof STAGES)[number], string> = {
    import_pdf: "Import PDF",
    render_pages: "Render pages",
    ocr_extract: "Vision OCR",
    structure_paper: "Structure paper",
    build_learning_plan: "Build learning plan",
    generate_pages: "Generate pages",
    validate_pages: "Validate pages",
    save_pages: "Save permanently",
    index_pages: "Index for search",
    complete: "Learning system ready",
  };

  const STAGE_DETAILS: Record<(typeof STAGES)[number], string> = {
    import_pdf: "Capturing the source PDF into local storage.",
    render_pages: "Preparing page images for document vision.",
    ocr_extract: "Extracting text, layout, tables, equations, and figures.",
    structure_paper: "Normalizing the paper into durable structured data.",
    build_learning_plan: "Turning sections into a page dependency plan.",
    generate_pages: "Writing explanations, quizzes, explorers, and simulations.",
    validate_pages: "Checking generated pages before they are marked ready.",
    save_pages: "Persisting page records and local HTML assets.",
    index_pages: "Making saved pages searchable.",
    complete: "The generated learning system is available offline.",
  };

  const job = $derived<LiveJob | undefined>($jobs.byPaper[paperId]);
  const currentIndex = $derived.by(() => {
    if (!job) return -1;
    return STAGES.indexOf(job.stage as (typeof STAGES)[number]);
  });
  const pct = $derived(
    job ? Math.min(100, Math.max(0, Math.round((job.progress ?? 0) * 100))) : 0,
  );

  let dismissed = $state(false);
  let resuming = $state(false);
  let cancelling = $state(false);
  let lastJobId = $state<string | null>(null);
  const visible = $derived(!!job && !dismissed && job.stage !== "complete");
  const canResume = $derived(
    !!job && (job.status === "failed" || job.status === "paused"),
  );
  const canCancel = $derived(
    !!job && (job.status === "running" || job.status === "paused"),
  );
  const title = $derived.by(() => {
    if (!job) return "";
    if (job.status === "failed") return "Pipeline needs attention";
    if (job.status === "paused") return "Pipeline paused";
    return "Building learning system";
  });

  $effect(() => {
    const nextJobId = job?.id ?? null;
    if (nextJobId && nextJobId !== lastJobId) {
      lastJobId = nextJobId;
      dismissed = false;
    }
    if (job?.status === "failed") dismissed = false;
  });

  async function cancel() {
    if (!job || cancelling) return;
    cancelling = true;
    try {
      await ipc.cancelJob(job.id);
      // Hide the panel immediately — the orchestrator will exit at
      // the next stage checkpoint, and any further events for this
      // job_id are uninteresting once the user has dismissed it.
      dismissed = true;
    } catch (e) {
      console.error("cancel failed", e);
      toast.error(`Could not cancel pipeline: ${String(e)}`);
    } finally {
      cancelling = false;
    }
  }

  async function resume() {
    if (!job || resuming) return;
    resuming = true;
    try {
      await ipc.resumeJob(job.id);
    } catch (e) {
      console.error("resume failed", e);
      toast.error(`Could not resume pipeline: ${String(e)}`);
    } finally {
      resuming = false;
    }
  }

  function prettyStage(s: string): string {
    return STAGE_LABELS[s as (typeof STAGES)[number]] ?? s.replace(/_/g, " ");
  }

  function detailForStage(s: string): string {
    return STAGE_DETAILS[s as (typeof STAGES)[number]] ?? "";
  }
</script>

{#if visible && job}
  <section
    class="panel surface-glass edge-lit fade-rise"
    class:failed={job.status === "failed"}
    aria-labelledby="pipeline-progress-title"
  >
    <p class="sr-status" role="status" aria-live="polite" aria-atomic="true">
      {title}. {prettyStage(job.stage)}. {pct}% complete.
    </p>
    <header>
      <h2 class="title" id="pipeline-progress-title">
        <span class="pulse" aria-hidden="true"></span>
        {title}
      </h2>
      <button class="close" type="button" onclick={() => (dismissed = true)} aria-label="Hide progress">
        ×
      </button>
    </header>

    <div class="progress-copy">
      <div class="current">{prettyStage(job.stage)}</div>
      <p>{detailForStage(job.stage)}</p>
      <progress
        class="progress-track"
        value={pct}
        max="100"
        aria-label={`${prettyStage(job.stage)} progress`}
      >{pct}%</progress>
    </div>

    <ol class="stages">
      {#each STAGES as stage, i (stage)}
        <li
          class="stage"
          class:done={i < currentIndex}
          class:active={i === currentIndex}
          aria-current={i === currentIndex ? "step" : undefined}
        >
          <span class="dot" aria-hidden="true"></span>
          <span class="name">{prettyStage(stage)}</span>
          {#if i === currentIndex}
            <span class="pct">{pct}%</span>
          {/if}
        </li>
      {/each}
    </ol>

    {#if job.error}
      <div class="error" role="alert">{job.error}</div>
    {/if}

    <footer>
      {#if canResume}
        <button
          class="btn primary"
          type="button"
          disabled={resuming}
          onclick={resume}
        >
          {resuming ? "Resuming…" : "Resume"}
        </button>
      {/if}
      {#if canCancel}
        <button class="btn" type="button" disabled={cancelling} onclick={cancel}>
          {cancelling ? "Cancelling…" : "Cancel"}
        </button>
      {/if}
    </footer>
  </section>
{/if}

<style>
  .panel {
    position: absolute;
    right: 20px;
    bottom: 20px;
    width: 360px;
    padding: 14px;
    border-radius: var(--radius-lg);
    z-index: var(--z-overlay);
    color: var(--text-hi);
    max-height: calc(100dvh - 96px);
    overflow-y: auto;
  }

  .panel.failed {
    border-color: rgba(255, 107, 123, 0.38);
    box-shadow: 0 18px 50px -28px rgba(255, 107, 123, 0.55);
  }

  .sr-status {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 10px;
  }

  .title {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-sm);
    font-weight: 600;
    letter-spacing: 0;
  }

  .pulse {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--violet-400);
    box-shadow: 0 0 12px var(--violet-glow);
    animation: pulse 1.4s ease-in-out infinite;
  }

  .panel.failed .pulse {
    background: var(--status-err);
    box-shadow: 0 0 12px rgba(255, 107, 123, 0.45);
    animation: none;
  }

  @keyframes pulse {
    0%, 100% { transform: scale(0.85); opacity: 0.7; }
    50% { transform: scale(1.1); opacity: 1; }
  }

  .close {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    color: var(--text-dim);
    font-size: 16px;
    line-height: 1;
    transition: color var(--dur) var(--ease), background var(--dur) var(--ease);
  }

  .close:hover {
    color: var(--text-hi);
    background: rgba(255, 255, 255, 0.05);
  }

  .progress-copy {
    margin-bottom: 12px;
    padding: 10px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background:
      linear-gradient(115deg, rgba(255, 255, 255, 0.045), transparent 32%),
      rgba(255, 255, 255, 0.025);
  }

  .current {
    color: var(--text-hi);
    font-size: var(--text-sm);
    font-weight: 650;
  }

  .progress-copy p {
    margin: 4px 0 9px;
    color: var(--text-mid);
    font-size: var(--text-xs);
    line-height: 1.45;
  }

  .progress-track {
    display: block;
    width: 100%;
    height: 5px;
    appearance: none;
    -webkit-appearance: none;
    border: 0;
    border-radius: var(--radius-pill);
    overflow: hidden;
    background: rgba(154, 107, 255, 0.12);
  }

  .progress-track::-webkit-progress-bar {
    background: rgba(154, 107, 255, 0.12);
    border-radius: inherit;
  }

  .progress-track::-webkit-progress-value {
    background: linear-gradient(90deg, var(--violet-500), var(--cyan-accent));
    border-radius: inherit;
    box-shadow: 0 0 16px rgba(154, 107, 255, 0.55);
    transition: width var(--dur-slow) var(--ease);
  }

  .progress-track::-moz-progress-bar {
    background: linear-gradient(90deg, var(--violet-500), var(--cyan-accent));
    border-radius: inherit;
  }

  .stages {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .stage {
    display: grid;
    grid-template-columns: 16px 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    color: var(--text-dim);
  }

  .stage.done {
    color: var(--text-mid);
  }

  .stage.active {
    color: var(--text-hi);
    background: rgba(154, 107, 255, 0.08);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-ghost);
    justify-self: center;
  }

  .stage.done .dot { background: var(--status-ok); box-shadow: 0 0 8px rgba(93, 255, 160, 0.4); }
  .stage.active .dot {
    background: var(--violet-400);
    box-shadow: 0 0 10px var(--violet-glow);
    animation: pulse 1.4s ease-in-out infinite;
  }

  .name {
    text-transform: none;
  }

  .pct {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--violet-200);
  }

  .error {
    margin-bottom: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: rgba(255, 107, 123, 0.08);
    border: 1px solid rgba(255, 107, 123, 0.25);
    color: var(--status-err);
    font-size: var(--text-xs);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .btn {
    min-height: 38px;
    padding: 6px 12px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    color: var(--text-mid);
    font-size: var(--text-xs);
    background: var(--bg-elevated);
    transition: color var(--dur) var(--ease), border-color var(--dur) var(--ease);
  }

  .btn:hover {
    color: var(--violet-100);
    border-color: var(--border-strong);
  }

  .btn.primary {
    background: var(--violet-500);
    color: white;
    border-color: transparent;
  }

  .btn.primary:hover:not(:disabled) {
    box-shadow: 0 6px 16px -8px rgba(106, 45, 255, 0.7);
    color: white;
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: progress;
  }

  .btn:focus-visible,
  .close:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 2px;
  }

  @media (prefers-reduced-motion: reduce) {
    .pulse,
    .stage.active .dot {
      animation: none;
    }
  }

  @media (max-width: 520px) {
    .panel {
      left: 14px;
      right: 14px;
      bottom: max(78px, env(safe-area-inset-bottom));
      width: auto;
      max-height: calc(100dvh - 112px);
    }

    .btn {
      min-height: 44px;
    }
  }
</style>
