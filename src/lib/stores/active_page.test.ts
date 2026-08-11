import { describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import type { PageRecord } from "$lib/ipc";
import { activePage } from "./active_page";

function page(id: string, status: PageRecord["status"] = "ready"): PageRecord {
  return {
    id,
    paper_id: "paper-1",
    title: id,
    type: "informational",
    source_sections: [],
    source_pages: [],
    html_path: `/${id}.html`,
    status,
    validation_status: "valid",
    order_index: 0,
    current_level: "default",
    cached_levels: ["default"],
    created_at: 0,
    updated_at: 0,
  };
}

describe("activePage", () => {
  it("advances its snapshot so consecutive shortcut steps do not repeat", () => {
    const select = vi.fn();
    activePage.set({
      pages: [page("one"), page("two"), page("three")],
      activePageId: "one",
      setActivePageId: select,
    });

    activePage.step(1);
    activePage.step(1);

    expect(select.mock.calls).toEqual([["two"], ["three"]]);
    expect(get(activePage).activePageId).toBe("three");
    activePage.clear();
  });

  it("skips pages that are not ready", () => {
    const select = vi.fn();
    activePage.set({
      pages: [page("one"), page("pending", "generating"), page("three")],
      activePageId: "one",
      setActivePageId: select,
    });

    activePage.step(1);

    expect(select).toHaveBeenCalledWith("three");
    activePage.clear();
  });
});
