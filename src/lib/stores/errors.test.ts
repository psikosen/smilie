import { describe, expect, it } from "vitest";
import { errorMessage } from "./errors";

describe("errorMessage", () => {
  it("keeps Error messages readable and serializes structured failures", () => {
    expect(errorMessage(new Error("disk full"))).toBe("disk full");
    expect(errorMessage({ code: "SQLITE_BUSY" })).toBe(
      '{"code":"SQLITE_BUSY"}',
    );
    expect(errorMessage({ message: "database locked" })).toBe(
      "database locked",
    );
    expect(errorMessage({ error: "model unavailable" })).toBe(
      "model unavailable",
    );
  });
});
