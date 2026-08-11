/**
 * Pure keyboard-shortcut classifier. Layout passes a KeyboardEvent in,
 * gets back a tagged action string. Kept side-effect free so tests can
 * assert on the mapping without mounting the component.
 *
 * "Editable target" rule: if the event originates inside an input,
 * textarea, or contenteditable region, only Esc and Cmd/Ctrl-K are
 * honored — everything else is left alone so typing works normally.
 */

export type ShortcutAction =
  | "esc"
  | "focus-search"
  | "open-settings"
  | "page-prev"
  | "page-next"
  | null;

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!target || typeof target !== "object") return false;
  const element = target as {
    closest?: (selector: string) => Element | null;
    isContentEditable?: boolean;
  };
  if (element.isContentEditable) return true;
  return (
    typeof element.closest === "function" &&
    element.closest(
      'input, textarea, select, [contenteditable]:not([contenteditable="false"])',
    ) !== null
  );
}

/** Returns the matched action, or `null` if the event should pass through. */
export function classify(ev: KeyboardEvent): ShortcutAction {
  if (ev.defaultPrevented || ev.isComposing) return null;

  const mod = ev.metaKey || ev.ctrlKey;
  const editable = isEditableTarget(ev.target);

  // Esc — universal close. Allowed in editable targets too.
  if (ev.key === "Escape") return "esc";

  // Cmd/Ctrl + K — focus the search input. Allowed in editable targets
  // (we WANT this to work when focus is currently inside another input).
  if (mod && !ev.shiftKey && !ev.altKey && (ev.key === "k" || ev.key === "K")) {
    return "focus-search";
  }

  // Inside inputs / contenteditable, swallow nothing else.
  if (editable) return null;

  // Cmd/Ctrl + , — settings.
  if (mod && !ev.shiftKey && !ev.altKey && ev.key === ",") {
    return "open-settings";
  }

  // Bare-key navigation (no modifiers; we don't want Cmd+J to fire).
  if (!mod && !ev.altKey && !ev.shiftKey) {
    if (ev.key === "ArrowUp" || ev.key === "k" || ev.key === "K") {
      return "page-prev";
    }
    if (ev.key === "ArrowDown" || ev.key === "j" || ev.key === "J") {
      return "page-next";
    }
  }

  return null;
}

/** Custom event name TopBar listens for in response to focus-search. */
export const FOCUS_SEARCH_EVENT = "smilie:focus-search";
