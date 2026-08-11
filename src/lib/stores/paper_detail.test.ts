import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type {
  JobSummary,
  PaperDetail,
  PaperId,
  PipelineEventPayload,
} from "$lib/ipc";

const getPaper = vi.fn();
const onPaperUpdated = vi.fn();
const onPipeline = vi.fn();
const seed = vi.fn();

vi.mock("$lib/ipc", () => ({
  ipc: {
    getPaper: (...args: unknown[]) => getPaper(...args),
  },
  events: {
    onPaperUpdated: (callback: unknown) => onPaperUpdated(callback),
    onPipeline: (callback: unknown) => onPipeline(callback),
  },
}));

vi.mock("$lib/stores/jobs", () => ({
  jobs: { seed: (...args: unknown[]) => seed(...args) },
}));

import { createPaperDetailStore } from "./paper_detail";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function detail(id: PaperId, activeJob: JobSummary | null = null): PaperDetail {
  return {
    paper: {
      id,
      title: id,
      authors: [],
      page_count: 1,
      processing_status: "complete",
      created_at: 0,
      updated_at: 0,
      thumbnail_path: null,
    },
    pages: [],
    active_job: activeJob,
  };
}

describe("paper detail store", () => {
  beforeEach(() => {
    getPaper.mockReset();
    onPaperUpdated.mockReset();
    onPipeline.mockReset();
    seed.mockReset();
    onPaperUpdated.mockResolvedValue(() => {});
    onPipeline.mockResolvedValue(() => {});
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  });

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("does not repopulate state when a load resolves after close", async () => {
    const pending = deferred<PaperDetail>();
    getPaper.mockReturnValue(pending.promise);
    const store = createPaperDetailStore();

    const opening = store.open("paper-a");
    store.close();
    pending.resolve(detail("paper-a"));
    await opening;

    expect(get(store)).toEqual({
      paper: null,
      loading: false,
      error: null,
    });
    expect(seed).not.toHaveBeenCalled();
  });

  it("does not let a slower paper overwrite the current paper", async () => {
    const first = deferred<PaperDetail>();
    getPaper.mockImplementation((id: string) =>
      id === "paper-a" ? first.promise : Promise.resolve(detail("paper-b")),
    );
    const store = createPaperDetailStore();

    const openingA = store.open("paper-a");
    const openingB = store.open("paper-b");
    await openingB;
    first.resolve(detail("paper-a"));
    await openingA;

    expect(get(store).paper?.paper.id).toBe("paper-b");
    expect(seed).toHaveBeenCalledTimes(1);
    expect(seed).toHaveBeenCalledWith("paper-b", null);
    store.close();
  });

  it("disposes listeners that resolve after close", async () => {
    const paperRegistration = deferred<() => void>();
    const pipelineRegistration = deferred<() => void>();
    const stopPaper = vi.fn();
    const stopPipeline = vi.fn();
    onPaperUpdated.mockReturnValue(paperRegistration.promise);
    onPipeline.mockReturnValue(pipelineRegistration.promise);
    getPaper.mockResolvedValue(detail("paper-a"));
    const store = createPaperDetailStore();

    const opening = store.open("paper-a");
    store.close();
    paperRegistration.resolve(stopPaper);
    pipelineRegistration.resolve(stopPipeline);
    await opening;

    expect(stopPaper).toHaveBeenCalledTimes(1);
    expect(stopPipeline).toHaveBeenCalledTimes(1);
    expect(get(store).paper).toBeNull();
  });

  it("preserves loaded pages when a background refresh fails", async () => {
    let notify!: (event: { payload: string }) => void;
    onPaperUpdated.mockImplementation((callback: typeof notify) => {
      notify = callback;
      return Promise.resolve(() => {});
    });
    getPaper
      .mockResolvedValueOnce(detail("paper-a"))
      .mockRejectedValueOnce(new Error("database busy"));
    const store = createPaperDetailStore();
    await store.open("paper-a");

    notify({ payload: "paper-a" });
    await vi.waitFor(() => expect(get(store).loading).toBe(false));

    expect(get(store).paper?.paper.id).toBe("paper-a");
    expect(get(store).error).toBe("database busy");
    store.close();
  });

  it("rolls back a partial listener setup so the same paper can retry", async () => {
    const stopPaper = vi.fn();
    onPaperUpdated
      .mockResolvedValueOnce(stopPaper)
      .mockResolvedValueOnce(() => {});
    onPipeline
      .mockRejectedValueOnce(new Error("event plugin unavailable"))
      .mockResolvedValueOnce(() => {});
    getPaper.mockResolvedValue(detail("paper-a"));
    vi.spyOn(console, "error").mockImplementation(() => {});
    const store = createPaperDetailStore();

    await store.open("paper-a");
    await store.open("paper-a");

    expect(stopPaper).toHaveBeenCalledTimes(1);
    expect(onPaperUpdated).toHaveBeenCalledTimes(2);
    expect(onPipeline).toHaveBeenCalledTimes(2);
    store.close();
    vi.restoreAllMocks();
  });

  it("refreshes the matching paper when a terminal event lacks paper_id", async () => {
    let emit!: (event: { payload: PipelineEventPayload }) => void;
    onPipeline.mockImplementation((callback: typeof emit) => {
      emit = callback;
      return Promise.resolve(() => {});
    });
    const runningJob: JobSummary = {
      id: "job-a",
      paper_id: "paper-a",
      stage: "generate_pages",
      status: "running",
      progress: 0.5,
      current_model: null,
      error_message: null,
      retry_count: 0,
    };
    getPaper
      .mockResolvedValueOnce(detail("paper-a", runningJob))
      .mockResolvedValueOnce(
        detail("paper-a", {
          ...runningJob,
          status: "failed",
          error_message: "generator crashed",
        }),
      );
    const store = createPaperDetailStore();
    await store.open("paper-a");

    emit({
      payload: {
        type: "stage_failed",
        job_id: "job-a",
        stage: "generate_pages",
        error: "generator crashed",
      },
    });
    await vi.waitFor(() => expect(getPaper).toHaveBeenCalledTimes(2));

    expect(get(store).paper?.active_job?.status).toBe("failed");
    store.close();
  });
});
