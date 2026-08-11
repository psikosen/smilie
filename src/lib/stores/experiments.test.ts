import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { get } from "svelte/store";

// The store imports ipc + events via $lib/ipc. We mock that whole
// module so we don't pull in @tauri-apps/api at test time.
const listExperimentsForPaper = vi.fn();
const getExperiment = vi.fn();
const onExperiment = vi.fn();

vi.mock("$lib/ipc", () => ({
  ipc: {
    listExperimentsForPaper: (...a: unknown[]) =>
      listExperimentsForPaper(...a),
    getExperiment: (...a: unknown[]) => getExperiment(...a),
  },
  events: {
    onExperiment: (cb: unknown) => onExperiment(cb),
  },
}));

import { experiments } from "./experiments";
import type {
  ExperimentDetail,
  ExperimentSummary,
} from "$lib/ipc";

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
  patch: Partial<ExperimentSummary> = {},
): ExperimentSummary {
  return {
    id,
    paper_id: PAPER_ID,
    parent_experiment_id: null,
    version: 1,
    title: `experiment ${id}`,
    status: "ready",
    plausibility_score: 80,
    refine_prompt: null,
    created_at: 0,
    updated_at: 0,
    ...patch,
  };
}

function detailFor(id: string): ExperimentDetail {
  return {
    ...summary(id),
    plausibility_reason: null,
    approach_summary: null,
    inputs_schema: [],
    outputs_schema: [],
    python_code: "OUTPUTS = {}",
    python_tests: null,
    test_attempts: 0,
    test_last_output: null,
    model_id: null,
  };
}

describe("experiments store", () => {
  beforeEach(() => {
    listExperimentsForPaper.mockReset();
    getExperiment.mockReset();
    onExperiment.mockReset();
    // Pretend we're in Tauri so the real codepaths run.
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      metadata: {},
    };
    // Default: events.onExperiment returns a teardown stub so open()
    // doesn't crash when subscribing.
    onExperiment.mockResolvedValue(() => {});
    experiments.close();
  });

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    experiments.close();
  });

  it("starts empty", () => {
    const state = get(experiments);
    expect(state.list).toEqual([]);
    expect(state.detail).toEqual({});
    expect(state.paperId).toBeNull();
  });

  it("open(paperId) refreshes list via ipc and stores it", async () => {
    listExperimentsForPaper.mockResolvedValue([summary("e1"), summary("e2")]);
    await experiments.open(PAPER_ID);

    expect(listExperimentsForPaper).toHaveBeenCalledWith(PAPER_ID);
    const state = get(experiments);
    expect(state.list.map((e) => e.id)).toEqual(["e1", "e2"]);
    expect(state.paperId).toBe(PAPER_ID);
    expect(state.loading).toBe(false);
    // Subscribed to the experiment event channel.
    expect(onExperiment).toHaveBeenCalledTimes(1);
  });

  it("loadDetail caches the detail in the store", async () => {
    listExperimentsForPaper.mockResolvedValue([summary("e1")]);
    getExperiment.mockResolvedValue(detailFor("e1"));
    await experiments.open(PAPER_ID);

    const det = await experiments.loadDetail("e1");
    expect(det.id).toBe("e1");

    const state = get(experiments);
    expect(state.detail["e1"].python_code).toBe("OUTPUTS = {}");
  });

  it("upsertSummary inserts a new summary at the top", () => {
    experiments.upsertSummary(summary("a"));
    experiments.upsertSummary(summary("b"));
    const state = get(experiments);
    // Newest first.
    expect(state.list.map((e) => e.id)).toEqual(["b", "a"]);
  });

  it("upsertSummary replaces an existing entry by id without reordering", async () => {
    listExperimentsForPaper.mockResolvedValue([summary("a"), summary("b")]);
    await experiments.open(PAPER_ID);

    experiments.upsertSummary(summary("a", { title: "renamed" }));
    const state = get(experiments);
    expect(state.list.map((e) => e.id)).toEqual(["a", "b"]);
    expect(state.list[0].title).toBe("renamed");
  });

  it("removeFromList drops the row and its cached detail", async () => {
    listExperimentsForPaper.mockResolvedValue([summary("a"), summary("b")]);
    getExperiment.mockResolvedValue(detailFor("a"));
    await experiments.open(PAPER_ID);
    await experiments.loadDetail("a");

    experiments.removeFromList("a");
    const state = get(experiments);
    expect(state.list.map((e) => e.id)).toEqual(["b"]);
    expect(state.detail["a"]).toBeUndefined();
  });

  it("subscribes to experiment events on open and unsubscribes on close", async () => {
    const unsub = vi.fn();
    onExperiment.mockResolvedValue(unsub);
    listExperimentsForPaper.mockResolvedValue([]);

    await experiments.open(PAPER_ID);
    expect(onExperiment).toHaveBeenCalled();

    experiments.close();
    expect(unsub).toHaveBeenCalled();
    expect(get(experiments).paperId).toBeNull();
  });

  it("event payload triggers a list refresh, and refreshes cached detail", async () => {
    let captured:
      | ((ev: { payload: unknown }) => Promise<void> | void)
      | null = null;
    onExperiment.mockImplementation((cb: typeof captured) => {
      captured = cb;
      return Promise.resolve(() => {});
    });
    listExperimentsForPaper.mockResolvedValue([summary("e1")]);
    getExperiment.mockResolvedValue(detailFor("e1"));

    await experiments.open(PAPER_ID);
    // Cache detail for e1 so the event handler refreshes it.
    await experiments.loadDetail("e1");

    listExperimentsForPaper.mockClear();
    getExperiment.mockClear();
    listExperimentsForPaper.mockResolvedValue([
      summary("e1", { status: "ready", plausibility_score: 99 }),
    ]);
    getExperiment.mockResolvedValue(detailFor("e1"));

    expect(captured).not.toBeNull();
    await captured!({
      payload: {
        type: "experiment_status",
        experiment_id: "e1",
        status: "ready",
      },
    });

    expect(listExperimentsForPaper).toHaveBeenCalledTimes(1);
    // Cached detail → refreshed.
    expect(getExperiment).toHaveBeenCalledWith("e1");
    expect(get(experiments).list[0].plausibility_score).toBe(99);
  });

  it("event payload does NOT refresh detail for an uncached experiment", async () => {
    let captured:
      | ((ev: { payload: unknown }) => Promise<void> | void)
      | null = null;
    onExperiment.mockImplementation((cb: typeof captured) => {
      captured = cb;
      return Promise.resolve(() => {});
    });
    listExperimentsForPaper.mockResolvedValue([summary("e1")]);

    await experiments.open(PAPER_ID);
    listExperimentsForPaper.mockClear();
    getExperiment.mockClear();
    listExperimentsForPaper.mockResolvedValue([summary("e1")]);

    await captured!({
      payload: {
        type: "experiment_status",
        experiment_id: "e1",
        status: "ready",
      },
    });

    expect(listExperimentsForPaper).toHaveBeenCalledTimes(1);
    // Detail wasn't loaded yet so the event handler shouldn't fetch it.
    expect(getExperiment).not.toHaveBeenCalled();
  });

  it("ignores a previous paper's list when it resolves after a switch", async () => {
    const slow = deferred<ExperimentSummary[]>();
    listExperimentsForPaper.mockImplementation((paperId: string) =>
      paperId === "paper-a"
        ? slow.promise
        : Promise.resolve([
            summary("current", { paper_id: "paper-b" }),
          ]),
    );

    const openingA = experiments.open("paper-a");
    await vi.waitFor(() =>
      expect(listExperimentsForPaper).toHaveBeenCalledWith("paper-a"),
    );
    await experiments.open("paper-b");
    slow.resolve([summary("stale", { paper_id: "paper-a" })]);
    await openingA;

    expect(get(experiments).paperId).toBe("paper-b");
    expect(get(experiments).list.map((item) => item.id)).toEqual(["current"]);
  });

  it("disposes a subscription that resolves after close", async () => {
    const registration = deferred<() => void>();
    const stop = vi.fn();
    onExperiment.mockReturnValue(registration.promise);
    listExperimentsForPaper.mockResolvedValue([]);

    const opening = experiments.open(PAPER_ID);
    experiments.close();
    registration.resolve(stop);
    await opening;

    expect(stop).toHaveBeenCalledTimes(1);
    expect(get(experiments).paperId).toBeNull();
    expect(listExperimentsForPaper).not.toHaveBeenCalled();
  });
});
