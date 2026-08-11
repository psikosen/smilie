import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { get } from "svelte/store";

// Same pattern as experiments.test.ts — mock $lib/ipc so we don't pull
// in @tauri-apps/api at test time.
const listComparisonsForPaper = vi.fn();
const getComparison = vi.fn();
const onComparison = vi.fn();

vi.mock("$lib/ipc", () => ({
  ipc: {
    listComparisonsForPaper: (...a: unknown[]) =>
      listComparisonsForPaper(...a),
    getComparison: (...a: unknown[]) => getComparison(...a),
  },
  events: {
    onComparison: (cb: unknown) => onComparison(cb),
  },
}));

import { comparisons } from "./comparisons";
import type { ComparisonDetail, ComparisonSummary } from "$lib/ipc";

const PAPER_ID = "paper_42";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function summary(
  id: string,
  patch: Partial<ComparisonSummary> = {},
): ComparisonSummary {
  return {
    id,
    paper_a_id: PAPER_ID,
    paper_b_id: `b_${id}`,
    paper_a_title: "Paper A",
    paper_b_title: `Paper ${id}`,
    status: "ready",
    created_at: 0,
    updated_at: 0,
    ...patch,
  };
}

function detailFor(id: string): ComparisonDetail {
  return {
    ...summary(id),
    html_path: `/tmp/compare_${id}.html`,
    approach_summary: null,
    error_message: null,
  };
}

describe("comparisons store", () => {
  beforeEach(() => {
    listComparisonsForPaper.mockReset();
    getComparison.mockReset();
    onComparison.mockReset();
    // Pretend we're in Tauri so the real codepaths run.
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      metadata: {},
    };
    onComparison.mockResolvedValue(() => {});
    comparisons.close();
  });

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    comparisons.close();
  });

  it("starts empty", () => {
    const state = get(comparisons);
    expect(state.list).toEqual([]);
    expect(state.detail).toEqual({});
    expect(state.paperId).toBeNull();
  });

  it("open(paperId) refreshes list via ipc and stores it", async () => {
    listComparisonsForPaper.mockResolvedValue([summary("c1"), summary("c2")]);
    await comparisons.open(PAPER_ID);

    expect(listComparisonsForPaper).toHaveBeenCalledWith(PAPER_ID);
    const state = get(comparisons);
    expect(state.list.map((c) => c.id)).toEqual(["c1", "c2"]);
    expect(state.paperId).toBe(PAPER_ID);
    expect(state.loading).toBe(false);
    expect(onComparison).toHaveBeenCalledTimes(1);
  });

  it("loadDetail caches the detail in the store", async () => {
    listComparisonsForPaper.mockResolvedValue([summary("c1")]);
    getComparison.mockResolvedValue(detailFor("c1"));
    await comparisons.open(PAPER_ID);

    const det = await comparisons.loadDetail("c1");
    expect(det.id).toBe("c1");

    const state = get(comparisons);
    expect(state.detail["c1"].html_path).toBe("/tmp/compare_c1.html");
  });

  it("upsertSummary inserts a new summary at the top", () => {
    comparisons.upsertSummary(summary("a"));
    comparisons.upsertSummary(summary("b"));
    const state = get(comparisons);
    expect(state.list.map((c) => c.id)).toEqual(["b", "a"]);
  });

  it("upsertSummary replaces an existing entry by id without reordering", async () => {
    listComparisonsForPaper.mockResolvedValue([summary("a"), summary("b")]);
    await comparisons.open(PAPER_ID);

    comparisons.upsertSummary(summary("a", { status: "failed" }));
    const state = get(comparisons);
    expect(state.list.map((c) => c.id)).toEqual(["a", "b"]);
    expect(state.list[0].status).toBe("failed");
  });

  it("removeFromList drops the row and its cached detail", async () => {
    listComparisonsForPaper.mockResolvedValue([summary("a"), summary("b")]);
    getComparison.mockResolvedValue(detailFor("a"));
    await comparisons.open(PAPER_ID);
    await comparisons.loadDetail("a");

    comparisons.removeFromList("a");
    const state = get(comparisons);
    expect(state.list.map((c) => c.id)).toEqual(["b"]);
    expect(state.detail["a"]).toBeUndefined();
  });

  it("subscribes to comparison events on open and unsubscribes on close", async () => {
    const unsub = vi.fn();
    onComparison.mockResolvedValue(unsub);
    listComparisonsForPaper.mockResolvedValue([]);

    await comparisons.open(PAPER_ID);
    expect(onComparison).toHaveBeenCalled();

    comparisons.close();
    expect(unsub).toHaveBeenCalled();
    expect(get(comparisons).paperId).toBeNull();
  });

  it("event payload triggers a list refresh, and refreshes cached detail", async () => {
    let captured:
      | ((ev: { payload: unknown }) => Promise<void> | void)
      | null = null;
    onComparison.mockImplementation((cb: typeof captured) => {
      captured = cb;
      return Promise.resolve(() => {});
    });
    listComparisonsForPaper.mockResolvedValue([summary("c1")]);
    getComparison.mockResolvedValue(detailFor("c1"));

    await comparisons.open(PAPER_ID);
    await comparisons.loadDetail("c1");

    listComparisonsForPaper.mockClear();
    getComparison.mockClear();
    listComparisonsForPaper.mockResolvedValue([
      summary("c1", { status: "ready" }),
    ]);
    getComparison.mockResolvedValue(detailFor("c1"));

    expect(captured).not.toBeNull();
    await captured!({
      payload: {
        type: "comparison_status",
        comparison_id: "c1",
        status: "ready",
      },
    });

    expect(listComparisonsForPaper).toHaveBeenCalledTimes(1);
    expect(getComparison).toHaveBeenCalledWith("c1");
    expect(get(comparisons).list[0].status).toBe("ready");
  });

  it("event payload does NOT refresh detail for an uncached comparison", async () => {
    let captured:
      | ((ev: { payload: unknown }) => Promise<void> | void)
      | null = null;
    onComparison.mockImplementation((cb: typeof captured) => {
      captured = cb;
      return Promise.resolve(() => {});
    });
    listComparisonsForPaper.mockResolvedValue([summary("c1")]);

    await comparisons.open(PAPER_ID);
    listComparisonsForPaper.mockClear();
    getComparison.mockClear();
    listComparisonsForPaper.mockResolvedValue([summary("c1")]);

    await captured!({
      payload: {
        type: "comparison_status",
        comparison_id: "c1",
        status: "ready",
      },
    });

    expect(listComparisonsForPaper).toHaveBeenCalledTimes(1);
    expect(getComparison).not.toHaveBeenCalled();
  });

  it("ignores a previous paper's list when it resolves after a switch", async () => {
    const slow = deferred<ComparisonSummary[]>();
    listComparisonsForPaper.mockImplementation((paperId: string) =>
      paperId === "paper-a"
        ? slow.promise
        : Promise.resolve([
            summary("current", {
              paper_a_id: "paper-b",
              paper_b_id: "paper-c",
            }),
          ]),
    );

    const openingA = comparisons.open("paper-a");
    await vi.waitFor(() =>
      expect(listComparisonsForPaper).toHaveBeenCalledWith("paper-a"),
    );
    await comparisons.open("paper-b");
    slow.resolve([
      summary("stale", {
        paper_a_id: "paper-a",
        paper_b_id: "paper-c",
      }),
    ]);
    await openingA;

    expect(get(comparisons).paperId).toBe("paper-b");
    expect(get(comparisons).list.map((item) => item.id)).toEqual(["current"]);
  });

  it("disposes a subscription that resolves after close", async () => {
    const registration = deferred<() => void>();
    const stop = vi.fn();
    onComparison.mockReturnValue(registration.promise);
    listComparisonsForPaper.mockResolvedValue([]);

    const opening = comparisons.open(PAPER_ID);
    comparisons.close();
    registration.resolve(stop);
    await opening;

    expect(stop).toHaveBeenCalledTimes(1);
    expect(get(comparisons).paperId).toBeNull();
    expect(listComparisonsForPaper).not.toHaveBeenCalled();
  });
});
