import { describe, it, expect } from "vitest";
import { get } from "svelte/store";
import { searchQuery } from "./search";

describe("searchQuery store", () => {
  it("starts empty", () => {
    searchQuery.set("");
    expect(get(searchQuery)).toBe("");
  });

  it("propagates updates", () => {
    searchQuery.set("attention");
    expect(get(searchQuery)).toBe("attention");
    searchQuery.set("");
    expect(get(searchQuery)).toBe("");
  });
});
