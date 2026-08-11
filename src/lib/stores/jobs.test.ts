import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { JobSummary, PipelineEventPayload } from "$lib/ipc";

const onPipeline = vi.fn();

vi.mock("$lib/ipc", () => ({
  events: {
    onPipeline: (callback: unknown) => onPipeline(callback),
  },
}));

import { createJobsStore } from "./jobs";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function job(patch: Partial<JobSummary> = {}): JobSummary {
  return {
    id: "job-1",
    paper_id: "paper-1",
    stage: "ocr_extract",
    status: "failed",
    progress: 0.4,
    current_model: null,
    error_message: "model stopped",
    retry_count: 1,
    ...patch,
  };
}

describe("jobs store", () => {
  beforeEach(() => {
    onPipeline.mockReset();
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  });

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("clears a seeded job when cancellation removes the active job", () => {
    const store = createJobsStore();
    store.seed("paper-1", job());
    store.seed("paper-1", null);
    expect(store.get("paper-1")).toBeUndefined();
  });

  it("does not let a stale null seed erase a live job", async () => {
    let emit!: (event: { payload: PipelineEventPayload }) => void;
    onPipeline.mockImplementation((callback: typeof emit) => {
      emit = callback;
      return Promise.resolve(() => {});
    });
    const store = createJobsStore();
    await store.init();
    emit({
      payload: {
        type: "stage_started",
        job_id: "job-1",
        paper_id: "paper-1",
        stage: "ocr_extract",
      },
    });

    store.seed("paper-1", null);

    expect(store.get("paper-1")).toMatchObject({
      id: "job-1",
      status: "running",
    });
    store.close();
  });

  it("keeps a live failure from being overwritten by a stale DB seed", async () => {
    let emit!: (event: { payload: PipelineEventPayload }) => void;
    onPipeline.mockImplementation((callback: typeof emit) => {
      emit = callback;
      return Promise.resolve(() => {});
    });
    const store = createJobsStore();
    store.seed("paper-1", job({ status: "running", error_message: null }));
    await store.init();

    emit({
      payload: {
        type: "stage_failed",
        job_id: "job-1",
        stage: "build_learning_plan",
        error: "out of memory",
      },
    });
    store.seed("paper-1", job({ status: "running", error_message: null }));

    expect(store.get("paper-1")).toMatchObject({
      stage: "build_learning_plan",
      status: "failed",
      error: "out of memory",
    });
    store.close();
  });

  it("replays a failure that arrives before the job is seeded", async () => {
    let emit!: (event: { payload: PipelineEventPayload }) => void;
    onPipeline.mockImplementation((callback: typeof emit) => {
      emit = callback;
      return Promise.resolve(() => {});
    });
    const store = createJobsStore();
    await store.init();

    emit({
      payload: {
        type: "stage_failed",
        job_id: "job-1",
        stage: "generate_pages",
        error: "generator crashed",
      },
    });
    store.seed("paper-1", job({ status: "running", error_message: null }));

    expect(store.get("paper-1")).toMatchObject({
      stage: "generate_pages",
      status: "failed",
      error: "generator crashed",
    });
    store.close();
  });

  it("allows newer persisted state to replace an earlier persisted seed", () => {
    const store = createJobsStore();
    store.seed("paper-1", job());
    store.seed(
      "paper-1",
      job({
        status: "running",
        progress: 0.6,
        error_message: null,
        retry_count: 2,
      }),
    );
    expect(store.get("paper-1")).toMatchObject({
      status: "running",
      progress: 0.6,
      error: null,
    });
  });

  it("deduplicates concurrent subscription setup", async () => {
    const registration = deferred<() => void>();
    onPipeline.mockReturnValue(registration.promise);
    const store = createJobsStore();

    const first = store.init();
    const second = store.init();
    expect(onPipeline).toHaveBeenCalledTimes(1);

    registration.resolve(() => {});
    await Promise.all([first, second]);
    store.close();
  });

  it("disposes a listener that resolves after close", async () => {
    const registration = deferred<() => void>();
    const stop = vi.fn();
    onPipeline.mockReturnValue(registration.promise);
    const store = createJobsStore();

    const opening = store.init();
    store.close();
    registration.resolve(stop);
    await opening;

    expect(stop).toHaveBeenCalledTimes(1);
    expect(get(store).byPaper).toEqual({});
  });
});
