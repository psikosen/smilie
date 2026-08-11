/**
 * Comparisons store — mirrors the Rust comparisons table for the open
 * paper. Subscribes to `comparison` events while open and refreshes the
 * affected comparison's detail when one fires.
 *
 * Keyed implicitly by the open `paperId` — opening a different paper
 * resets state. Detail rows are cached lazily; the list is the source of
 * truth for what exists, and `loadDetail(id)` pulls the heavy fields on
 * demand (html_path, approach_summary, error_message).
 *
 * Mirrors the experiments-store shape so the Compare tab can reuse the
 * same lifecycle conventions (open / close / refreshList / loadDetail /
 * upsertSummary / removeFromList).
 */

import { writable, type Readable } from "svelte/store";
import {
  ipc,
  events,
  type ComparisonSummary,
  type ComparisonDetail,
  type ComparisonId,
  type PaperId,
} from "$lib/ipc";
import { isTauri } from "$lib/ipc/tauri-guard";
import { errorMessage } from "./errors";

export interface ComparisonsState {
  paperId: PaperId | null;
  list: ComparisonSummary[];
  detail: Record<ComparisonId, ComparisonDetail>;
  loading: boolean;
  error: string | null;
}

const EMPTY: ComparisonsState = {
  paperId: null,
  list: [],
  detail: {},
  loading: false,
  error: null,
};

function create() {
  const { subscribe, set, update } = writable<ComparisonsState>(EMPTY);

  let unlistenComparison: (() => void) | null = null;
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
      const list = await ipc.listComparisonsForPaper(paperId);
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

  async function loadDetail(id: ComparisonId): Promise<ComparisonDetail> {
    if (!isTauri()) {
      throw new Error("loadDetail requires Tauri runtime");
    }
    const paperId = currentPaperId;
    const token = lifecycle;
    const detail = await ipc.getComparison(id);
    if (
      paperId &&
      token === lifecycle &&
      currentPaperId === paperId &&
      (detail.paper_a_id === paperId || detail.paper_b_id === paperId)
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
    if (currentPaperId === paperId && unlistenComparison) {
      // Already open for this paper — just refresh.
      return refreshList();
    }
    // Tearing down a previous subscription (different paper) before
    // re-subscribing keeps the event firehose tied to the visible paper.
    unlistenComparison?.();
    unlistenComparison = null;
    const token = ++lifecycle;
    refreshVersion += 1;
    currentPaperId = paperId;
    set({ ...EMPTY, paperId });

    const task = (async () => {
      if (isTauri()) {
        try {
          const stop = await events.onComparison(async (ev) => {
            const payload = ev.payload;
            if (
              token !== lifecycle ||
              currentPaperId !== paperId ||
              !payload ||
              payload.type !== "comparison_status"
            ) {
              return;
            }
            await refreshList();
            if (token !== lifecycle || currentPaperId !== paperId) return;
            const id = payload.comparison_id;
            let cached = false;
            update((s) => {
              cached = id in s.detail;
              return s;
            });
            if (cached) {
              try {
                await loadDetail(id);
              } catch (err) {
                console.error("comparison detail refresh failed", err);
              }
            }
          });
          if (token !== lifecycle || currentPaperId !== paperId) {
            stop();
            return;
          }
          unlistenComparison = stop;
        } catch (error) {
          if (token === lifecycle && currentPaperId === paperId) {
            console.error("comparison event subscription failed", error);
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
    unlistenComparison?.();
    unlistenComparison = null;
    currentPaperId = null;
    opening = null;
    openingPaperId = null;
    set(EMPTY);
  }

  function upsertSummary(summary: ComparisonSummary) {
    update((s) => {
      const existingIdx = s.list.findIndex((c) => c.id === summary.id);
      const list =
        existingIdx >= 0
          ? s.list.map((c, i) => (i === existingIdx ? summary : c))
          : [summary, ...s.list];
      return { ...s, list };
    });
  }

  function removeFromList(id: ComparisonId) {
    update((s) => {
      const list = s.list.filter((c) => c.id !== id);
      const detail = { ...s.detail };
      delete detail[id];
      return { ...s, list, detail };
    });
  }

  const store: Readable<ComparisonsState> & {
    open: (paperId: PaperId) => Promise<void>;
    close: () => void;
    refreshList: () => Promise<void>;
    loadDetail: (id: ComparisonId) => Promise<ComparisonDetail>;
    upsertSummary: (summary: ComparisonSummary) => void;
    removeFromList: (id: ComparisonId) => void;
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

export const comparisons = create();
