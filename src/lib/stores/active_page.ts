/**
 * Active-page store — populated by the paper detail route so the global
 * keyboard shortcut handler in `+layout.svelte` can navigate prev/next
 * (J/K and ArrowUp/ArrowDown) without coupling to the route component.
 *
 * Pages with `status !== "ready"` are skipped during navigation since the
 * viewer can only render ready pages.
 */

import { writable, get, type Readable } from "svelte/store";
import type { PageRecord } from "$lib/ipc";

interface ActivePageState {
  pages: PageRecord[];
  activePageId: string | null;
  setActivePageId: ((id: string) => void) | null;
}

const EMPTY: ActivePageState = {
  pages: [],
  activePageId: null,
  setActivePageId: null,
};

function create() {
  const inner = writable<ActivePageState>(EMPTY);

  function set(state: Partial<ActivePageState>) {
    inner.update((s) => ({ ...s, ...state }));
  }

  function clear() {
    inner.set(EMPTY);
  }

  /** Step to the previous/next ready page in the list. No-op if nothing
   *  is loaded or there's no ready neighbor. */
  function step(direction: 1 | -1) {
    const s = get(inner);
    if (!s.setActivePageId || s.pages.length === 0) return;
    const ready = s.pages.filter((p) => p.status === "ready");
    if (ready.length === 0) return;
    const currentIdx = ready.findIndex((p) => p.id === s.activePageId);
    // If active is not in ready (e.g. a queued page), pick the first ready
    // when going forward, last when going back.
    const fallback = direction === 1 ? 0 : ready.length - 1;
    const idx = currentIdx === -1 ? fallback : currentIdx + direction;
    if (idx < 0 || idx >= ready.length) return;
    const nextId = ready[idx].id;
    set({ activePageId: nextId });
    s.setActivePageId(nextId);
  }

  const store: Readable<ActivePageState> & {
    set: (state: Partial<ActivePageState>) => void;
    clear: () => void;
    step: (direction: 1 | -1) => void;
  } = { subscribe: inner.subscribe, set, clear, step };

  return store;
}

export const activePage = create();
