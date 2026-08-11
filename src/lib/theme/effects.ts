/**
 * Dynamic lighting effects for the Night-Tokyo theme.
 * - Cursor-follow glow (updates --mx / --my CSS custom props)
 * - Shimmer sweep loop registration
 * - Reduced-motion guard
 *
 * Kept small + dependency-free so pages stay snappy.
 */

const PREFERS_REDUCED_MOTION = () =>
  typeof window !== "undefined" &&
  window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;

export interface CursorGlowOptions {
  /** Multiplier applied to --mx-strength on pointer move. 0..1. */
  strength?: number;
  /** Decay time in ms after pointer leaves before strength returns to 0. */
  fadeMs?: number;
}

/**
 * Attach a cursor-follow glow to an element. The element (and its descendants
 * that reference --mx / --my / --mx-strength) will light up near the pointer.
 *
 * Returns a cleanup function.
 */
export function attachCursorGlow(
  el: HTMLElement,
  opts: CursorGlowOptions = {},
): () => void {
  if (PREFERS_REDUCED_MOTION()) {
    el.style.setProperty("--mx-strength", "0");
    return () => {};
  }

  const strength = opts.strength ?? 1;
  const fadeMs = opts.fadeMs ?? 260;
  let raf = 0;
  let pendingX = 50;
  let pendingY = 50;
  let fadeTimer: ReturnType<typeof setTimeout> | undefined;

  const apply = () => {
    raf = 0;
    el.style.setProperty("--mx", String(pendingX));
    el.style.setProperty("--my", String(pendingY));
  };

  const onMove = (ev: PointerEvent) => {
    const rect = el.getBoundingClientRect();
    pendingX = ((ev.clientX - rect.left) / rect.width) * 100;
    pendingY = ((ev.clientY - rect.top) / rect.height) * 100;
    el.style.setProperty("--mx-strength", String(strength));
    if (fadeTimer) clearTimeout(fadeTimer);
    if (!raf) raf = requestAnimationFrame(apply);
  };

  const onLeave = () => {
    if (fadeTimer) clearTimeout(fadeTimer);
    fadeTimer = setTimeout(() => {
      el.style.setProperty("--mx-strength", "0");
    }, fadeMs);
  };

  el.addEventListener("pointermove", onMove, { passive: true });
  el.addEventListener("pointerleave", onLeave, { passive: true });

  return () => {
    el.removeEventListener("pointermove", onMove);
    el.removeEventListener("pointerleave", onLeave);
    if (raf) cancelAnimationFrame(raf);
    if (fadeTimer) clearTimeout(fadeTimer);
  };
}

/** Svelte action form — usage: `<div use:cursorGlow={{strength:1}}>` */
export function cursorGlow(node: HTMLElement, opts: CursorGlowOptions = {}) {
  let destroy = attachCursorGlow(node, opts);
  return {
    update(newOpts: CursorGlowOptions) {
      destroy();
      destroy = attachCursorGlow(node, newOpts);
    },
    destroy() {
      destroy();
    },
  };
}
