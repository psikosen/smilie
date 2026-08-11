/**
 * Jobs store — mirrors pipeline events so the progress panel can render
 * live stage updates without polling.
 */

import { writable, type Readable } from "svelte/store";
import {
  events,
  type JobSummary,
  type PipelineEventPayload,
  type PipelineStage,
  type JobId,
  type PaperId,
} from "$lib/ipc";
import { isTauri } from "$lib/ipc/tauri-guard";

export interface LiveJob {
  id: JobId;
  paper_id: PaperId;
  stage: PipelineStage;
  status: "running" | "failed" | "completed" | "paused";
  progress: number;
  error: string | null;
}

interface JobsState {
  byPaper: Record<PaperId, LiveJob>;
}

function clampProgress(progress: number): number {
  return Number.isFinite(progress) ? Math.min(1, Math.max(0, progress)) : 0;
}

export function createJobsStore(): Readable<JobsState> & {
  init: () => Promise<void>;
  close: () => void;
  get: (paperId: PaperId) => LiveJob | undefined;
  seed: (paperId: PaperId, job: JobSummary | null) => void;
} {
  const { subscribe, update } = writable<JobsState>({ byPaper: {} });
  let snapshot: JobsState = { byPaper: {} };
  subscribe((s) => (snapshot = s));

  let unlisten: (() => void) | null = null;
  let initPromise: Promise<void> | null = null;
  let lifecycle = 0;
  const liveJobIds = new Set<JobId>();
  const pendingByJob = new Map<JobId, PipelineEventPayload[]>();

  function applyEvent(ev: PipelineEventPayload) {
    let matched = ev.type === "stage_started";
    update((s) => {
      const byPaper = { ...s.byPaper };
      if (ev.type === "stage_started") {
        const existing = byPaper[ev.paper_id];
        byPaper[ev.paper_id] = {
          id: ev.job_id,
          paper_id: ev.paper_id,
          stage: ev.stage,
          status: "running",
          progress:
            existing?.id === ev.job_id && existing.stage === ev.stage
              ? existing.progress
              : 0,
          error: null,
        };
      } else if (ev.type === "stage_progress") {
        for (const key of Object.keys(byPaper)) {
          if (byPaper[key]?.id === ev.job_id) {
            matched = true;
            byPaper[key] = {
              ...byPaper[key]!,
              stage: ev.stage,
              status: "running",
              progress: clampProgress(ev.progress),
              error: null,
            };
          }
        }
      } else if (ev.type === "stage_completed") {
        for (const key of Object.keys(byPaper)) {
          if (byPaper[key]?.id === ev.job_id) {
            matched = true;
            const isFinal = ev.stage === "complete";
            byPaper[key] = {
              ...byPaper[key]!,
              stage: ev.stage,
              progress: isFinal ? 1 : byPaper[key]!.progress,
              status: isFinal ? "completed" : byPaper[key]!.status,
            };
          }
        }
      } else if (ev.type === "stage_failed") {
        for (const key of Object.keys(byPaper)) {
          if (byPaper[key]?.id === ev.job_id) {
            matched = true;
            byPaper[key] = {
              ...byPaper[key]!,
              stage: ev.stage,
              status: "failed",
              error: ev.error,
            };
          }
        }
      }
      return { byPaper };
    });

    if (ev.type === "page_ready") return;
    if (matched) {
      liveJobIds.add(ev.job_id);
      const pending = pendingByJob.get(ev.job_id);
      if (pending) {
        pendingByJob.delete(ev.job_id);
        for (const queued of pending) applyEvent(queued);
      }
      return;
    }

    const pending = pendingByJob.get(ev.job_id) ?? [];
    pending.push(ev);
    pendingByJob.set(ev.job_id, pending.slice(-20));
  }

  async function init() {
    if (!isTauri() || unlisten) return;
    if (initPromise) return initPromise;

    const token = lifecycle;
    initPromise = events
      .onPipeline((ev) => applyEvent(ev.payload))
      .then((stop) => {
        if (token !== lifecycle) {
          stop();
          return;
        }
        unlisten = stop;
      })
      .catch((error) => {
        console.error("pipeline event subscription failed", error);
      })
      .finally(() => {
        if (token === lifecycle) initPromise = null;
      });
    return initPromise;
  }

  function close() {
    lifecycle += 1;
    unlisten?.();
    unlisten = null;
    initPromise = null;
    liveJobIds.clear();
    pendingByJob.clear();
  }

  function seed(paperId: PaperId, job: JobSummary | null) {
    let pending: PipelineEventPayload[] | undefined;
    update((s) => {
      const byPaper = { ...s.byPaper };
      if (!job) {
        const existing = byPaper[paperId];
        if (!existing) return s;
        if (liveJobIds.has(existing.id)) return s;
        liveJobIds.delete(existing.id);
        delete byPaper[paperId];
        return { byPaper };
      }
      // If a live event has already produced a fresher snapshot,
      // keep it — the seeded copy is from a one-shot DB read.
      const existing = byPaper[paperId];
      if (existing && existing.id === job.id && liveJobIds.has(job.id)) {
        return s;
      }
      byPaper[paperId] = {
        id: job.id,
        paper_id: job.paper_id,
        stage: job.stage,
        status: job.status === "pending" ? "running" : job.status,
        progress: clampProgress(job.progress),
        error: job.error_message,
      };
      pending = pendingByJob.get(job.id);
      pendingByJob.delete(job.id);
      return { byPaper };
    });
    if (pending) {
      for (const event of pending) applyEvent(event);
    }
  }

  return {
    subscribe,
    init,
    close,
    get: (paperId: PaperId) => snapshot.byPaper[paperId],
    seed,
  };
}

export const jobs = createJobsStore();
