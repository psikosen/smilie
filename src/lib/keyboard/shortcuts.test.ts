import { describe, it, expect } from "vitest";
import { classify, isEditableTarget } from "./shortcuts";

function key(
  init: KeyboardEventInit & { target?: EventTarget | null },
): KeyboardEvent {
  const ev = new KeyboardEvent("keydown", { cancelable: true, ...init });
  if (init.target) {
    Object.defineProperty(ev, "target", { value: init.target });
  }
  return ev;
}

describe("isEditableTarget", () => {
  it("flags inputs and textareas", () => {
    const input = document.createElement("input");
    const ta = document.createElement("textarea");
    expect(isEditableTarget(input)).toBe(true);
    expect(isEditableTarget(ta)).toBe(true);
  });

  it("flags descendants of contenteditable regions", () => {
    const editor = document.createElement("div");
    editor.setAttribute("contenteditable", "true");
    const child = document.createElement("span");
    editor.append(child);
    expect(isEditableTarget(child)).toBe(true);
  });

  it("returns false for plain elements and null", () => {
    expect(isEditableTarget(document.createElement("div"))).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});

describe("classify", () => {
  it("maps Esc to 'esc'", () => {
    expect(classify(key({ key: "Escape" }))).toBe("esc");
  });

  it("maps Cmd+K to 'focus-search'", () => {
    expect(classify(key({ key: "k", metaKey: true }))).toBe("focus-search");
    expect(classify(key({ key: "K", ctrlKey: true }))).toBe("focus-search");
  });

  it("maps Cmd+, to 'open-settings'", () => {
    expect(classify(key({ key: ",", metaKey: true }))).toBe("open-settings");
  });

  it("maps J/K and arrows to page-prev/page-next", () => {
    expect(classify(key({ key: "j" }))).toBe("page-next");
    expect(classify(key({ key: "ArrowDown" }))).toBe("page-next");
    expect(classify(key({ key: "k" }))).toBe("page-prev");
    expect(classify(key({ key: "ArrowUp" }))).toBe("page-prev");
  });

  it("inside an input, only Esc and Cmd+K survive", () => {
    const input = document.createElement("input");
    expect(classify(key({ key: "Escape", target: input }))).toBe("esc");
    expect(classify(key({ key: "k", metaKey: true, target: input }))).toBe(
      "focus-search",
    );
    expect(classify(key({ key: "j", target: input }))).toBe(null);
    expect(classify(key({ key: "ArrowDown", target: input }))).toBe(null);
    expect(classify(key({ key: ",", metaKey: true, target: input }))).toBe(null);
  });

  it("Cmd+J should NOT be page-next (modifier guard)", () => {
    expect(classify(key({ key: "j", metaKey: true }))).toBe(null);
  });

  it("does not intercept composing, handled, or shifted key events", () => {
    expect(classify(key({ key: "j", isComposing: true }))).toBe(null);
    const handled = key({ key: "j" });
    handled.preventDefault();
    expect(classify(handled)).toBe(null);
    expect(classify(key({ key: "ArrowDown", shiftKey: true }))).toBe(null);
    expect(classify(key({ key: "J", shiftKey: true }))).toBe(null);
  });
});
