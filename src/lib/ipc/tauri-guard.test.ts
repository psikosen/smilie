import { describe, it, expect, afterEach } from "vitest";
import { isTauri } from "./tauri-guard";

describe("isTauri", () => {
  afterEach(() => {
    if ("__TAURI_INTERNALS__" in window) {
      delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    }
  });

  it("returns false in plain browsers", () => {
    expect(isTauri()).toBe(false);
  });

  it("returns true when the Tauri marker is present", () => {
    // @ts-expect-error test injection
    window.__TAURI_INTERNALS__ = { metadata: {} };
    expect(isTauri()).toBe(true);
  });

  it("rejects null or undefined marker values", () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ =
      undefined;
    expect(isTauri()).toBe(false);
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = null;
    expect(isTauri()).toBe(false);
  });
});
