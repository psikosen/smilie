import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// We mock @tauri-apps/api/core to control invoke per-test. Because
// resolveRunnerUrl uses dynamic `import("@tauri-apps/api/core")`, the
// mock is hoisted by vitest before the dynamic import resolves.
const invokeMock = vi.fn();
const convertFileSrcMock = vi.fn(
  (path: string, protocol: string) =>
    `http://${protocol}.localhost/${encodeURI(path).replace(/^\//, "")}`,
);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  convertFileSrc: (path: string, protocol: string) =>
    convertFileSrcMock(path, protocol),
}));

import {
  resolveRunnerUrl,
  RUNNER_COMMAND_MISSING_ERROR,
  shortInputsHash,
} from "./runner-url";

describe("resolveRunnerUrl", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    convertFileSrcMock.mockClear();
    // Simulate Tauri runtime: tauri-guard checks for the marker.
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      metadata: {},
    };
  });

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("returns null/null when not running under Tauri", async () => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    const res = await resolveRunnerUrl();
    expect(res).toEqual({ url: null, error: null });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("wraps the raw FS path returned by experiment_runner_url with convertFileSrc", async () => {
    // The Rust command returns a filesystem path. The iframe cannot
    // load that directly — `convertFileSrc(_, "asset")` must wrap it.
    invokeMock.mockResolvedValueOnce(
      "/Users/me/Library/Application Support/com.smilie.app/resources/assets/experiment-runner.html",
    );
    const res = await resolveRunnerUrl();
    expect(invokeMock).toHaveBeenCalledWith("experiment_runner_url");
    expect(convertFileSrcMock).toHaveBeenCalledWith(
      "/Users/me/Library/Application Support/com.smilie.app/resources/assets/experiment-runner.html",
      "asset",
    );
    expect(res.url).toBe(
      "http://asset.localhost/Users/me/Library/Application%20Support/com.smilie.app/resources/assets/experiment-runner.html",
    );
    expect(res.error).toBeNull();
  });

  it("passes already-formed asset URLs through unchanged", async () => {
    // Belt-and-braces: if the Rust side ever returns a pre-wrapped URL
    // (forward compat), don't double-wrap it.
    invokeMock.mockResolvedValueOnce("http://asset.localhost/x.html");
    const res = await resolveRunnerUrl();
    expect(res.url).toBe("http://asset.localhost/x.html");
    expect(convertFileSrcMock).not.toHaveBeenCalled();
  });

  it("returns the rebuild-required error when the command throws", async () => {
    invokeMock.mockRejectedValueOnce(new Error("command not found"));
    const res = await resolveRunnerUrl();
    expect(res.url).toBeNull();
    // Should be the canonical message + invoke error appended.
    expect(res.error).toContain(RUNNER_COMMAND_MISSING_ERROR);
    expect(res.error).toContain("command not found");
  });

  it("returns the rebuild-required error when the command resolves to empty string", async () => {
    invokeMock.mockResolvedValueOnce("");
    const res = await resolveRunnerUrl();
    expect(res.url).toBeNull();
    expect(res.error).toBe(RUNNER_COMMAND_MISSING_ERROR);
  });
});

describe("shortInputsHash", () => {
  it("returns a 6-char hex string", () => {
    const h = shortInputsHash({ a: 1 });
    expect(h).toMatch(/^[0-9a-f]{6}$/);
  });

  it("is stable for equivalent inputs", () => {
    expect(shortInputsHash({ a: 1, b: 2 })).toBe(shortInputsHash({ a: 1, b: 2 }));
  });

  it("differs for different inputs", () => {
    expect(shortInputsHash({ a: 1 })).not.toBe(shortInputsHash({ a: 2 }));
  });
});
