/**
 * Paper-detail store — holds the current paper + its pages + active job.
 * Reloads on `paper:updated` and `pipeline` events.
 */

import { writable, type Readable } from "svelte/store";
import {
  ipc,
  events,
  type PaperDetail,
  type PaperId,
} from "$lib/ipc";
import { isTauri } from "$lib/ipc/tauri-guard";
import { jobs } from "$lib/stores/jobs";
import { errorMessage } from "./errors";

interface State {
  paper: PaperDetail | null;
  loading: boolean;
  error: string | null;
}

export function createPaperDetailStore() {
  const { subscribe, set, update } = writable<State>({
    paper: null,
    loading: false,
    error: null,
  });

  let unlistenPaper: (() => void) | null = null;
  let unlistenPipeline: (() => void) | null = null;
  let currentId: PaperId | null = null;
  let currentJobId: string | null = null;
  let lifecycle = 0;
  let loadVersion = 0;
  let opening: Promise<void> | null = null;
  let openingId: PaperId | null = null;

  async function load(id: PaperId, token: number) {
    if (!isTauri()) {
      if (token === lifecycle && currentId === id) {
        set({ paper: null, loading: false, error: null });
      }
      return;
    }
    const request = ++loadVersion;
    update((s) => ({ ...s, loading: true, error: null }));
    try {
      const paper = await ipc.getPaper(id);
      if (
        token !== lifecycle ||
        currentId !== id ||
        request !== loadVersion
      ) {
        return;
      }
      // Seed the live-jobs store with whatever the DB knows so the
      // ProgressPanel can show a Resume button for failed/paused
      // jobs even if no pipeline events have fired this session.
      jobs.seed(id, paper.active_job);
      currentJobId = paper.active_job?.id ?? null;
      set({ paper, loading: false, error: null });
    } catch (err) {
      if (
        token === lifecycle &&
        currentId === id &&
        request === loadVersion
      ) {
        update((state) => ({
          ...state,
          loading: false,
          error: errorMessage(err),
        }));
      }
    }
  }

  async function attachListeners(id: PaperId, token: number) {
    if (!isTauri()) return;
    const [paperResult, pipelineResult] = await Promise.allSettled([
      events.onPaperUpdated((ev) => {
        if (token === lifecycle && ev.payload === id) {
          void load(id, token);
        }
      }),
      events.onPipeline((ev) => {
        const payload = ev.payload;
        const isTerminalForCurrentJob =
          "job_id" in payload &&
          payload.job_id === currentJobId &&
          (payload.type === "stage_failed" ||
            (payload.type === "stage_completed" &&
              payload.stage === "complete"));
        if (
          token === lifecycle &&
          (("paper_id" in payload && payload.paper_id === id) ||
            isTerminalForCurrentJob)
        ) {
          void load(id, token);
        }
      }),
    ]);

    const paperStop =
      paperResult.status === "fulfilled" ? paperResult.value : null;
    const pipelineStop =
      pipelineResult.status === "fulfilled" ? pipelineResult.value : null;
    if (token !== lifecycle || currentId !== id) {
      paperStop?.();
      pipelineStop?.();
      return;
    }
    if (paperResult.status === "rejected") {
      console.error("paper update subscription failed", paperResult.reason);
    }
    if (pipelineResult.status === "rejected") {
      console.error("pipeline subscription failed", pipelineResult.reason);
    }
    if (!paperStop || !pipelineStop) {
      paperStop?.();
      pipelineStop?.();
      return;
    }
    unlistenPaper = paperStop;
    unlistenPipeline = pipelineStop;
  }

  function open(id: PaperId): Promise<void> {
    if (currentId === id && openingId === id && opening) {
      return opening;
    }
    if (currentId === id && unlistenPaper && unlistenPipeline) {
      return Promise.resolve();
    }

    const token = ++lifecycle;
    loadVersion += 1;
    unlistenPaper?.();
    unlistenPipeline?.();
    unlistenPaper = null;
    unlistenPipeline = null;
    currentId = id;
    currentJobId = null;
    set({ paper: null, loading: isTauri(), error: null });

    const task = Promise.all([
      load(id, token),
      attachListeners(id, token),
    ]).then(() => {});
    opening = task;
    openingId = id;
    void task.finally(() => {
      if (token === lifecycle) {
        opening = null;
        openingId = null;
      }
    });
    return task;
  }

  function close() {
    lifecycle += 1;
    loadVersion += 1;
    unlistenPaper?.();
    unlistenPipeline?.();
    unlistenPaper = null;
    unlistenPipeline = null;
    currentId = null;
    currentJobId = null;
    opening = null;
    openingId = null;
    set({ paper: null, loading: false, error: null });
  }

  const store: Readable<State> & {
    open: (id: PaperId) => Promise<void>;
    close: () => void;
  } = { subscribe, open, close };

  return store;
}

export const paperDetail = createPaperDetailStore();
