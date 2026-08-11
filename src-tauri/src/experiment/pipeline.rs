//! Experiment pipeline orchestrator.
//!
//! Three sequential stages — `analyze`, `generate`, `test_and_refine` —
//! each lives in `stages/*` and writes its result back through
//! `db::queries::*`. Designed to be spawned via the JobQueue (or
//! `tokio::spawn` while the synthetic-paper-id FK deviation in
//! WORKAROUNDS.md is in place); the caller (Tauri command) returns
//! immediately and the UI listens on the `experiment` event channel.

use std::sync::Arc;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager, Runtime};

use super::stages::{run_analyze_stage, run_generate_stage, run_refine_path, status};
use super::test_loop::run_test_and_refine_loop;
use crate::db::{queries, Db};
use crate::events::{emit_experiment, ExperimentEvent};
use crate::ingest::structure::PaperJson;
use crate::runtime::{PriorAttempt, RuntimeSelector};

/// Choice of starting stage.
///
/// - `FreshAnalyze` — run analyze, then generate, then the test loop.
/// - `Refine` — fork forks skip `analyze` because they inherit
///   `approach_summary` from the parent and feed a `PriorAttempt`
///   directly into `refine_experiment`.
/// - `ForceGenerate` — user override on a borderline-implausible
///   experiment (`score ≥ 50` but the model said `plausible: false`).
///   Skip analyze, reuse the approach summary the analyze pass already
///   wrote, run generate + test loop. Lets the user say "I disagree
///   with the verdict, try it anyway".
pub enum ExperimentPipelineHandle {
    FreshAnalyze,
    Refine {
        approach_summary: String,
        prior: PriorAttempt,
    },
    ForceGenerate {
        approach_summary: String,
    },
}

/// Drive one experiment row from the entry stage all the way to a
/// terminal status. Errors are caught and persisted as `failed`; this
/// fn never returns `Err` to its spawner.
pub async fn run_experiment_pipeline<R: Runtime>(
    db: Db,
    runtime: Arc<RuntimeSelector>,
    handle: AppHandle<R>,
    experiment_id: String,
    paper_id: String,
    entry: ExperimentPipelineHandle,
) {
    if let Err(e) = try_run(&db, &runtime, &handle, &experiment_id, &paper_id, entry).await {
        let msg = format!("{e:#}");
        tracing::error!(experiment_id, %msg, "experiment pipeline failed");
        let _ = queries::update_experiment_status(
            &db,
            &experiment_id,
            status::FAILED,
            None,
            None,
            None,
            Some(&msg),
        )
        .await;
        emit_experiment(
            &handle,
            &ExperimentEvent::ExperimentStatus {
                experiment_id: experiment_id.clone(),
                status: status::FAILED.into(),
                score: None,
                error: Some(msg),
            },
        );
    }
}

async fn try_run<R: Runtime>(
    db: &Db,
    runtime: &Arc<RuntimeSelector>,
    handle: &AppHandle<R>,
    experiment_id: &str,
    paper_id: &str,
    entry: ExperimentPipelineHandle,
) -> Result<()> {
    let paper = load_paper_json(db, handle, paper_id).await?;

    let approach = match entry {
        ExperimentPipelineHandle::FreshAnalyze => {
            run_analyze_stage(db, runtime, handle, experiment_id, paper.clone()).await?
        }
        ExperimentPipelineHandle::Refine {
            approach_summary,
            prior,
        } => {
            // For a refine fork, we DO want analyze to look like it
            // already happened — copy the approach + emit `analyzing →
            // generating` so the UI's status chip lands on the same
            // path as a fresh experiment.
            queries::update_experiment_status(
                db,
                experiment_id,
                status::ANALYZING,
                None,
                None,
                Some(&approach_summary),
                None,
            )
            .await?;
            return run_refine_path(db, runtime, handle, experiment_id, paper, &prior).await;
        }
        ExperimentPipelineHandle::ForceGenerate { approach_summary } => {
            // User override on a borderline-implausible experiment.
            // Reuse the approach the analyze pass already produced —
            // it's still on the row — and emit a status flip so the
            // UI banner switches from "Implausible" to the in-flight
            // pipeline progression.
            queries::update_experiment_status(
                db,
                experiment_id,
                status::GENERATING,
                None,
                None,
                Some(&approach_summary),
                None,
            )
            .await?;
            emit_experiment(
                handle,
                &ExperimentEvent::ExperimentStatus {
                    experiment_id: experiment_id.to_string(),
                    status: status::GENERATING.into(),
                    score: None,
                    error: None,
                },
            );
            Some(approach_summary)
        }
    };

    let Some(approach) = approach else {
        // Marked implausible inside run_analyze_stage. Done.
        return Ok(());
    };

    let artifacts =
        run_generate_stage(db, runtime, handle, experiment_id, paper.clone(), &approach).await?;
    run_test_and_refine_loop(db, runtime, handle, experiment_id, paper, artifacts).await
}

async fn load_paper_json<R: Runtime>(
    db: &Db,
    handle: &AppHandle<R>,
    paper_id: &str,
) -> Result<PaperJson> {
    // `<app_data>/papers/<paper_id>.paper.json` is written by the
    // `structure_paper` stage (see pipeline::stages::structure). We
    // mirror the path resolution `Ctx::app_data` uses so dev builds can
    // redirect via `SMILIE_APP_DATA_OVERRIDE`.
    let app_data = resolve_app_data(handle)?;
    let path = app_data
        .join("papers")
        .join(format!("{paper_id}.paper.json"));
    if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading paper json at {}", path.display()))?;
        return Ok(serde_json::from_str(&raw)?);
    }
    // Fallback for "experiment requested before the import pipeline
    // finished structuring the paper" — synthesize a minimal PaperJson
    // from the DB row so the model at least sees the title.
    let row = queries::get_paper(db, paper_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("paper not found: {paper_id}"))?;
    Ok(PaperJson {
        paper_id: row.id,
        title: row.title,
        authors: serde_json::from_str(&row.authors).unwrap_or_default(),
        sections: vec![],
        figures: vec![],
        tables: vec![],
        equations: vec![],
        references: vec![],
        pages: vec![],
    })
}

fn resolve_app_data<R: Runtime>(handle: &AppHandle<R>) -> Result<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    {
        if let Ok(p) = std::env::var("SMILIE_APP_DATA_OVERRIDE") {
            if !p.is_empty() {
                return Ok(std::path::PathBuf::from(p));
            }
        }
    }
    Ok(handle.path().app_data_dir()?)
}
