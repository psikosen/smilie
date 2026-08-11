/**
 * Pure helpers for the reading-level segmented toggle in the PageViewer
 * chrome. Kept in its own module (no Svelte deps) so the formatting +
 * cached-vs-pending derivation can be unit-tested without mounting a
 * component.
 */

import type { ReadingLevel } from "$lib/ipc";

/** Order matters — drives the rendered segment order in the toggle. */
export const READING_LEVELS: ReadingLevel[] = ["eli5", "default", "expert"];

/** Short label shown inside each segment. */
export function levelLabel(level: ReadingLevel): string {
  switch (level) {
    case "eli5":
      return "ELI5";
    case "default":
      return "Default";
    case "expert":
      return "Expert";
  }
}

/** Long-form description used in the (i) tooltip. */
export function levelDescription(level: ReadingLevel): string {
  switch (level) {
    case "eli5":
      return "Plain-language summary you could read to a curious 5th grader.";
    case "default":
      return "Balanced explanation aimed at a technically-literate reader.";
    case "expert":
      return "Dense, jargon-on form aimed at someone in the field.";
  }
}

/** "Generating Default…" pill copy when a regen is in flight. */
export function pendingLabel(level: ReadingLevel): string {
  return `Generating ${levelLabel(level)}…`;
}

export type SegmentState =
  | "active"   // currently rendered level
  | "cached"   // ready to switch instantly
  | "pending"  // requested but the regen is in flight
  | "available"; // not cached and not pending — clicking kicks regen

/**
 * Derive the per-segment state for the toggle.
 *
 * Inputs:
 *   currentLevel — what the page is currently rendering (server truth)
 *   cachedLevels — levels with persisted HTML on disk
 *   pendingLevel — level the user just clicked but hasn't landed yet
 *
 * Rules:
 *   - The currentLevel is always "active" (regardless of pendingLevel).
 *   - A pendingLevel that isn't currentLevel is "pending".
 *   - Anything in cachedLevels is "cached".
 *   - Otherwise "available" (will trigger a regen).
 */
export function deriveSegmentState(
  segment: ReadingLevel,
  currentLevel: ReadingLevel,
  cachedLevels: ReadingLevel[],
  pendingLevel: ReadingLevel | null,
): SegmentState {
  if (segment === currentLevel) return "active";
  if (pendingLevel && segment === pendingLevel) return "pending";
  if (cachedLevels.includes(segment)) return "cached";
  return "available";
}
