import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const listen = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listen(...args),
}));

import {
  events,
  ipc,
  IpcCommandError,
  TauriUnavailableError,
} from "./index";

describe("IPC runtime boundary", () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("rejects commands predictably in browser preview", async () => {
    await expect(ipc.listPapers()).rejects.toBeInstanceOf(
      TauriUnavailableError,
    );
    expect(invoke).not.toHaveBeenCalled();
  });

  it("makes event subscriptions a safe no-op in browser preview", async () => {
    const stop = await events.onPaperUpdated(() => {});
    stop();
    expect(listen).not.toHaveBeenCalled();
  });

  it("delegates commands in Tauri and normalizes string failures", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    invoke.mockRejectedValue("database unavailable");

    await expect(ipc.getPaper("paper-1")).rejects.toMatchObject({
      name: "IpcCommandError",
      command: "get_paper",
      message: "get_paper failed: database unavailable",
    } satisfies Partial<IpcCommandError>);
    expect(invoke).toHaveBeenCalledWith("get_paper", { id: "paper-1" });
  });

  it("normalizes event subscription failures with the channel name", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    listen.mockRejectedValue(new Error("event plugin offline"));

    await expect(events.onPipeline(() => {})).rejects.toMatchObject({
      name: "IpcCommandError",
      command: "listen:pipeline",
      message: "listen:pipeline failed: event plugin offline",
    });
  });
});
