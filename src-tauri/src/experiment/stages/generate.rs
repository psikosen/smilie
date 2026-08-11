//! Stage 2 — initial generation + the refine-fork entrypoint.
//!
//! Two related codepaths live here because both produce the FIRST
//! `ExperimentArtifacts` for a row and both finish by transitioning the
//! row to `testing`:
//!   - `run_generate_stage`: fresh experiment, drives `generate_experiment`.
//!   - `run_refine_path`: fork from a parent experiment, drives
//!     `refine_experiment` with the user's prompt + parent code/tests.

use std::sync::Arc;

use anyhow::Result;
use tauri::{AppHandle, Runtime};

use super::save::{emit_status, persist_artifacts, status};
use crate::db::{queries, Db};
use crate::experiment::test_loop::run_test_and_refine_loop;
use crate::ingest::structure::PaperJson;
use crate::runtime::{ExperimentArtifacts, PriorAttempt, RuntimeSelector};

pub async fn run_generate_stage<R: Runtime>(
    db: &Db,
    runtime: &Arc<RuntimeSelector>,
    handle: &AppHandle<R>,
    experiment_id: &str,
    paper: PaperJson,
    approach: &str,
) -> Result<ExperimentArtifacts> {
    let artifacts = runtime
        .generator
        .generate_experiment(paper, approach)
        .await?;
    persist_artifacts(db, experiment_id, &artifacts, 1, None).await?;
    queries::update_experiment_status(db, experiment_id, status::TESTING, None, None, None, None)
        .await?;
    emit_status(handle, experiment_id, status::TESTING, None, None);
    Ok(artifacts)
}

pub async fn run_refine_path<R: Runtime>(
    db: &Db,
    runtime: &Arc<RuntimeSelector>,
    handle: &AppHandle<R>,
    experiment_id: &str,
    paper: PaperJson,
    prior: &PriorAttempt,
) -> Result<()> {
    queries::update_experiment_status(
        db,
        experiment_id,
        status::GENERATING,
        None,
        None,
        None,
        None,
    )
    .await?;
    emit_status(handle, experiment_id, status::GENERATING, None, None);

    let artifacts = runtime
        .generator
        .refine_experiment(paper.clone(), prior)
        .await?;
    persist_artifacts(db, experiment_id, &artifacts, 1, None).await?;
    queries::update_experiment_status(db, experiment_id, status::TESTING, None, None, None, None)
        .await?;
    emit_status(handle, experiment_id, status::TESTING, None, None);
    run_test_and_refine_loop(db, runtime, handle, experiment_id, paper, artifacts).await
}
