//! Typed pipeline events emitted to the frontend.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

pub const PIPELINE_EVENT: &str = "pipeline";
pub const PAPER_UPDATED_EVENT: &str = "paper:updated";
pub const EXPERIMENT_EVENT: &str = "experiment";
pub const COMPARISON_EVENT: &str = "comparison";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PipelineEvent {
    StageStarted {
        job_id: String,
        paper_id: String,
        stage: String,
    },
    StageProgress {
        job_id: String,
        stage: String,
        progress: f64,
    },
    StageCompleted {
        job_id: String,
        stage: String,
    },
    StageFailed {
        job_id: String,
        stage: String,
        error: String,
    },
    PageReady {
        page_id: String,
        paper_id: String,
    },
}

// Emit functions are generic over the Tauri runtime so the integration
// smoke test can drive the pipeline with `tauri::test::MockRuntime`. The
// production call sites continue to compile as `AppHandle<Wry>` thanks
// to the default param on `AppHandle`.
pub fn emit_pipeline<R: Runtime>(handle: &AppHandle<R>, ev: &PipelineEvent) {
    if let Err(e) = handle.emit(PIPELINE_EVENT, ev) {
        tracing::warn!(?e, "emit pipeline event failed");
    }
}

pub fn emit_paper_updated<R: Runtime>(handle: &AppHandle<R>, paper_id: &str) {
    if let Err(e) = handle.emit(PAPER_UPDATED_EVENT, paper_id) {
        tracing::warn!(?e, "emit paper updated failed");
    }
}

/// Pipeline-event payload emitted on the `experiment` channel. Matches
/// the `ExperimentEventPayload` TS shape in `src/lib/ipc/index.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExperimentEvent {
    ExperimentStatus {
        experiment_id: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        score: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

pub fn emit_experiment<R: Runtime>(handle: &AppHandle<R>, ev: &ExperimentEvent) {
    if let Err(e) = handle.emit(EXPERIMENT_EVENT, ev) {
        tracing::warn!(?e, "emit experiment event failed");
    }
}

/// Pipeline-event payload emitted on the `comparison` channel. Matches
/// the `ComparisonEventPayload` TS shape in `src/lib/ipc/index.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComparisonEvent {
    ComparisonStatus {
        comparison_id: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

pub fn emit_comparison<R: Runtime>(handle: &AppHandle<R>, ev: &ComparisonEvent) {
    if let Err(e) = handle.emit(COMPARISON_EVENT, ev) {
        tracing::warn!(?e, "emit comparison event failed");
    }
}
