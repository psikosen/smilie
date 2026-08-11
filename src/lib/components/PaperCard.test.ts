import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaperSummary } from "$lib/ipc";
import PaperCard from "./PaperCard.svelte";

const mocks = vi.hoisted(() => ({
  deletePaper: vi.fn(),
  getPaper: vi.fn(),
  resumeJob: vi.fn(),
  refresh: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
  info: vi.fn(),
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
    deletePaper: (...args: unknown[]) => mocks.deletePaper(...args),
    getPaper: (...args: unknown[]) => mocks.getPaper(...args),
    resumeJob: (...args: unknown[]) => mocks.resumeJob(...args),
  },
}));

vi.mock("$lib/stores/papers", () => ({
  papers: {
    refresh: (...args: unknown[]) => mocks.refresh(...args),
  },
}));

vi.mock("$lib/stores/toast", () => ({
  toast: {
    success: (...args: unknown[]) => mocks.success(...args),
    error: (...args: unknown[]) => mocks.error(...args),
    info: (...args: unknown[]) => mocks.info(...args),
  },
}));

vi.mock("$lib/theme/effects", () => ({
  cursorGlow: () => ({ destroy: () => {} }),
}));

function paper(patch: Partial<PaperSummary> = {}): PaperSummary {
  return {
    id: "paper-1",
    title: "Neural Rendering at Night",
    authors: ["A. Researcher", "B. Scientist"],
    page_count: 1,
    processing_status: "complete",
    created_at: 1,
    updated_at: 1,
    thumbnail_path: null,
    ...patch,
  };
}

describe("PaperCard", () => {
  beforeEach(() => {
    for (const mock of Object.values(mocks)) mock.mockReset();
    mocks.refresh.mockResolvedValue(undefined);
  });

  afterEach(cleanup);

  it("uses a dedicated open button with sibling actions", async () => {
    const onOpen = vi.fn();
    render(PaperCard, { paper: paper(), onOpen });

    const article = screen.getByRole("article", {
      name: "Neural Rendering at Night",
    });
    const open = screen.getByRole("button", {
      name: "Neural Rendering at Night",
    });
    const remove = screen.getByRole("button", {
      name: "Delete Neural Rendering at Night",
    });

    expect(article.contains(open)).toBe(true);
    expect(open.contains(remove)).toBe(false);
    expect(screen.getByText("1 page")).toBeTruthy();

    await fireEvent.click(open);
    expect(onOpen).toHaveBeenCalledWith("paper-1");
  });

  it("resumes a failed job without opening the paper", async () => {
    const onOpen = vi.fn();
    mocks.getPaper.mockResolvedValue({
      paper: paper({ processing_status: "failed" }),
      pages: [],
      active_job: { id: "job-7" },
    });
    mocks.resumeJob.mockResolvedValue(undefined);

    render(PaperCard, {
      paper: paper({ processing_status: "failed" }),
      onOpen,
    });

    await fireEvent.click(
      screen.getByRole("button", {
        name: "Resume processing for Neural Rendering at Night",
      }),
    );

    await waitFor(() => expect(mocks.resumeJob).toHaveBeenCalledWith("job-7"));
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("opens explicit deletion confirmation without opening the paper", async () => {
    const onOpen = vi.fn();
    render(PaperCard, { paper: paper(), onOpen });

    await fireEvent.click(
      screen.getByRole("button", {
        name: "Delete Neural Rendering at Night",
      }),
    );

    expect(screen.getByRole("dialog", { name: "Delete this paper?" })).toBeTruthy();
    expect(screen.getByText(/all generated pages will be removed/i)).toBeTruthy();
    expect(onOpen).not.toHaveBeenCalled();
  });
});
