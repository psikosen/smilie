import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const onDragDropEvent = vi.fn();

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent }),
}));

import { createDragStore } from "./drag";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("drag store", () => {
  beforeEach(() => {
    onDragDropEvent.mockReset();
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    vi.restoreAllMocks();
  });

  it("retries initialization after listener setup fails", async () => {
    const stop = vi.fn();
    onDragDropEvent
      .mockRejectedValueOnce(new Error("webview not ready"))
      .mockResolvedValueOnce(stop);
    const store = createDragStore();

    await store.init();
    await store.init();

    expect(onDragDropEvent).toHaveBeenCalledTimes(2);
    store.teardown();
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("disposes a listener that resolves after teardown", async () => {
    const registration = deferred<() => void>();
    const stop = vi.fn();
    onDragDropEvent.mockReturnValue(registration.promise);
    const store = createDragStore();

    const opening = store.init();
    store.teardown();
    registration.resolve(stop);
    await opening;

    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("continues delivering a drop when one handler throws", async () => {
    let emit!: (event: {
      payload: { type: "drop"; paths: string[] };
    }) => void;
    onDragDropEvent.mockImplementation((callback: typeof emit) => {
      emit = callback;
      return Promise.resolve(() => {});
    });
    const store = createDragStore();
    const next = vi.fn();
    store.onDrop(() => {
      throw new Error("consumer failed");
    });
    store.onDrop(next);
    await store.init();

    emit({ payload: { type: "drop", paths: ["/paper.pdf"] } });

    expect(next).toHaveBeenCalledWith(["/paper.pdf"]);
    store.teardown();
  });
});
