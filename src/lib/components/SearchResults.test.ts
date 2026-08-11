import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SearchResults from "./SearchResults.svelte";

const mocks = vi.hoisted(() => ({
  searchPages: vi.fn(),
  goto: vi.fn(),
}));

vi.mock("$lib/ipc", () => ({
  ipc: {
    searchPages: mocks.searchPages,
  },
}));

vi.mock("$lib/ipc/tauri-guard", () => ({
  isTauri: () => true,
}));

vi.mock("$app/navigation", () => ({
  goto: mocks.goto,
}));

function page(id: string, title: string) {
  return {
    id,
    paper_id: "paper/one",
    title,
    type: "concept",
    source_sections: ["Methods"],
    source_pages: [4],
    html_path: `/tmp/${id}.html`,
    status: "ready",
    validation_status: "valid",
    order_index: 0,
    current_level: "default",
    cached_levels: ["default"],
    created_at: 0,
    updated_at: 0,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("SearchResults", () => {
  beforeEach(() => {
    mocks.searchPages.mockReset();
    mocks.goto.mockReset();
  });

  it("ignores a stale response from an older query", async () => {
    const older = deferred<ReturnType<typeof page>[]>();
    const newer = deferred<ReturnType<typeof page>[]>();
    mocks.searchPages
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(newer.promise);

    const view = render(SearchResults, { query: "alpha" });
    await waitFor(() => expect(mocks.searchPages).toHaveBeenCalledWith("alpha"));

    await view.rerender({ query: "beta" });
    await waitFor(() => expect(mocks.searchPages).toHaveBeenCalledWith("beta"));

    newer.resolve([page("beta", "Beta concept")]);
    await screen.findByText("Beta concept");

    older.resolve([page("alpha", "Old alpha result")]);
    await waitFor(() => {
      expect(screen.queryByText("Old alpha result")).toBeNull();
      expect(screen.getByText("Beta concept")).toBeTruthy();
    });
  });

  it("clears an in-flight loading state when the query becomes too short", async () => {
    mocks.searchPages.mockReturnValue(new Promise(() => {}));
    const view = render(SearchResults, { query: "attention" });

    await screen.findByText("Searching saved pages…");
    await view.rerender({ query: "a" });

    expect(screen.getByText("Type at least 2 characters.")).toBeTruthy();
    expect(screen.queryByText("Searching saved pages…")).toBeNull();
  });

  it("encodes paper and page identifiers in result links", async () => {
    mocks.searchPages.mockResolvedValue([page("page/two", "Encoded concept")]);
    render(SearchResults, { query: "encoded" });

    await fireEvent.click(await screen.findByRole("button", {
      name: /Encoded concept/,
    }));

    expect(mocks.goto).toHaveBeenCalledWith(
      "/papers/paper%2Fone/?page=page%2Ftwo",
    );
  });
});
