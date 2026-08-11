/**
 * Papers store — reactive mirror of the Rust `papers` table.
 * Subscribes to `paper:updated` events to stay in sync.
 */

import { writable, type Readable } from "svelte/store";
import { ipc, events, type PaperSummary } from "$lib/ipc";
import { isTauri } from "$lib/ipc/tauri-guard";
import { errorMessage } from "./errors";

interface PapersState {
  items: PaperSummary[];
  loading: boolean;
  error: string | null;
}

export function createPapersStore(): Readable<PapersState> & {
  refresh: () => Promise<void>;
  init: () => Promise<void>;
  close: () => void;
} {
  const { subscribe, update } = writable<PapersState>({
    items: [],
    loading: false,
    error: null,
  });

  let unlisten: (() => void) | null = null;
  let initPromise: Promise<void> | null = null;
  let lifecycle = 0;
  let refreshVersion = 0;

  async function refresh() {
    if (!isTauri()) return;
    const request = ++refreshVersion;
    update((s) => ({ ...s, loading: true, error: null }));
    try {
      const items = await ipc.listPapers();
      if (request !== refreshVersion) return;
      update((s) => ({ ...s, items, loading: false, error: null }));
    } catch (err) {
      if (request !== refreshVersion) return;
      update((s) => ({
        ...s,
        loading: false,
        error: errorMessage(err),
      }));
    }
  }

  async function init() {
    if (!isTauri() || unlisten) return;
    if (initPromise) return initPromise;

    const token = lifecycle;
    initPromise = (async () => {
      try {
        const stop = await events.onPaperUpdated(() => {
          void refresh();
        });
        if (token !== lifecycle) {
          stop();
          return;
        }
        unlisten = stop;
        await refresh();
      } catch (error) {
        if (token === lifecycle) {
          update((s) => ({
            ...s,
            loading: false,
            error: errorMessage(error),
          }));
        }
      }
    })().finally(() => {
      if (token === lifecycle) initPromise = null;
    });
    return initPromise;
  }

  function close() {
    lifecycle += 1;
    refreshVersion += 1;
    unlisten?.();
    unlisten = null;
    initPromise = null;
    update((s) => ({ ...s, loading: false }));
  }

  return { subscribe, refresh, init, close };
}

export const papers = createPapersStore();
