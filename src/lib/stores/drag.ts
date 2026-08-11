/**
 * Single-source drag-drop coordinator. Both `Dropzone` and `DragOverlay`
 * subscribe rather than each registering their own webview listener
 * (Tauri allows multiple, but every listener has to disable the OS
 * default and we want one call site for the file-handling side-effect).
 *
 * The store holds:
 *   - `hovering`: are we currently being dragged over (for visual chrome)
 *   - subscribers can register a `drop` handler to receive paths
 *
 * The actual `getCurrentWebview().onDragDropEvent` listener is attached
 * lazily on the first `init()` call and reused for the rest of the app's
 * lifetime.
 */

import { writable } from "svelte/store";
import { isTauri } from "$lib/ipc/tauri-guard";

export interface DragState {
  hovering: boolean;
}

export type DropHandler = (paths: string[]) => void;

export function createDragStore() {
  const { subscribe, set, update } = writable<DragState>({
    hovering: false,
  });
  let unlisten: (() => void) | null = null;
  let initPromise: Promise<void> | null = null;
  let lifecycle = 0;
  const handlers = new Set<DropHandler>();

  function init(): Promise<void> {
    if (unlisten) return Promise.resolve();
    if (initPromise) return initPromise;
    if (!isTauri()) return Promise.resolve();

    const token = lifecycle;
    initPromise = (async () => {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        const webview = getCurrentWebview();
        const stop = await webview.onDragDropEvent((ev) => {
          if (token !== lifecycle) return;
          if (ev.payload.type === "over") {
            update((s) => ({ ...s, hovering: true }));
          } else if (ev.payload.type === "leave") {
            update((s) => ({ ...s, hovering: false }));
          } else if (ev.payload.type === "drop") {
            update((s) => ({ ...s, hovering: false }));
            const paths = Array.isArray(ev.payload.paths)
              ? ev.payload.paths.filter(
                  (path): path is string => typeof path === "string",
                )
              : [];
            for (const handler of handlers) {
              try {
                handler(paths);
              } catch (error) {
                console.error("drop handler failed", error);
              }
            }
          }
        });
        if (token !== lifecycle) {
          stop();
          return;
        }
        unlisten = stop;
      } catch (error) {
        if (token === lifecycle) {
          console.error("drag-drop subscription failed", error);
        }
      }
    })().finally(() => {
      if (token === lifecycle) initPromise = null;
    });
    return initPromise;
  }

  function onDrop(fn: DropHandler): () => void {
    handlers.add(fn);
    return () => handlers.delete(fn);
  }

  function teardown() {
    lifecycle += 1;
    unlisten?.();
    unlisten = null;
    initPromise = null;
    handlers.clear();
    set({ hovering: false });
  }

  return { subscribe, init, onDrop, teardown };
}

export const drag = createDragStore();
