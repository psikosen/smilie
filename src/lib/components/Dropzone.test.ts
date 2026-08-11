import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Dropzone from "./Dropzone.svelte";

type DropHandler = (paths: string[]) => void;

const mocks = vi.hoisted(() => ({
  ingestPdf: vi.fn(),
  refresh: vi.fn(),
  dragInit: vi.fn(),
  unsubscribe: vi.fn(),
  dropHandler: null as DropHandler | null,
}));

vi.mock("svelte", async () => {
  const { pathToFileURL } = await import("node:url");
  const clientUrl = pathToFileURL(
    `${process.cwd()}/node_modules/svelte/src/index-client.js`,
  );
  return import(/* @vite-ignore */ clientUrl.href);
});

vi.mock("$lib/ipc", () => ({
  ipc: {
    ingestPdf: (...args: unknown[]) => mocks.ingestPdf(...args),
  },
}));

vi.mock("$lib/ipc/tauri-guard", () => ({
  isTauri: () => false,
}));

vi.mock("$lib/stores/papers", () => ({
  papers: {
    refresh: (...args: unknown[]) => mocks.refresh(...args),
  },
}));

vi.mock("$lib/stores/viewport", () => ({
  viewport: {
    subscribe: (run: (value: { mobile: boolean; coarsePointer: boolean }) => void) => {
      run({ mobile: false, coarsePointer: false });
      return () => {};
    },
  },
}));

vi.mock("$lib/stores/drag", () => ({
  drag: {
    subscribe: (run: (value: { hovering: boolean }) => void) => {
      run({ hovering: false });
      return () => {};
    },
    init: (...args: unknown[]) => mocks.dragInit(...args),
    onDrop: (handler: DropHandler) => {
      mocks.dropHandler = handler;
      return mocks.unsubscribe;
    },
  },
}));

describe("Dropzone", () => {
  beforeEach(() => {
    mocks.ingestPdf.mockReset();
    mocks.refresh.mockReset();
    mocks.dragInit.mockReset();
    mocks.unsubscribe.mockReset();
    mocks.dropHandler = null;
    mocks.refresh.mockResolvedValue(undefined);
  });

  afterEach(cleanup);

  it("imports valid PDFs from a mixed batch and reports skipped files", async () => {
    mocks.ingestPdf
      .mockRejectedValueOnce(new Error("OCR service unavailable"))
      .mockResolvedValueOnce(undefined);

    render(Dropzone);
    await waitFor(() => expect(mocks.dropHandler).not.toBeNull());

    mocks.dropHandler?.([
      "/tmp/notes.txt",
      "C:\\papers\\broken.pdf",
      "/tmp/working.pdf",
    ]);

    await waitFor(() => expect(mocks.refresh).toHaveBeenCalledTimes(1));
    expect(mocks.ingestPdf).toHaveBeenNthCalledWith(1, "C:\\papers\\broken.pdf");
    expect(mocks.ingestPdf).toHaveBeenNthCalledWith(2, "/tmp/working.pdf");

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("1 non-PDF file was skipped");
    expect(alert.textContent).toContain("broken.pdf: OCR service unavailable");
    expect(alert.textContent).not.toContain("C:\\papers");
    expect(screen.getByRole("status").textContent).toContain("Paper imported");
  });

  it("ignores additional drops while an import is active", async () => {
    let finishImport: (() => void) | undefined;
    mocks.ingestPdf.mockImplementation(
      () => new Promise<void>((resolve) => {
        finishImport = resolve;
      }),
    );

    render(Dropzone);
    await waitFor(() => expect(mocks.dropHandler).not.toBeNull());

    mocks.dropHandler?.(["/tmp/first.pdf"]);
    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "Importing 1 of 1" }),
      ).toHaveProperty("disabled", true);
    });

    mocks.dropHandler?.(["/tmp/second.pdf"]);
    expect(mocks.ingestPdf).toHaveBeenCalledTimes(1);

    finishImport?.();
    await waitFor(() => expect(mocks.refresh).toHaveBeenCalledTimes(1));
  });

  it("explains that browser preview cannot open the native picker", async () => {
    render(Dropzone);

    await fireEvent.click(
      screen.getByRole("button", {
        name: "Choose one or more scientific PDF files",
      }),
    );

    expect(screen.getByText(/open the tauri app to drop files/i)).toBeTruthy();
  });
});
