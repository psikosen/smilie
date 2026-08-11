/**
 * Iframe URL resolution for the experiment runner.
 *
 * Calls the Rust command `experiment_runner_url` (which returns a
 * filesystem path to `src-tauri/assets/experiment-runner.html`) and
 * wraps it with `convertFileSrc(_, "asset")` so the WebView can
 * actually load it via the Tauri asset protocol — on macOS this is
 * `http://asset.localhost/...`. On any failure we surface a clear
 * error string for the runner component to render as a banner.
 *
 * Returns null in browser dev mode (no Tauri).
 *
 * The pre-rendered `convertFileSrc` URL is what we hand to
 * `<iframe src>`; raw paths cannot be loaded by the WebView under our
 * `assetProtocol.scope` rules. See `PageViewer.svelte` for the same
 * pattern on the learning-pages side.
 */
import { isTauri } from "$lib/ipc/tauri-guard";

export interface RunnerUrlResult {
  url: string | null;
  error: string | null;
}

export const RUNNER_COMMAND_MISSING_ERROR =
  "Tauri command experiment_runner_url not available — rebuild the app";

export async function resolveRunnerUrl(): Promise<RunnerUrlResult> {
  if (!isTauri()) return { url: null, error: null };

  try {
    const { invoke, convertFileSrc } = await import("@tauri-apps/api/core");
    const path = await invoke<string>("experiment_runner_url");
    if (typeof path === "string" && path.length > 0) {
      // Always wrap. If the Rust side ever evolves to return a
      // pre-formatted asset URL, `convertFileSrc` is a no-op on inputs
      // that already start with the protocol prefix — but today it
      // returns a raw FS path and the wrap is load-bearing.
      const url = path.startsWith("http://asset")
        ? path
        : convertFileSrc(path, "asset");
      return { url, error: null };
    }
    return { url: null, error: RUNNER_COMMAND_MISSING_ERROR };
  } catch (err) {
    // Most common failure: the command isn't registered on the Rust
    // side yet (developer running an older binary). Surface that
    // clearly so the user knows to rebuild instead of seeing a generic
    // "invoke failed" string.
    const detail = err instanceof Error ? err.message : String(err);
    return {
      url: null,
      error: `${RUNNER_COMMAND_MISSING_ERROR} (${detail})`,
    };
  }
}

/**
 * Stable 32-bit FNV-1a hash over a JSON-serialisable value, emitted as
 * a 6-char hex string. Used for run-history chip labels — not a
 * cryptographic primitive.
 */
export function shortInputsHash(value: unknown): string {
  const s = JSON.stringify(value);
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0").slice(0, 6);
}
