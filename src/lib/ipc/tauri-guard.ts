/**
 * Browser dev fallback: detect whether the app is running inside a Tauri
 * webview. Lets `npm run dev` load the UI in a plain browser for visual work
 * without crashing on missing IPC.
 */
export const isTauri = (): boolean =>
  typeof window !== "undefined" &&
  typeof (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ ===
    "object" &&
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !== null;
