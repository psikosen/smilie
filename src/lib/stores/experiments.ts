/**
 * Experiments store — mirrors the Rust experiments table for the open
 * paper. Subscribes to `experiment` events while open and refreshes the
 * affected experiment's detail when one fires.
 *
 * Keyed implicitly by the open `paperId` — opening a different paper
 * resets state. Detail rows are cached lazily; the list is the source of
 * truth for what exists, and `loadDetail(id)` pulls the heavy fields on
 * demand (python_code/tests/inputs_schema/outputs_schema).
 */

import { writable, type Readable } from "svelte/store";
import {
  ipc,
  events,
  type ExperimentSummary,
  type ExperimentDetail,
  type ExperimentId,
  type PaperId,
} from "$lib/ipc";
import { isTauri } from "$lib/ipc/tauri-guard";
import { errorMessage } from "./errors";

export interface ExperimentsState {
  paperId: PaperId | null;
  list: ExperimentSummary[];
  detail: Record<ExperimentId, ExperimentDetail>;
  loading: boolean;
  error: string | null;
}

const EMPTY: ExperimentsState = {
  paperId: null,
  list: [],
  detail: {},
  loading: false,
  error: null,
};

function create() {
  const { subscribe, set, update } = writable<ExperimentsState>(EMPTY);

  let unlistenExperiment: (() => void) | null = null;
  let currentPaperId: PaperId | null = null;
  let lifecycle = 0;
  let refreshVersion = 0;
  let opening: Promise<void> | null = null;
  let openingPaperId: PaperId | null = null;

  async function refreshList() {
    const paperId = currentPaperId;
    const token = lifecycle;
    if (!paperId) return;
    if (!isTauri()) {
      if (token === lifecycle && currentPaperId === paperId) {
        update((s) => ({ ...s, loading: false }));
      }
      return;
    }
    const request = ++refreshVersion;
    update((s) => ({ ...s, loading: true, error: null }));
    try {
      const list = await ipc.listExperimentsForPaper(paperId);
      if (
        token !== lifecycle ||
        currentPaperId !== paperId ||
        request !== refreshVersion
      ) {
        return;
      }
      update((s) => ({ ...s, list, loading: false, error: null }));
    } catch (err) {
      if (
        token === lifecycle &&
        currentPaperId === paperId &&
        request === refreshVersion
      ) {
        update((s) => ({
          ...s,
          loading: false,
          error: errorMessage(err),
        }));
      }
    }
  }

  async function loadDetail(id: ExperimentId): Promise<ExperimentDetail> {
    if (!isTauri()) {
      throw new Error("loadDetail requires Tauri runtime");
    }
    const paperId = currentPaperId;
    const token = lifecycle;
    const detail = await ipc.getExperiment(id);
    if (
      paperId &&
      token === lifecycle &&
      currentPaperId === paperId &&
      detail.paper_id === paperId
    ) {
      update((s) => ({ ...s, detail: { ...s.detail, [id]: detail } }));
    }
    return detail;
  }

  function open(paperId: PaperId): Promise<void> {
    if (
      currentPaperId === paperId &&
      openingPaperId === paperId &&
      opening
    ) {
      return opening;
    }
    if (currentPaperId === paperId && unlistenExperiment) {
      // Already open for this paper — just refresh.
      return refreshList();
    }
    // Tearing down a previous subscription (different paper) before
    // re-subscribing keeps the event firehose tied to the visible paper.
    unlistenExperiment?.();
    unlistenExperiment = null;
    const token = ++lifecycle;
    refreshVersion += 1;
    currentPaperId = paperId;
    set({ ...EMPTY, paperId });

    const task = (async () => {
      if (isTauri()) {
        try {
          const stop = await events.onExperiment(async (ev) => {
            const payload = ev.payload;
            if (
              token !== lifecycle ||
              currentPaperId !== paperId ||
              !payload ||
              payload.type !== "experiment_status"
            ) {
              return;
            }
            // Refresh both list (for status pill) and detail (for the runner).
            await refreshList();
            if (token !== lifecycle || currentPaperId !== paperId) return;
            const id = payload.experiment_id;
            let cached = false;
            update((s) => {
              cached = id in s.detail;
              return s;
            });
            if (cached) {
              try {
                await loadDetail(id);
              } catch (err) {
                console.error("experiment detail refresh failed", err);
              }
            }
          });
          if (token !== lifecycle || currentPaperId !== paperId) {
            stop();
            return;
          }
          unlistenExperiment = stop;
        } catch (error) {
          if (token === lifecycle && currentPaperId === paperId) {
            console.error("experiment event subscription failed", error);
          }
        }
      }
      await refreshList();
    })();
    opening = task;
    openingPaperId = paperId;
    void task.finally(() => {
      if (token === lifecycle) {
        opening = null;
        openingPaperId = null;
      }
    });
    return task;
  }

  function close() {
    lifecycle += 1;
    refreshVersion += 1;
    unlistenExperiment?.();
    unlistenExperiment = null;
    currentPaperId = null;
    opening = null;
    openingPaperId = null;
    set(EMPTY);
  }

  function upsertSummary(summary: ExperimentSummary) {
    update((s) => {
      const existingIdx = s.list.findIndex((e) => e.id === summary.id);
      const list =
        existingIdx >= 0
          ? s.list.map((e, i) => (i === existingIdx ? summary : e))
          : [summary, ...s.list];
      return { ...s, list };
    });
  }

  function removeFromList(id: ExperimentId) {
    update((s) => {
      const list = s.list.filter((e) => e.id !== id);
      const detail = { ...s.detail };
      delete detail[id];
      return { ...s, list, detail };
    });
  }

  const store: Readable<ExperimentsState> & {
    open: (paperId: PaperId) => Promise<void>;
    close: () => void;
    refreshList: () => Promise<void>;
    loadDetail: (id: ExperimentId) => Promise<ExperimentDetail>;
    upsertSummary: (summary: ExperimentSummary) => void;
    removeFromList: (id: ExperimentId) => void;
  } = {
    subscribe,
    open,
    close,
    refreshList,
    loadDetail,
    upsertSummary,
    removeFromList,
  };

  return store;
}

export const experiments = create();
