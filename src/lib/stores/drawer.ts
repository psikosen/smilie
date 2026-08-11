/**
 * Drawer/mobile-sheet close coordination.
 *
 * Layout-level Esc handling broadcasts `closeRequest` (a fresh Symbol so
 * subscribers always see a change even if Esc is hit twice in a row).
 * Sheets/drawers subscribe and clear their own open state.
 */

import { writable } from "svelte/store";

export const closeRequest = writable<symbol>(Symbol("init"));

/** Broadcast a close request. Mobile sheets watch this and self-close. */
export function requestCloseAll() {
  closeRequest.set(Symbol("close"));
}
