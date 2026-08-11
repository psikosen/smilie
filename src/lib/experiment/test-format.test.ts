/**
 * Locks the contract between the model-emitted Python test module and
 * the worker-side discovery logic in `src-tauri/assets/experiment-worker.js`
 * (`_smilie_run_tests`). The worker discovers tests by looking for
 * `def test_<name>():` at module-top level and ignores bare top-level
 * `assert` statements.
 *
 * If you change the discovery logic on either side, update the JS
 * mirror below + this test set in lockstep.
 */

import { describe, it, expect } from "vitest";

/**
 * Mirror of the worker's discovery rule. We don't claim to parse Python
 * — we only need to count top-level `def test_*` declarations the same
 * way the worker does. Worker uses Python's `globals()` after `exec`,
 * which tolerates indentation, decorators, etc. — but for the
 * round-trip contract we only care that test sources structured per the
 * spec produce the right *number* and *names* of test functions.
 *
 * Heuristic:
 *  - A line is a test definition if (after stripping leading spaces) it
 *    matches `^def test_[A-Za-z0-9_]*\s*\(` AND the original line had
 *    NO leading whitespace (top-level only).
 *  - `assert` statements at the top level (no indent) are flagged as
 *    contract violations.
 */
export interface DiscoveryReport {
  testNames: string[];
  bareTopLevelAsserts: number;
}

const TEST_DEF_RE = /^def\s+(test_[A-Za-z0-9_]*)\s*\(/;

export function discoverTests(source: string): DiscoveryReport {
  const names: string[] = [];
  let bareAsserts = 0;
  for (const raw of source.split(/\r?\n/)) {
    // Skip indented lines — only top-level statements count.
    if (raw.length > 0 && (raw[0] === " " || raw[0] === "\t")) continue;
    const line = raw.trimEnd();
    const match = TEST_DEF_RE.exec(line);
    if (match) {
      names.push(match[1]);
      continue;
    }
    if (/^assert\b/.test(line)) bareAsserts += 1;
  }
  return { testNames: names, bareTopLevelAsserts: bareAsserts };
}

describe("test-format discovery (worker contract mirror)", () => {
  it("finds a single def test_* at top level", () => {
    const src = [
      "def add(a, b):",
      "    return a + b",
      "",
      "def test_basic():",
      "    assert add(1, 2) == 3",
      "",
    ].join("\n");
    expect(discoverTests(src)).toEqual({
      testNames: ["test_basic"],
      bareTopLevelAsserts: 0,
    });
  });

  it("finds multiple test_* functions in declaration order", () => {
    const src = [
      "def test_one():",
      "    assert True",
      "",
      "def test_two():",
      "    assert 1 + 1 == 2",
      "",
      "def helper():",
      "    return 42",
      "",
      "def test_three():",
      "    assert helper() == 42",
      "",
    ].join("\n");
    const { testNames } = discoverTests(src);
    expect(testNames).toEqual(["test_one", "test_two", "test_three"]);
  });

  it("ignores indented def test_* (must be top-level)", () => {
    const src = [
      "class Suite:",
      "    def test_nested(self):",
      "        assert True",
      "",
      "def test_top():",
      "    assert True",
    ].join("\n");
    expect(discoverTests(src).testNames).toEqual(["test_top"]);
  });

  it("flags bare top-level asserts as a contract violation", () => {
    const src = [
      "import math",
      "assert math.sqrt(4) == 2",
      "",
      "def test_real():",
      "    assert math.sqrt(9) == 3",
    ].join("\n");
    const report = discoverTests(src);
    expect(report.testNames).toEqual(["test_real"]);
    // The contract says "NO bare top-level asserts". Worker doesn't
    // *enforce* this directly — Python will execute the assert at
    // module-load time and either pass or raise — but we surface it
    // here so a contract regression on the model side is caught.
    expect(report.bareTopLevelAsserts).toBe(1);
  });

  it("ignores indented assert statements", () => {
    const src = [
      "def test_x():",
      "    assert True",
      "    if False:",
      "        assert False",
    ].join("\n");
    const report = discoverTests(src);
    expect(report.testNames).toEqual(["test_x"]);
    expect(report.bareTopLevelAsserts).toBe(0);
  });

  it("returns no test functions when the module has none (worker reports 'no test_* functions found')", () => {
    const src = [
      "def helper():",
      "    return 0",
      "",
      "def setup():",
      "    pass",
    ].join("\n");
    expect(discoverTests(src).testNames).toEqual([]);
  });

  it("treats a name like test_ (just the prefix) as a discovered test", () => {
    // Matches the worker's `name.startswith('test_')` rule.
    const src = "def test_():\n    assert True\n";
    expect(discoverTests(src).testNames).toEqual(["test_"]);
  });
});
