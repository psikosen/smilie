/**
 * Smilie Experiment Worker — Pyodide-backed Python sandbox.
 *
 * Compatible with Pyodide 0.27.x (vendored via scripts/install_pyodide.sh).
 * Bump in lockstep with that script's PYODIDE_VERSION default.
 *
 * Lives inside a Web Worker spawned by experiment-runner.html. The
 * iframe is a pure relay; this worker is where the policy lives.
 *
 * Protocol — see docs/plans/2026-04-26-experiments-design.md §5.
 *
 *   Host → Worker
 *     { type: "init" }
 *     { type: "run",       runId, code, inputs }
 *     { type: "run_tests", runId, code, tests }
 *     { type: "install",   runId, package }
 *
 *   Worker → Host
 *     { type: "loading",      progress }      // 0..1, only during init
 *     { type: "ready" }                       // pyodide loaded
 *     { type: "log",          level, line }
 *     { type: "result",       runId, outputs }
 *     { type: "tests_result", runId, passed, report }
 *     { type: "error",        runId, message }
 */

"use strict";

// ── State ───────────────────────────────────────────────────────
// Single Pyodide instance per worker. Initialized lazily on first
// `init`. Re-init is idempotent (we ignore subsequent `init` msgs).
let pyodide = null;
let initPromise = null;

// Snapshot of the post-init globals namespace (keys only). Captured
// once after NORMALIZE_PY runs. We use this to reset globals before
// each `run` / `run_tests` so user code from a previous run does NOT
// leak into the next one (e.g. a stale `def helper(): ...` shadowing
// a fresh definition, or a global `state` dict accumulating).
//
// Why snapshot keys (not values): the value snapshot would be huge
// (numpy module objects etc.) and cloning isn't free. Deleting any
// keys that didn't exist post-init is enough — it preserves all the
// pre-imported modules and our `_smilie_*` helpers untouched.
let baselineGlobalKeys = null;

// ── Boot ────────────────────────────────────────────────────────
function emitLoading(progress) {
  postMessage({ type: "loading", progress });
}

function log(level, line) {
  postMessage({ type: "log", level, line: String(line) });
}

/**
 * Load Pyodide + pre-pinned packages.
 *
 * Progress reporting is approximate — Pyodide does not expose a real
 * byte-counter, so we tick a series of milestones (importScripts done,
 * runtime loaded, numpy installed) and interpolate.
 */
async function bootPyodide() {
  if (pyodide) return pyodide;
  if (initPromise) return initPromise;

  initPromise = (async () => {
    emitLoading(0.02);

    // pyodide.js sits next to this worker in src-tauri/assets/pyodide/.
    // importScripts is synchronous — fine inside a worker, blocks for
    // the ~600KB JS shim only (the WASM is loaded by loadPyodide).
    try {
      // eslint-disable-next-line no-undef
      importScripts("./pyodide/pyodide.js");
    } catch (e) {
      // Most likely cause: the install script wasn't run, so the
      // bundle is missing. Surface a hint instead of a cryptic
      // ReferenceError on `loadPyodide`.
      throw new Error(
        "Failed to load ./pyodide/pyodide.js — did you run scripts/install_pyodide.sh? (" +
          (e && e.message) +
          ")"
      );
    }

    emitLoading(0.15);

    // The WASM + .data bundles are fetched relative to indexURL.
    // eslint-disable-next-line no-undef
    pyodide = await loadPyodide({
      indexURL: "./pyodide/",
      // Pipe Python stdout/stderr to the host as `log` events so
      // generated experiments can `print(...)` for debugging.
      stdout: (line) => log("info", line),
      stderr: (line) => log("warn", line),
    });

    emitLoading(0.55);

    // Pre-pinned package set per design §1. micropip is needed for
    // `install` messages later; numpy is a hard dependency for the
    // bulk of physics/biology experiments the model emits.
    await pyodide.loadPackage(["micropip", "numpy"]);

    emitLoading(0.95);

    // Install our normalization helper as a Python module so the
    // run-paths can call it via pyodide.runPython without re-defining
    // it every call.
    pyodide.runPython(NORMALIZE_PY);

    // Snapshot the baseline globals AFTER all our helpers are defined.
    // resetUserGlobals() will delete anything not in this set.
    baselineGlobalKeys = new Set(
      pyodide
        .runPython("list(globals().keys())")
        .toJs(),
    );

    emitLoading(1.0);
    postMessage({ type: "ready" });
    return pyodide;
  })();

  return initPromise;
}

// ── Python helpers ─────────────────────────────────────────────
//
// Defined once and exec'd into the global namespace at boot time.
// Kept inline so the worker is a single-file artifact (no extra
// asset to load). If this grows past ~80 lines, split it into
// experiment-runtime.py and bundle alongside.
const NORMALIZE_PY = `
import json
import traceback as _tb

def _smilie_normalize(value):
    """Recursively convert numpy / non-JSON scalars to plain Python."""
    try:
        import numpy as _np  # noqa: WPS433 — local import is fine, numpy is pre-loaded
    except ImportError:
        _np = None

    if _np is not None:
        if isinstance(value, _np.ndarray):
            return [_smilie_normalize(v) for v in value.tolist()]
        if isinstance(value, _np.generic):
            return value.item()
    if isinstance(value, dict):
        return {str(k): _smilie_normalize(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_smilie_normalize(v) for v in value]
    if isinstance(value, (bool, int, float, str)) or value is None:
        return value
    # Last resort: stringify. The output schema validator on the host
    # will flag this as a type mismatch and tell the user.
    return repr(value)

def _smilie_collect_outputs(globs):
    out = globs.get("OUTPUTS", {})
    if out is None:
        return {}
    if not isinstance(out, dict):
        # Author wrote OUTPUTS = something_else; coerce to a single-key
        # dict so we don't silently drop their value.
        return {"value": _smilie_normalize(out)}
    return _smilie_normalize(out)

def _smilie_run_tests(test_source):
    """Split a test module on top-level 'def test_*' and run each test
    function in isolation, capturing AssertionError per test.

    The test source is plain Python with assert statements — there is
    no pytest import (the model is instructed to write pytest-style
    standalone tests; we provide our own micro-runner)."""
    import re

    # Step 1: exec the whole module so functions are defined. The
    # caller is expected to have already exec'd the experiment code,
    # so any helpers it defines are available too.
    test_globals = dict(globals())
    try:
        exec(test_source, test_globals)
    except Exception as e:  # noqa: BLE001 — we want every error
        return False, "FAIL (module load): " + repr(e) + "\\n" + _tb.format_exc()

    # Step 2: discover test_* callables in the resulting globals.
    test_names = [
        name for name, val in test_globals.items()
        if name.startswith("test_") and callable(val)
    ]

    if not test_names:
        return False, "FAIL: no test_* functions found in test source"

    lines = []
    passed = 0
    failed = 0
    for name in sorted(test_names):
        try:
            test_globals[name]()
            passed += 1
            lines.append("PASS  " + name)
        except AssertionError as e:
            failed += 1
            lines.append("FAIL  " + name + ": " + (str(e) or "assertion"))
        except Exception as e:  # noqa: BLE001
            failed += 1
            # Include a one-line traceback excerpt so the model can
            # see what went wrong on the refine pass.
            tb = _tb.format_exc().strip().splitlines()
            tail = tb[-1] if tb else repr(e)
            lines.append("ERROR " + name + ": " + tail)

    summary = "{} passed, {} failed (of {})".format(passed, failed, passed + failed)
    report = summary + "\\n" + "\\n".join(lines)
    return failed == 0, report
`;

/**
 * Reset Pyodide globals to the post-init baseline.
 *
 * Removes any name introduced by previous user code so a Run can't
 * accidentally inherit state from a prior Run on the same worker.
 * The baseline (set in bootPyodide) preserves numpy, micropip, and
 * the `_smilie_*` helpers, so the next exec starts fresh but cheap —
 * no need to re-loadPackage().
 */
function resetUserGlobals() {
  if (!pyodide || !baselineGlobalKeys) return;
  const currentKeys = pyodide.runPython("list(globals().keys())").toJs();
  const toDelete = [];
  for (const k of currentKeys) {
    if (typeof k !== "string") continue;
    if (baselineGlobalKeys.has(k)) continue;
    // Skip CPython dunders we shouldn't touch (e.g. __builtins__).
    if (k.startsWith("__") && k.endsWith("__")) continue;
    toDelete.push(k);
  }
  if (toDelete.length === 0) return;
  // Delete in one shot via Python. JSON-encode names to dodge quoting.
  const py =
    "for _k in " +
    JSON.stringify(toDelete) +
    ":\n" +
    "    globals().pop(_k, None)\n" +
    "del _k\n";
  pyodide.runPython(py);
}

// ── Message handlers ───────────────────────────────────────────

/**
 * `run` — execute experiment code with INPUTS, collect OUTPUTS.
 *
 * Convention (documented in the model prompt):
 *   - `INPUTS` is a Python dict with the user's adjusted variables.
 *   - The experiment writes its results into a dict named `OUTPUTS`.
 *
 * Anything raised becomes an `error` message with the formatted
 * traceback. We never let an exception kill the worker.
 */
async function handleRun(msg) {
  const { runId, code, inputs } = msg;
  try {
    await bootPyodide();

    // Drop any names introduced by a previous run on this worker so
    // user code starts from a clean baseline (numpy/micropip/helpers
    // are preserved by the snapshot in bootPyodide).
    resetUserGlobals();

    // Push inputs as a Python dict literal. pyodide.toPy converts
    // a JS object → Python dict (or PyProxy, depending on version);
    // either way the experiment code can index it as INPUTS["foo"].
    const pyInputs = pyodide.toPy(inputs || {});
    pyodide.globals.set("INPUTS", pyInputs);
    // Reset OUTPUTS to a fresh dict so we don't carry stale keys
    // from a previous run on the same worker.
    pyodide.runPython("OUTPUTS = {}");

    // runPythonAsync supports top-level await, which experiments
    // may use for `await micropip.install(...)` if the model emits
    // a self-installing snippet.
    await pyodide.runPythonAsync(code);

    const outputs = pyodide
      .runPython("_smilie_collect_outputs(globals())")
      .toJs({ dict_converter: Object.fromEntries });

    postMessage({
      type: "result",
      runId,
      outputs: jsifyDeep(outputs),
    });
  } catch (e) {
    postMessage({
      type: "error",
      runId,
      message: formatError(e),
    });
  }
}

/**
 * `run_tests` — exec experiment code, then run a tests module against
 * it. The host uses this for the TDD refine loop (design §1, §2).
 *
 * Tests are arbitrary Python; we just look for `def test_*` and run
 * each. AssertionError counts as fail; other exceptions count as
 * error (also fail). Reported as a multi-line string so the model
 * can read it on the refine round-trip.
 */
async function handleRunTests(msg) {
  const { runId, code, tests } = msg;
  try {
    await bootPyodide();

    // Wipe user-introduced globals from any previous run before exec'ing
    // the experiment module + tests. Same reasoning as handleRun.
    resetUserGlobals();

    // Reset INPUTS/OUTPUTS to defaults — tests should not depend on
    // user-adjustable inputs (the test author writes deterministic
    // assertions against pure functions in `code`).
    pyodide.globals.set("INPUTS", pyodide.toPy({}));
    pyodide.runPython("OUTPUTS = {}");

    // Exec the experiment module so its functions land in the global
    // namespace the test runner snapshots.
    await pyodide.runPythonAsync(code);

    // Hand the test source to the Python harness. We pass it via a
    // global rather than f-string interpolation to avoid quoting hell
    // with multi-line tests containing backticks / triple quotes.
    pyodide.globals.set("__SMILIE_TEST_SRC__", tests);
    const result = pyodide
      .runPython("_smilie_run_tests(__SMILIE_TEST_SRC__)")
      .toJs();
    // Result is a (passed: bool, report: str) tuple → JS array.
    const passed = !!(result && result[0]);
    const report = result && result[1] != null ? String(result[1]) : "";

    postMessage({ type: "tests_result", runId, passed, report });
  } catch (e) {
    postMessage({
      type: "error",
      runId,
      message: formatError(e),
    });
  }
}

/**
 * `install` — pip-install a package via micropip. Only pure-Python
 * wheels and Pyodide-built C extensions are supported (this is a
 * micropip constraint, not ours).
 */
async function handleInstall(msg) {
  const { runId, package: pkg } = msg;
  if (!pkg || typeof pkg !== "string") {
    postMessage({
      type: "error",
      runId,
      message: "install: missing or invalid `package` field",
    });
    return;
  }
  // Reject anything that doesn't look like a bare package name to
  // avoid `micropip.install("foo'); rm -rf …")`-style injection.
  // Hyphens, dots, underscores, version specifiers are allowed.
  if (!/^[A-Za-z0-9_.\-=<>!~ ]+$/.test(pkg)) {
    postMessage({
      type: "error",
      runId,
      message: "install: package name contains disallowed characters",
    });
    return;
  }
  try {
    await bootPyodide();
    // JSON-encode the package string so quotes / backslashes can't
    // break out of the Python string literal.
    const py = `
import micropip
await micropip.install(${JSON.stringify(pkg)})
`;
    await pyodide.runPythonAsync(py);
    postMessage({
      type: "result",
      runId,
      outputs: { installed: pkg },
    });
  } catch (e) {
    postMessage({
      type: "error",
      runId,
      message: formatError(e),
    });
  }
}

// ── Output normalization (JS side) ─────────────────────────────
//
// The Python helper already converts numpy → list and dict-of-dict
// recursively. But pyodide.toJs may still leave us with TypedArrays
// (Float64Array, Int32Array, BigInt64Array) for numeric lists, and
// BigInts for large ints. The host's IPC layer + JSON serializer
// reject both. Walk and flatten.
function jsifyDeep(value) {
  if (value == null) return value;
  if (typeof value === "bigint") {
    // Lossy if > 2^53, but the design doc only supports number-typed
    // outputs, and the model is told to keep ints in safe range.
    return Number(value);
  }
  if (
    value instanceof Float32Array ||
    value instanceof Float64Array ||
    value instanceof Int8Array ||
    value instanceof Int16Array ||
    value instanceof Int32Array ||
    value instanceof Uint8Array ||
    value instanceof Uint16Array ||
    value instanceof Uint32Array
  ) {
    return Array.from(value);
  }
  if (value instanceof BigInt64Array || value instanceof BigUint64Array) {
    return Array.from(value, (v) => Number(v));
  }
  if (Array.isArray(value)) {
    return value.map(jsifyDeep);
  }
  // pyodide.toJs returns Maps for dicts when no dict_converter is
  // supplied — we DO supply Object.fromEntries above, so this is a
  // belt-and-braces fallback for any nested PyProxy that escapes.
  if (value instanceof Map) {
    const obj = {};
    for (const [k, v] of value) obj[String(k)] = jsifyDeep(v);
    return obj;
  }
  if (typeof value === "object") {
    // Plain object (already converted by dict_converter). Walk keys.
    const out = {};
    for (const k of Object.keys(value)) out[k] = jsifyDeep(value[k]);
    return out;
  }
  return value;
}

function formatError(e) {
  // Pyodide wraps Python exceptions in PythonError, whose .message
  // already contains the formatted traceback. Plain JS errors fall
  // back to .stack or .message.
  if (e && typeof e === "object") {
    if (e.message) return String(e.message);
    if (e.stack) return String(e.stack);
  }
  return String(e);
}

// ── Dispatcher ─────────────────────────────────────────────────
self.onmessage = function (ev) {
  const msg = ev.data;
  if (!msg || typeof msg !== "object") return;

  switch (msg.type) {
    case "init":
      // Fire-and-forget; emits `loading` + `ready` on its own.
      bootPyodide().catch((e) => {
        postMessage({
          type: "error",
          runId: "__init__",
          message: formatError(e),
        });
      });
      return;

    case "run":
      handleRun(msg);
      return;

    case "run_tests":
      handleRunTests(msg);
      return;

    case "install":
      handleInstall(msg);
      return;

    default:
      postMessage({
        type: "error",
        runId: msg.runId || "__unknown__",
        message: "unknown message type: " + String(msg.type),
      });
  }
};
