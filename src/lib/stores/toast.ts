/**
 * Toast notifications. Tiny pub-sub list with auto-dismiss timers.
 *
 * Usage:
 *   toast.success("Exported");
 *   toast.error("Export failed");
 *   toast.info("Hint", { duration: 4000 });
 *
 * The component (`Toast.svelte`) subscribes to `toasts` and renders a
 * stack. Removal is timer-driven; consumers can also call `dismiss(id)`
 * for explicit close interactions.
 */

import { writable, type Readable } from "svelte/store";

export type ToastKind = "success" | "error" | "info";

export interface ToastEntry {
  id: number;
  kind: ToastKind;
  message: string;
  ts: number;
}

export interface ToastOptions {
  /** Override the default 2500ms auto-dismiss. Pass 0 to disable. */
  duration?: number;
}

const DEFAULT_DURATION = 2500;

const { subscribe, update } = writable<ToastEntry[]>([]);

let counter = 0;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

function schedule(id: number, duration: number) {
  if (duration <= 0) return;
  // Skip timers under jsdom — vitest doesn't run them and we don't want
  // dangling setTimeout handles in tests.
  if (typeof window === "undefined") return;
  const t = setTimeout(() => dismiss(id), duration);
  timers.set(id, t);
}

function push(kind: ToastKind, message: string, opts?: ToastOptions): number {
  const id = ++counter;
  const entry: ToastEntry = { id, kind, message, ts: Date.now() };
  update((list) => [...list, entry]);
  schedule(id, opts?.duration ?? DEFAULT_DURATION);
  return id;
}

function dismiss(id: number) {
  const t = timers.get(id);
  if (t) {
    clearTimeout(t);
    timers.delete(id);
  }
  update((list) => list.filter((e) => e.id !== id));
}

function clear() {
  for (const t of timers.values()) clearTimeout(t);
  timers.clear();
  update(() => []);
}

export const toasts: Readable<ToastEntry[]> = { subscribe };

export const toast = {
  success: (message: string, opts?: ToastOptions) => push("success", message, opts),
  error: (message: string, opts?: ToastOptions) => push("error", message, opts),
  info: (message: string, opts?: ToastOptions) => push("info", message, opts),
  dismiss,
  clear,
};
