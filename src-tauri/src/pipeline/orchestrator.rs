//! Stage orchestrator — runs the 10-stage pipeline for a single paper.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;
use tauri::Runtime;

use super::stages;
use super::JobQueue;
use crate::db::queries;
use crate::events::{emit_paper_updated, emit_pipeline, PipelineEvent};

pub const STAGES: &[&str] = &[
    "import_pdf",
    "render_pages",
    "ocr_extract",
    "structure_paper",
    "build_learning_plan",
    "generate_pages",
    "validate_pages",
    "save_pages",
    "index_pages",
    "complete",
];

// Generic over `R` so the smoke test can drive the orchestrator with a
// `MockRuntime` AppHandle. Production callers pass `Arc<JobQueue<Wry>>`
// and the inferred `R = Wry` keeps existing call sites unchanged.
pub async fn run_pipeline<R: Runtime>(queue: &Arc<JobQueue<R>>, paper_id: &str) -> Result<()> {
    // Find or create a job. If a previous job failed, REUSE it and
    // resume from the stage it died on — rerunning earlier stages
    // (PDF render, OCR for ~10 pages, full reasoning pass) takes
    // minutes and the user expects "Resume" to actually skip them.
    let existing = queries::get_active_job(&queue.db, paper_id).await?;
    let (job_id, resume_from) = match existing {
        Some(job) => {
            let resume = if job.status == "failed" || job.status == "paused" {
                STAGES.iter().position(|s| *s == job.stage).unwrap_or(0)
            } else {
                0
            };
            (job.id, resume)
        }
        None => (
            queries::insert_job(&queue.db, paper_id, "import_pdf").await?,
            0,
        ),
    };

    // Register (or look up) the cancel flag for this job. cancel_job
    // sets it; we poll it between stages and at the end of stages so
    // a long-running RPC can be interrupted at the next checkpoint.
    let cancel = queue.cancel_flag(&job_id).await;
    cancel.store(false, Ordering::SeqCst);

    queries::update_paper_status(&queue.db, paper_id, "running").await?;
    queries::update_job(&queue.db, &job_id, None, Some("running"), None, None).await?;
    emit_paper_updated(&queue.handle, paper_id);

    let mut ctx = stages::Ctx {
        db: queue.db.clone(),
        runtime: Arc::clone(&queue.runtime),
        handle: queue.handle.clone(),
        paper_id: paper_id.to_string(),
        job_id: job_id.clone(),
    };

    if resume_from > 0 {
        tracing::info!(
            paper_id,
            stage = STAGES[resume_from],
            "resuming pipeline at last failed stage"
        );
    }

    for &stage in &STAGES[resume_from..] {
        queue.wait_if_paused(&job_id).await;

        if cancel.load(Ordering::SeqCst) {
            tracing::info!(paper_id, stage, "pipeline cancelled before stage");
            queue.clear_job_controls(&job_id).await;
            emit_paper_updated(&queue.handle, paper_id);
            return Ok(());
        }

        emit_pipeline(
            &queue.handle,
            &PipelineEvent::StageStarted {
                job_id: job_id.clone(),
                paper_id: paper_id.to_string(),
                stage: stage.to_string(),
            },
        );
        queries::update_job(
            &queue.db,
            &job_id,
            Some(stage),
            Some("running"),
            Some(0.0),
            None,
        )
        .await?;

        let outcome = stages::run_stage(stage, &mut ctx).await;

        if cancel.load(Ordering::SeqCst) {
            tracing::info!(paper_id, stage, "pipeline cancelled mid-stage");
            // cancel_job already wrote status=failed + error msg. Just
            // drop the flag, fire a paper update so the UI refreshes,
            // and exit cleanly.
            queue.clear_job_controls(&job_id).await;
            emit_paper_updated(&queue.handle, paper_id);
            return Ok(());
        }

        match outcome {
            Ok(()) => {
                emit_pipeline(
                    &queue.handle,
                    &PipelineEvent::StageCompleted {
                        job_id: job_id.clone(),
                        stage: stage.to_string(),
                    },
                );
                queries::update_job(
                    &queue.db,
                    &job_id,
                    Some(stage),
                    Some("running"),
                    Some(1.0),
                    None,
                )
                .await?;
            }
            Err(e) => {
                let msg = format!("{e:#}");
                emit_pipeline(
                    &queue.handle,
                    &PipelineEvent::StageFailed {
                        job_id: job_id.clone(),
                        stage: stage.to_string(),
                        error: msg.clone(),
                    },
                );
                queries::update_job(
                    &queue.db,
                    &job_id,
                    Some(stage),
                    Some("failed"),
                    None,
                    Some(&msg),
                )
                .await?;
                queries::update_paper_status(&queue.db, paper_id, "failed").await?;
                emit_paper_updated(&queue.handle, paper_id);
                return Err(e);
            }
        }
    }

    queries::update_job(
        &queue.db,
        &job_id,
        Some("complete"),
        Some("completed"),
        Some(1.0),
        None,
    )
    .await?;
    queries::update_paper_status(&queue.db, paper_id, "complete").await?;
    emit_paper_updated(&queue.handle, paper_id);
    queue.clear_job_controls(&job_id).await;
    Ok(())
}
