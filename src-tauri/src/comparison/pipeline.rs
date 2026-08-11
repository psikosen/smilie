//! Comparison pipeline orchestrator.
//!
//! Reads both papers' `paper.json` from disk (falling back to a stub
//! built from the DB row when structure has not yet run), drives the
//! reasoning runtime's `compare_papers` to produce `PageBlocks`, and
//! renders + persists a single HTML page under
//! `app_data/comparisons/<id>/index.html`. On any error the comparison
//! row is committed terminal as `failed` and the error is mirrored on
//! the `comparison` event channel.
//!
//! Spawned via bare `tokio::spawn` (same JobQueue deviation as the
//! experiment pipeline — see WORKAROUNDS.md).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager, Runtime};

use crate::db::{comparison_status as status, queries, Db};
use crate::events::{emit_comparison, ComparisonEvent};
use crate::ingest::structure::PaperJson;
use crate::pipeline::render::{embed_page_sdk, render_page};
use crate::runtime::{Block, PageBlocks, RuntimeSelector};

/// Drive a single comparison row from `analyzing → ready | failed`.
/// Errors are caught and persisted; this fn never returns `Err` to the
/// `tokio::spawn` caller.
pub async fn run_comparison_pipeline<R: Runtime>(
    db: Db,
    runtime: Arc<RuntimeSelector>,
    handle: AppHandle<R>,
    comparison_id: String,
    paper_a_id: String,
    paper_b_id: String,
) {
    if let Err(e) = try_run(
        &db,
        &runtime,
        &handle,
        &comparison_id,
        &paper_a_id,
        &paper_b_id,
    )
    .await
    {
        let msg = format!("{e:#}");
        tracing::error!(comparison_id, %msg, "comparison pipeline failed");
        let _ = queries::update_comparison_status(
            &db,
            &comparison_id,
            status::FAILED,
            None,
            None,
            Some(&msg),
            None,
        )
        .await;
        emit_comparison(
            &handle,
            &ComparisonEvent::ComparisonStatus {
                comparison_id: comparison_id.clone(),
                status: status::FAILED.into(),
                error: Some(msg),
            },
        );
    }
}

async fn try_run<R: Runtime>(
    db: &Db,
    runtime: &Arc<RuntimeSelector>,
    handle: &AppHandle<R>,
    comparison_id: &str,
    paper_a_id: &str,
    paper_b_id: &str,
) -> Result<()> {
    emit_comparison(
        handle,
        &ComparisonEvent::ComparisonStatus {
            comparison_id: comparison_id.to_string(),
            status: status::ANALYZING.into(),
            error: None,
        },
    );

    let paper_a = load_paper_json(db, handle, paper_a_id).await?;
    let paper_b = load_paper_json(db, handle, paper_b_id).await?;

    // Reasoning runtime tends to be the larger/more capable model and
    // the diff is more conceptual than rendering-heavy — match the
    // experiment pipeline's choice for the same reason.
    let blocks = runtime
        .reasoning
        .compare_papers(paper_a.clone(), paper_b.clone())
        .await?;
    let html_path = write_comparison_html(handle, comparison_id, &paper_a, &paper_b, &blocks)?;

    let approach = build_approach_summary(&paper_a, &paper_b, &blocks);
    queries::update_comparison_status(
        db,
        comparison_id,
        status::READY,
        Some(&html_path),
        Some(&approach),
        None,
        None,
    )
    .await?;
    emit_comparison(
        handle,
        &ComparisonEvent::ComparisonStatus {
            comparison_id: comparison_id.to_string(),
            status: status::READY.into(),
            error: None,
        },
    );
    Ok(())
}

/// Render `blocks` to HTML, embed the page SDK, write to
/// `<app_data>/comparisons/<id>/index.html`, and return the absolute
/// path. Any prior directory contents are wiped first so a re-run does
/// not leak stale files.
fn write_comparison_html<R: Runtime>(
    handle: &AppHandle<R>,
    comparison_id: &str,
    paper_a: &PaperJson,
    paper_b: &PaperJson,
    blocks: &PageBlocks,
) -> Result<String> {
    let app_data = resolve_app_data(handle)?;
    let dir = app_data.join("comparisons").join(comparison_id);
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating comparison dir {}", dir.display()))?;

    let fallback_title = format!("Comparison: {} vs {}", paper_a.title, paper_b.title);
    let raw_html = render_page(comparison_id, &fallback_title, blocks);
    let embedded = embed_page_sdk(&raw_html);
    let out_path = dir.join("index.html");
    std::fs::write(&out_path, embedded)
        .with_context(|| format!("writing comparison html {}", out_path.display()))?;
    Ok(out_path.to_string_lossy().to_string())
}

/// Build the short "approach" summary used by the comparison list UI.
/// Composed from the diff title + the first heading text in the blocks
/// (when present) so the user can scan the summary in the picker without
/// opening the iframe.
fn build_approach_summary(paper_a: &PaperJson, paper_b: &PaperJson, blocks: &PageBlocks) -> String {
    let title = if blocks.title.trim().is_empty() {
        format!("Comparison: {} vs {}", paper_a.title, paper_b.title)
    } else {
        blocks.title.clone()
    };
    let first_heading = blocks.blocks.iter().find_map(|b| match b {
        Block::Heading { text, .. } => Some(text.clone()),
        _ => None,
    });
    match first_heading {
        Some(h) if !h.trim().is_empty() => format!("{title} — {h}"),
        _ => title,
    }
}

async fn load_paper_json<R: Runtime>(
    db: &Db,
    handle: &AppHandle<R>,
    paper_id: &str,
) -> Result<PaperJson> {
    let app_data = resolve_app_data(handle)?;
    let path = app_data
        .join("papers")
        .join(format!("{paper_id}.paper.json"));
    if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading paper json at {}", path.display()))?;
        return Ok(serde_json::from_str(&raw)?);
    }
    // Fallback for "comparison requested before structure_paper finished
    // for this paper" — synthesize a minimal PaperJson so the model at
    // least sees the title.
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

fn resolve_app_data<R: Runtime>(handle: &AppHandle<R>) -> Result<PathBuf> {
    #[cfg(debug_assertions)]
    {
        if let Ok(p) = std::env::var("SMILIE_APP_DATA_OVERRIDE") {
            if !p.is_empty() {
                return Ok(PathBuf::from(p));
            }
        }
    }
    Ok(handle.path().app_data_dir()?)
}
