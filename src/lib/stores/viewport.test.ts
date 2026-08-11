import { afterEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

const originalMatchMedia = window.matchMedia;

describe("viewport store", () => {
  afterEach(() => {
    vi.resetModules();
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: originalMatchMedia,
    });
  });

  it("falls back safely when matchMedia is unavailable", async () => {
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: undefined,
    });

    const { viewport } = await import("./viewport");
    expect(get(viewport)).toEqual({ mobile: false, coarsePointer: false });

  });

  it("recomputes when a new subscriber starts after an idle gap", async () => {
    let mobile = false;
    const listeners = new Set<() => void>();
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn((query: string) => ({
        get matches() {
          return query.startsWith("(max-width") ? mobile : false;
        },
        media: query,
        onchange: null,
        addEventListener: (_type: string, listener: () => void) =>
          listeners.add(listener),
        removeEventListener: (_type: string, listener: () => void) =>
          listeners.delete(listener),
        addListener: vi.fn(),
        removeListener: vi.fn(),
        dispatchEvent: vi.fn(),
      })),
    });

    const { viewport } = await import("./viewport");
    const stop = viewport.subscribe(() => {});
    stop();
    mobile = true;

    expect(get(viewport).mobile).toBe(true);
  });
});
