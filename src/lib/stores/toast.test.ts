import { describe, it, expect, beforeEach } from "vitest";
import { get } from "svelte/store";
import { toast, toasts } from "./toast";

describe("toast store", () => {
  beforeEach(() => {
    toast.clear();
  });

  it("starts empty", () => {
    expect(get(toasts)).toEqual([]);
  });

  it("pushes a success entry with the given message", () => {
    const id = toast.success("Saved");
    const list = get(toasts);
    expect(list).toHaveLength(1);
    expect(list[0]).toMatchObject({ id, kind: "success", message: "Saved" });
  });

  it("supports error and info kinds", () => {
    toast.error("Boom");
    toast.info("FYI");
    const list = get(toasts);
    expect(list.map((e) => e.kind)).toEqual(["error", "info"]);
  });

  it("dismiss removes a single entry by id", () => {
    const a = toast.info("a");
    const b = toast.info("b");
    toast.dismiss(a);
    const list = get(toasts);
    expect(list).toHaveLength(1);
    expect(list[0].id).toBe(b);
  });

  it("clear empties the list", () => {
    toast.success("x");
    toast.success("y");
    toast.clear();
    expect(get(toasts)).toEqual([]);
  });
});
