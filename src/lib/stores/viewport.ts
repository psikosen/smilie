/**
 * Reactive viewport classification (desktop vs. mobile) backed by
 * `matchMedia`. Used by mobile-aware components so we don't duplicate
 * the breakpoint constant or the listener boilerplate.
 *
 * The breakpoint matches the Library / paper-detail @media queries
 * (720px) — keep them in sync if you change one.
 */

import { readable } from "svelte/store";

export const MOBILE_BREAKPOINT_PX = 720;

function compute(): { mobile: boolean; coarsePointer: boolean } {
  if (
    typeof window === "undefined" ||
    typeof window.matchMedia !== "function"
  ) {
    return { mobile: false, coarsePointer: false };
  }
  return {
    mobile: window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT_PX}px)`).matches,
    coarsePointer: window.matchMedia("(pointer: coarse)").matches,
  };
}

export const viewport = readable(compute(), (set) => {
  if (
    typeof window === "undefined" ||
    typeof window.matchMedia !== "function"
  ) {
    set({ mobile: false, coarsePointer: false });
    return () => {};
  }
  const widthMql = window.matchMedia(
    `(max-width: ${MOBILE_BREAKPOINT_PX}px)`,
  );
  const pointerMql = window.matchMedia("(pointer: coarse)");
  const update = () => set(compute());
  update();
  widthMql.addEventListener("change", update);
  pointerMql.addEventListener("change", update);
  return () => {
    widthMql.removeEventListener("change", update);
    pointerMql.removeEventListener("change", update);
  };
});
