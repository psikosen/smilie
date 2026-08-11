import { describe, it, expect } from "vitest";
import {
  READING_LEVELS,
  levelLabel,
  levelDescription,
  pendingLabel,
  deriveSegmentState,
} from "./level-toggle";

describe("level-toggle helpers", () => {
  it("READING_LEVELS is ordered eli5 → default → expert", () => {
    expect(READING_LEVELS).toEqual(["eli5", "default", "expert"]);
  });

  it("levelLabel returns the user-facing short label per level", () => {
    expect(levelLabel("eli5")).toBe("ELI5");
    expect(levelLabel("default")).toBe("Default");
    expect(levelLabel("expert")).toBe("Expert");
  });

  it("levelDescription returns a non-empty tooltip blurb per level", () => {
    for (const lvl of READING_LEVELS) {
      const d = levelDescription(lvl);
      expect(typeof d).toBe("string");
      expect(d.length).toBeGreaterThan(10);
    }
  });

  it("pendingLabel mentions the pretty label", () => {
    expect(pendingLabel("eli5")).toBe("Generating ELI5…");
    expect(pendingLabel("default")).toBe("Generating Default…");
    expect(pendingLabel("expert")).toBe("Generating Expert…");
  });

  describe("deriveSegmentState", () => {
    it("marks the current level as active", () => {
      expect(
        deriveSegmentState("default", "default", ["default"], null),
      ).toBe("active");
    });

    it("active wins over pending for the current level", () => {
      // We just optimistically swapped to default; even though it's also
      // 'pending' in some sense, the page IS rendering it now → active.
      expect(
        deriveSegmentState("default", "default", ["default"], "default"),
      ).toBe("active");
    });

    it("marks a pendingLevel that isn't current as pending", () => {
      expect(
        deriveSegmentState("expert", "default", ["default"], "expert"),
      ).toBe("pending");
    });

    it("marks a cached non-current level as cached", () => {
      expect(
        deriveSegmentState("eli5", "default", ["default", "eli5"], null),
      ).toBe("cached");
    });

    it("marks a non-cached, non-pending level as available", () => {
      expect(
        deriveSegmentState("expert", "default", ["default"], null),
      ).toBe("available");
    });

    it("treats pending as priority over cached when level not yet active", () => {
      // Edge: cached_levels contains the requested level (cache hit), but
      // we haven't yet seen the paper-update event flip current_level.
      // Optimistic UI shows the pending shimmer rather than 'cached'.
      expect(
        deriveSegmentState("eli5", "default", ["default", "eli5"], "eli5"),
      ).toBe("pending");
    });
  });
});
