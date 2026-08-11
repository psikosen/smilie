//! Persistence + event helpers shared by every experiment stage.
//!
//! Status FSM labels (mirror of `migrations/004_experiments.sql`) live
//! here so other stages — analyze, generate, test_refine — import a
//! single source of truth.

use anyhow::Result;
use tauri::{AppHandle, Runtime};

use crate::db::{queries, Db};
use crate::events::{emit_experiment, ExperimentEvent};
use crate::runtime::ExperimentArtifacts;

/// Status labels surfaced via `emit_experiment` for the UI's progress
/// indicator. Mirror of the FSM in `migrations/004_experiments.sql`.
pub mod status {
    pub const ANALYZING: &str = "analyzing";
    pub const IMPLAUSIBLE: &str = "implausible";
    pub const GENERATING: &str = "generating";
    pub const TESTING: &str = "testing";
    pub const READY: &str = "ready";
    pub const FAILED: &str = "failed";
}

/// Write artifacts (code, tests, schemas, attempts, last_output) back
/// to the experiments row. Used after every (re)generation pass.
pub async fn persist_artifacts(
    db: &Db,
    experiment_id: &str,
    artifacts: &ExperimentArtifacts,
    attempts: i64,
    last_output: Option<&str>,
) -> Result<()> {
    let inputs = serde_json::to_string(&artifacts.inputs_schema)?;
    let outputs = serde_json::to_string(&artifacts.outputs_schema)?;
    queries::update_experiment_artifacts(
        db,
        experiment_id,
        &inputs,
        &outputs,
        &artifacts.python_code,
        &artifacts.python_tests,
        attempts,
        last_output,
        None,
    )
    .await?;
    Ok(())
}

/// Fire-and-forget emit of the `experiment_status` event used by the
/// UI's progress chip + version-list indicator.
pub fn emit_status<R: Runtime>(
    handle: &AppHandle<R>,
    experiment_id: &str,
    status: &str,
    score: Option<u8>,
    error: Option<String>,
) {
    emit_experiment(
        handle,
        &ExperimentEvent::ExperimentStatus {
            experiment_id: experiment_id.to_string(),
            status: status.to_string(),
            score,
            error,
        },
    );
}
