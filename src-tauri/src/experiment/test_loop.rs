//! Inner test-fix loop for the experiment pipeline.
//!
//! Pulled out of `pipeline.rs` so the orchestration there stays
//! readable. The loop owns three concerns:
//!   1. Run the model's tests through `python3` (when configured), using
//!      a harness that MIRRORS the Pyodide worker shape: discover every
//!      top-level `def test_*` callable and call each in isolation,
//!      collecting per-function `PASS` / `FAIL` / `ERROR` lines.
//!   2. On any failure, ask the runtime for a refined `(code, tests)`
//!      with the per-function detail fed back in.
//!   3. Cap the number of `(test → refine)` cycles.
//!
//! Pyodide is the long-term in-app runner (frontend iframe). The host
//! `python3` shell-out exists so the same contract is exercised on the
//! Rust side BEFORE shipping to the frontend, catching test-shape and
//! import drift early. Set `SMILIE_EXPERIMENT_PYTHON=""` to skip
//! server-side testing entirely.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tauri::{AppHandle, Runtime};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::db::{queries, Db};
use crate::ingest::structure::PaperJson;
use crate::runtime::{ExperimentArtifacts, PriorAttempt, RuntimeSelector};

use super::stages::{emit_status, persist_artifacts, status};

/// How many `(test → refine)` cycles we attempt before giving up.
/// Mirrors the design doc §2 ("up to 3 retries").
pub(super) const MAX_TEST_ATTEMPTS: i64 = 3;
/// Wall-clock budget for one `python3` test run.
pub(super) const TEST_TIMEOUT_SECS: u64 = 30;

pub(super) async fn run_test_and_refine_loop<R: Runtime>(
    db: &Db,
    runtime: &Arc<RuntimeSelector>,
    handle: &AppHandle<R>,
    experiment_id: &str,
    paper: PaperJson,
    initial: ExperimentArtifacts,
) -> Result<()> {
    let mut artifacts = initial;
    let mut attempts: i64 = 1;
    let python = configured_python();

    loop {
        let outcome = if let Some(py) = &python {
            run_tests_via_python(py, &artifacts.python_code, &artifacts.python_tests).await
        } else {
            // No `python3` available: trust the model's first output.
            // The frontend Pyodide runner surfaces failures and the
            // user can refine via the UI prompt path.
            TestOutcome::Skipped
        };

        match outcome {
            TestOutcome::Passed | TestOutcome::Skipped => {
                persist_artifacts(db, experiment_id, &artifacts, attempts, None).await?;
                queries::update_experiment_status(
                    db,
                    experiment_id,
                    status::READY,
                    None,
                    None,
                    None,
                    None,
                )
                .await?;
                emit_status(handle, experiment_id, status::READY, None, None);
                return Ok(());
            }
            TestOutcome::Failed { report } => {
                if attempts >= MAX_TEST_ATTEMPTS {
                    persist_artifacts(db, experiment_id, &artifacts, attempts, Some(&report))
                        .await?;
                    queries::update_experiment_status(
                        db,
                        experiment_id,
                        status::FAILED,
                        None,
                        None,
                        None,
                        Some(&format!("tests failed after {attempts} attempts")),
                    )
                    .await?;
                    emit_status(handle, experiment_id, status::FAILED, None, Some(report));
                    return Ok(());
                }

                attempts += 1;
                let prior = PriorAttempt {
                    code: artifacts.python_code.clone(),
                    tests: artifacts.python_tests.clone(),
                    error_output: report.clone(),
                    user_refine_prompt: None,
                };
                tracing::info!(experiment_id, attempts, "test failed, refining");
                artifacts = runtime
                    .generator
                    .refine_experiment(paper.clone(), &prior)
                    .await?;
                persist_artifacts(db, experiment_id, &artifacts, attempts, Some(&report)).await?;
            }
        }
    }
}

#[derive(Debug)]
pub(super) enum TestOutcome {
    Passed,
    /// `report` is the multi-line per-function status detail suitable
    /// for the refine prompt's `error_output` field.
    Failed {
        report: String,
    },
    Skipped,
}

pub(super) fn configured_python() -> Option<String> {
    let raw = std::env::var("SMILIE_EXPERIMENT_PYTHON").unwrap_or_else(|_| "python3".to_string());
    if raw.trim().is_empty() {
        None
    } else {
        Some(raw)
    }
}

/// The harness script source. Mirrors the Pyodide worker contract:
/// discover every `def test_*` callable in the module's namespace and
/// call each in isolation, collecting per-function status lines:
///
///   - `PASS <name>`               — function returned without raising
///   - `FAIL <name>: <message>`    — `AssertionError`
///   - `ERROR <name>: <message>`   — any other exception
///
/// Bare top-level `assert` failures abort module import; we surface
/// those as `MODULE_IMPORT_ERROR` so the refine prompt knows to wrap.
///
/// Lives in `harness.py` so the file stays valid python (lints OK,
/// editor highlighting works) and so `test_loop.rs` stays under the
/// 300-line cap from CLAUDE.md §1.
const HARNESS_PY: &str = include_str!("harness.py");

/// Run `code` then per-function `tests` through the configured python
/// interpreter using the harness above. Output is the multi-line
/// per-function report; the loop feeds this verbatim back to the model.
///
/// The child is reaped on timeout via `child.kill().await`; partial
/// stdout/stderr are still surfaced via the pre-spawned reader tasks
/// so the refine prompt has SOMETHING to act on. No process leaks.
async fn run_tests_via_python(python: &str, code: &str, tests: &str) -> TestOutcome {
    let driver = build_driver(code, tests);

    let mut cmd = Command::new(python);
    cmd.arg("-c")
        .arg(&driver)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            // Couldn't even spawn — treat as "skipped" so we don't
            // bounce around the refine loop on a missing interpreter.
            tracing::warn!(?e, "failed to spawn experiment python; skipping test loop");
            return TestOutcome::Skipped;
        }
    };

    // Pre-spawn readers so we capture partial output even when the
    // child has to be killed. `take()` detaches the handles from
    // `child` so we can later call `kill().await` without contention.
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf).await;
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        buf
    });

    let wait = tokio::time::timeout(Duration::from_secs(TEST_TIMEOUT_SECS), child.wait()).await;

    let exit_status = match wait {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            // I/O error reaping — try to kill anyway, surface the error.
            let _ = child.kill().await;
            let _ = child.wait().await;
            return TestOutcome::Failed {
                report: format!("python harness io error: {e}"),
            };
        }
        Err(_) => {
            // Timed out — kill, then await wait() to reap the zombie.
            // Without the second wait, the child becomes a defunct
            // process attached to our pid. With it, the OS releases
            // resources before we return.
            let _ = child.kill().await;
            let _ = child.wait().await;
            return TestOutcome::Failed {
                report: format!("python harness timed out after {TEST_TIMEOUT_SECS}s"),
            };
        }
    };

    let stdout_bytes = stdout_task.await.unwrap_or_default();
    let stderr_bytes = stderr_task.await.unwrap_or_default();
    let stdout_str = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stderr_str = String::from_utf8_lossy(&stderr_bytes).into_owned();

    if exit_status.success() {
        return TestOutcome::Passed;
    }

    // Build a single readable report for the refine prompt. stdout
    // carries the per-function status lines; stderr carries the
    // import / discovery diagnostics.
    let mut report = String::new();
    if !stdout_str.trim().is_empty() {
        report.push_str(stdout_str.trim_end());
        report.push('\n');
    }
    if !stderr_str.trim().is_empty() {
        report.push_str("---- stderr ----\n");
        report.push_str(stderr_str.trim_end());
        report.push('\n');
    }
    if report.trim().is_empty() {
        report = format!("python harness exited {} with no output", exit_status);
    }
    TestOutcome::Failed { report }
}

/// Splice the user's `code` and `tests` into the harness via JSON
/// string literals, side-stepping shell + python escaping concerns.
fn build_driver(code: &str, tests: &str) -> String {
    let code_lit = serde_json::to_string(code).unwrap_or_else(|_| "\"\"".into());
    let tests_lit = serde_json::to_string(tests).unwrap_or_else(|_| "\"\"".into());
    format!(
        "_USER_CODE = {code}\n_USER_TESTS = {tests}\n{harness}",
        code = code_lit,
        tests = tests_lit,
        harness = HARNESS_PY
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attempts_caps_at_three() {
        assert_eq!(MAX_TEST_ATTEMPTS, 3);
    }

    #[test]
    fn empty_python_env_disables_loop() {
        let key = "SMILIE_EXPERIMENT_PYTHON";
        let prev = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, "");
        }
        assert!(configured_python().is_none());
        unsafe {
            std::env::set_var(key, "python3");
        }
        assert_eq!(configured_python().as_deref(), Some("python3"));
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    /// Round-trip a simple passing test through the real harness. Skips
    /// when `python3` is unavailable (CI nodes without it). The point
    /// is to lock the per-function reporting contract — `OUTPUTS["x"]`
    /// is set in code, asserted in a `def test_*():` function, and the
    /// harness must report `PASS`.
    #[tokio::test]
    async fn harness_accepts_well_formed_test_module() {
        let Some(py) = which_python() else {
            return; // skip
        };
        let code = "OUTPUTS[\"x\"] = 7\n";
        let tests = "def test_x_is_seven():\n    assert OUTPUTS[\"x\"] == 7\n";
        let outcome = run_tests_via_python(&py, code, tests).await;
        assert!(
            matches!(outcome, TestOutcome::Passed),
            "expected Passed, got {outcome:?}",
        );
    }

    /// Bare top-level `assert` (the OLD contract we are deliberately
    /// rejecting) must surface as a Failed outcome, not silently pass.
    /// A model that returns this shape needs to be told to wrap.
    #[tokio::test]
    async fn harness_rejects_bare_top_level_asserts() {
        let Some(py) = which_python() else {
            return; // skip
        };
        let code = "OUTPUTS[\"x\"] = 1\n";
        // Top-level assert that PASSES — the harness must STILL fail
        // because no `def test_*` functions are defined.
        let tests = "assert OUTPUTS[\"x\"] == 1\n";
        let outcome = run_tests_via_python(&py, code, tests).await;
        match outcome {
            TestOutcome::Failed { report } => {
                assert!(
                    report.contains("NO_TESTS_FOUND") || report.contains("test_"),
                    "report should explain missing test_* fns; got: {report}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// Per-function failures must surface readable `FAIL <name>` lines
    /// in the report so the refine prompt can target the failing test.
    #[tokio::test]
    async fn harness_reports_per_function_failure() {
        let Some(py) = which_python() else {
            return; // skip
        };
        let code = "OUTPUTS[\"x\"] = 1\n";
        let tests = "def test_x_is_two():\n    assert OUTPUTS[\"x\"] == 2, \"expected 2\"\n";
        let outcome = run_tests_via_python(&py, code, tests).await;
        match outcome {
            TestOutcome::Failed { report } => {
                assert!(
                    report.contains("FAIL test_x_is_two"),
                    "expected FAIL line in report; got: {report}",
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    fn which_python() -> Option<String> {
        // Honour the override the same way `configured_python` does so
        // a CI runner can point at a specific interpreter or disable
        // these tests entirely (`SMILIE_EXPERIMENT_PYTHON=""`).
        let py = configured_python()?;
        // Quick `--version` smoke test so we don't wedge in `cargo
        // test` on hosts without the binary on PATH.
        let ok = std::process::Command::new(&py)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            Some(py)
        } else {
            None
        }
    }
}
