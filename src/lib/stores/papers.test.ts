import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { PaperSummary } from "$lib/ipc";

const listPapers = vi.fn();
const onPaperUpdated = vi.fn();

vi.mock("$lib/ipc", () => ({
  ipc: {
    listPapers: () => listPapers(),
  },
  events: {
    onPaperUpdated: (callback: unknown) => onPaperUpdated(callback),
  },
}));

import { createPapersStore } from "./papers";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function paper(id: string): PaperSummary {
  return {
    id,
    title: id,
    authors: [],
    page_count: 1,
    processing_status: "complete",
    created_at: 0,
    updated_at: 0,
    thumbnail_path: null,
  };
}

describe("papers store", () => {
  beforeEach(() => {
    listPapers.mockReset();
    onPaperUpdated.mockReset();
    onPaperUpdated.mockResolvedValue(() => {});
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  });

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("ignores a slower refresh that completes after a newer one", async () => {
    const slow = deferred<PaperSummary[]>();
    const fast = deferred<PaperSummary[]>();
    listPapers
      .mockReturnValueOnce(slow.promise)
      .mockReturnValueOnce(fast.promise);
    const store = createPapersStore();

    const first = store.refresh();
    const second = store.refresh();
    fast.resolve([paper("new")]);
    await second;
    slow.resolve([paper("stale")]);
    await first;

    expect(get(store).items.map((item) => item.id)).toEqual(["new"]);
  });

  it("preserves the last library snapshot when refresh fails", async () => {
    listPapers.mockResolvedValueOnce([paper("saved")]);
    const store = createPapersStore();
    await store.refresh();
    listPapers.mockRejectedValueOnce({ code: "SQLITE_BUSY" });

    await store.refresh();

    expect(get(store)).toMatchObject({
      items: [paper("saved")],
      loading: false,
      error: '{"code":"SQLITE_BUSY"}',
    });
  });

  it("deduplicates concurrent init and subscribes before refreshing", async () => {
    const registration = deferred<() => void>();
    onPaperUpdated.mockReturnValue(registration.promise);
    listPapers.mockResolvedValue([]);
    const store = createPapersStore();

    const first = store.init();
    const second = store.init();
    expect(onPaperUpdated).toHaveBeenCalledTimes(1);
    expect(listPapers).not.toHaveBeenCalled();

    registration.resolve(() => {});
    await Promise.all([first, second]);
    expect(listPapers).toHaveBeenCalledTimes(1);
    store.close();
  });

  it("disposes a listener that resolves after close", async () => {
    const registration = deferred<() => void>();
    const stop = vi.fn();
    onPaperUpdated.mockReturnValue(registration.promise);
    const store = createPapersStore();

    const opening = store.init();
    store.close();
    registration.resolve(stop);
    await opening;

    expect(stop).toHaveBeenCalledTimes(1);
    expect(listPapers).not.toHaveBeenCalled();
  });
});
