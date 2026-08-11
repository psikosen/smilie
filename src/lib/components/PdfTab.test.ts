import { render, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import PdfTab from "./PdfTab.svelte";

const mocks = vi.hoisted(() => ({
  getPaperPdfUrl: vi.fn(),
  openPaperInDefaultApp: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("$lib/ipc", () => ({
  ipc: {
    getPaperPdfUrl: mocks.getPaperPdfUrl,
    openPaperInDefaultApp: mocks.openPaperInDefaultApp,
  },
}));

vi.mock("$lib/ipc/tauri-guard", () => ({
  isTauri: () => true,
}));

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset:${path}`,
}));

vi.mock("$lib/stores/toast", () => ({
  toast: {
    error: mocks.toastError,
  },
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("PdfTab", () => {
  beforeEach(() => {
    mocks.getPaperPdfUrl.mockReset();
    mocks.openPaperInDefaultApp.mockReset();
    mocks.toastError.mockReset();
  });

  it("resolves the initial PDF exactly once", async () => {
    mocks.getPaperPdfUrl.mockResolvedValue("/papers/one.pdf");
    const view = render(PdfTab, { paperId: "paper-one" });

    await waitFor(() => {
      expect(mocks.getPaperPdfUrl).toHaveBeenCalledTimes(1);
      expect(view.container.querySelector("object")?.getAttribute("data")).toBe(
        "asset:/papers/one.pdf",
      );
    });
  });

  it("does not let an older paper request replace the current PDF", async () => {
    const older = deferred<string>();
    const newer = deferred<string>();
    mocks.getPaperPdfUrl
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(newer.promise);

    const view = render(PdfTab, { paperId: "paper-one" });
    await waitFor(() => {
      expect(mocks.getPaperPdfUrl).toHaveBeenCalledWith("paper-one");
    });

    await view.rerender({ paperId: "paper-two" });
    await waitFor(() => {
      expect(mocks.getPaperPdfUrl).toHaveBeenCalledWith("paper-two");
    });

    newer.resolve("/papers/two.pdf");
    await waitFor(() => {
      expect(view.container.querySelector("object")?.getAttribute("data")).toBe(
        "asset:/papers/two.pdf",
      );
    });

    older.resolve("/papers/one.pdf");
    await waitFor(() => {
      expect(view.container.querySelector("object")?.getAttribute("data")).toBe(
        "asset:/papers/two.pdf",
      );
    });
  });
});
