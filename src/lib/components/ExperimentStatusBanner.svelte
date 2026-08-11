<script lang="ts">
  /**
   * Status banner above the iframe when an experiment is in any
   * non-`ready` state.
   *
   * For the three in-flight phases (`analyzing` / `generating` /
   * `testing`) we show a per-stage time-based progress bar (elapsed
   * vs. typical), a row of three stage dots so the user sees where
   * they are in the flow, and an elapsed-seconds ticker driven from
   * `created_at`. mlx-vlm doesn't expose token-level progress, so
   * "elapsed/expected" is our honest approximation: the bar caps at
   * 95% if a stage runs long and snaps to 100% on completion. Better
   * than indeterminate because the user gets a real time sense.
   *
   * `failed` is red with the test-output excerpt; `implausible` is
   * dashed-soft with the score + reasoning.
   */
  import { onDestroy } from "svelte";
  import type { ExperimentDetail, ExperimentStatus } from "$lib/ipc";

  interface Props {
    detail: ExperimentDetail;
    onRefine: () => void;
    /** "Try anyway" override — only fired when implausible + score ≥ 50. */
    onForceTry: () => void;
    /** Disables the override button while a force-try call is in flight. */
    forceTryBusy?: boolean;
  }

  let { detail, onRefine, onForceTry, forceTryBusy = false }: Props = $props();

  /** Borderline-implausible papers (score ≥ 50 but `plausible: false`)
   *  get an "I disagree, try anyway" escape hatch. Below 50 we hide the
   *  button — the model has high confidence the experiment won't work
   *  and forcing it through wastes a generate pass. */
  const canForceTry = $derived(
    detail.status === "implausible" &&
      (detail.plausibility_score ?? 0) >= 50,
  );

  const STAGE_ORDER: ExperimentStatus[] = ["analyzing", "generating", "testing"];
  const STAGE_LABEL: Record<ExperimentStatus, string> = {
    analyzing:    "Analyzing reproducibility",
    generating:   "Writing experiment",
    testing:      "Verifying with tests",
    ready:        "Ready",
    failed:       "Failed",
    implausible:  "Implausible",
  };

  /** Typical wall-clock duration per stage on Apple-Silicon mlx-vlm
   *  with the trimmed analyze prompt (~5-10k chars input). The bar
   *  shows elapsed / expected, clamped to 95% if a stage runs long.
   *  Tweak these as we collect more telemetry; defaults are
   *  intentionally conservative so the bar doesn't sit at 95% the
   *  whole time. */
  const STAGE_EXPECTED_SECS: Record<string, number> = {
    analyzing:  20,
    generating: 45,
    testing:    30,
  };

  const inFlight = $derived(STAGE_ORDER.includes(detail.status));
  const currentStageIndex = $derived(
    STAGE_ORDER.indexOf(detail.status as ExperimentStatus),
  );

  // Elapsed-seconds ticker. Re-baselines whenever `created_at` changes
  // (i.e. when the user opens a different experiment).
  let nowMs = $state(Date.now());
  let timer: ReturnType<typeof setInterval> | null = null;

  $effect(() => {
    if (inFlight) {
      // Force re-read of `nowMs` reactivity on every tick so the ticker
      // re-renders without the rest of the parent re-running.
      nowMs = Date.now();
      timer = setInterval(() => (nowMs = Date.now()), 1000);
    } else if (timer) {
      clearInterval(timer);
      timer = null;
    }
    return () => {
      if (timer) clearInterval(timer);
      timer = null;
    };
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  const elapsedSecs = $derived(
    Math.max(0, Math.floor(nowMs / 1000) - detail.created_at),
  );

  /** Seconds since the current stage started — `updated_at` advances
   *  on every status transition, so this resets when stage changes. */
  const stageElapsedSecs = $derived(
    Math.max(0, Math.floor(nowMs / 1000) - detail.updated_at),
  );

  /** 0..1 progress for the bar. Caps at 0.95 while in-flight so the
   *  user can see we haven't actually finished even if the stage runs
   *  longer than typical. */
  const stageProgress = $derived.by(() => {
    if (!inFlight) return 0;
    const expected = STAGE_EXPECTED_SECS[detail.status] ?? 30;
    const ratio = stageElapsedSecs / expected;
    return Math.min(0.95, Math.max(0.02, ratio));
  });

  function fmtElapsed(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  const view = $derived.by(() => {
    switch (detail.status) {
      case "analyzing":
      case "generating":
      case "testing":
        return {
          kind: "info" as const,
          title: `${STAGE_LABEL[detail.status]}…`,
          body: "",
        };
      case "failed":
        return {
          kind: "error" as const,
          title: "Generation failed",
          body:
            detail.test_last_output ??
            "The model could not produce a working experiment.",
        };
      case "implausible":
        return {
          kind: "soft" as const,
          title: `Plausibility too low (${detail.plausibility_score ?? 0}/100)`,
          body: detail.plausibility_reason ?? "",
        };
      default:
        return null;
    }
  });
</script>

{#if view}
  <div class="banner banner-{view.kind}" role="status" aria-live="polite">
    <div class="banner-head">
      <div class="banner-title">{view.title}</div>
      {#if inFlight}
        <div class="banner-elapsed" aria-label="Elapsed time">
          {fmtElapsed(elapsedSecs)}
        </div>
      {/if}
    </div>

    {#if inFlight}
      <ol class="stages" aria-label="Pipeline stage">
        {#each STAGE_ORDER as stage, i (stage)}
          <li
            class="stage"
            class:done={i < currentStageIndex}
            class:active={i === currentStageIndex}
          >
            <span class="dot" aria-hidden="true"></span>
            <span class="stage-label">{STAGE_LABEL[stage]}</span>
          </li>
        {/each}
      </ol>
      <div
        class="bar"
        role="progressbar"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={Math.round(stageProgress * 100)}
      >
        <div class="bar-track"></div>
        <div
          class="bar-fill"
          style="width: {(stageProgress * 100).toFixed(1)}%"
        ></div>
      </div>
    {/if}

    {#if view.body}
      <pre class="banner-body">{view.body}</pre>
    {/if}
    {#if view.kind === "error"}
      <button type="button" class="banner-cta" onclick={onRefine}>
        Refine with prompt
      </button>
    {/if}
    {#if canForceTry}
      <div class="cta-row">
        <button
          type="button"
          class="banner-cta force"
          onclick={onForceTry}
          disabled={forceTryBusy}
          title="Skip the analyze gate and try generating the experiment anyway"
        >
          {forceTryBusy ? "Starting…" : "Try anyway →"}
        </button>
        <span class="cta-hint">
          The model said no, but the score (≥ 50) is borderline. Override at
          your discretion.
        </span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .banner {
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

  .banner-elapsed {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--violet-200);
    padding: 1px 8px;
    border-radius: var(--radius-pill);
    background: rgba(154, 107, 255, 0.12);
    border: 1px solid var(--border);
    /* Tabular-num so the ticker doesn't shimmy as digits change. */
    font-variant-numeric: tabular-nums;
  }

  /* Stage dots */
  .stages {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    gap: 18px;
  }
  .stage {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-xs);
    color: var(--text-dim);
  }
  .stage.done {
    color: var(--text-mid);
  }
  .stage.active {
    color: var(--text-hi);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-ghost);
  }
  .stage.done .dot {
    background: var(--violet-300);
  }
  .stage.active .dot {
    background: var(--violet-400);
    box-shadow: 0 0 12px var(--violet-glow);
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { transform: scale(0.85); opacity: 0.7; }
    50%      { transform: scale(1.15); opacity: 1; }
  }

  /* Determinate progress bar — width animates as the stage's elapsed
     time approaches the typical duration. */
  .bar {
    position: relative;
    height: 4px;
    border-radius: var(--radius-pill);
    overflow: hidden;
  }
  .bar-track {
    position: absolute;
    inset: 0;
    background: rgba(154, 107, 255, 0.10);
  }
  .bar-fill {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    background: linear-gradient(
      90deg,
      var(--violet-500) 0%,
      var(--violet-300) 100%
    );
    box-shadow: 0 0 10px var(--violet-glow);
    /* Smoothly animate width changes so the bar doesn't tick stuttery
       as the second-resolution timer updates `stageProgress`. */
    transition: width var(--dur-slow) linear;
    border-radius: var(--radius-pill);
  }

  @media (prefers-reduced-motion: reduce) {
    .bar-fill { transition: none; }
    .stage.active .dot { animation: none; }
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

  .banner-soft {
    border-style: dashed;
    color: var(--text-mid);
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
  .banner-cta:disabled {
    opacity: 0.55;
    cursor: progress;
  }

  .cta-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .banner-cta.force {
    /* Primary-ish — the override is a deliberate user action so we
       give it a touch more visual weight than the default chip. */
    background: linear-gradient(
      160deg,
      rgba(154, 107, 255, 0.18),
      rgba(106, 45, 255, 0.10)
    );
    border-color: var(--violet-400);
    color: var(--text-hi);
  }

  .cta-hint {
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.4;
  }
</style>
