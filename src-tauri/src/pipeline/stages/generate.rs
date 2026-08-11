//! `generate_pages` stage — runs the generator runtime once per
//! placeholder row, renders the structured-block output to HTML, and
//! writes the artifacts under `app_data/pages/<page_id>/`.
//!
//! Idempotent on retry: each page's directory is cleared before its
//! files are written. `build_learning_plan` recreates page IDs on every
//! attempt, so any directory we encounter for a current page is from
//! this generation run — but a directory left behind from a prior plan
//! (different IDs) is harmless and will simply linger. We only own the
//! current set, and we wipe it before writing so a generator that
//! produced fewer files this attempt doesn't leak stale artifacts.
//!
//! Reading-level support: each row carries a `current_level` (one of
//! `eli5|default|expert`) which is threaded into the `PageSpec.level`
//! the generator sees. The output is written under a per-level
//! subdirectory so cached levels survive when a different one is
//! regenerated, and the resulting `index.html` path is stamped into
//! both `pages.html_path` and `pages.level_cache[level]` so a
//! subsequent switch to that level is instant.

use anyhow::{anyhow, Result};
use tauri::Runtime;

use super::Ctx;
use crate::db::{queries, PageRow};
use crate::events::{emit_pipeline, PipelineEvent};
use crate::pipeline::render::embed_page_sdk;
use crate::runtime::{GeneratedPage, LearningPlan};

/// Reading levels we cache on disk. Mirrors the `ReadingLevel` TS union
/// so the cache layout stays in lockstep with the IPC contract.
const KNOWN_LEVELS: &[&str] = &["eli5", "default", "expert"];

pub(super) async fn generate_pages<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    let raw = std::fs::read_to_string(ctx.plan_json_path()?)?;
    let plan: LearningPlan = serde_json::from_str(&raw)?;
    let rows = queries::list_pages_for_paper(&ctx.db, &ctx.paper_id).await?;
    let total = plan.pages.len() as f64;

    for (idx, spec) in plan.pages.iter().enumerate() {
        let row = rows
            .get(idx)
            .ok_or_else(|| anyhow!("plan/page row mismatch at {idx}"))?;

        // Skip pages already marked `ready`. This makes single-page
        // regeneration cheap: the user marks one row `queued`, we
        // re-run the pipeline, plan.rs short-circuits, and we only
        // re-render the row(s) the user actually asked for.
        if row.status == "ready" {
            emit_pipeline(
                &ctx.handle,
                &PipelineEvent::StageProgress {
                    job_id: ctx.job_id.clone(),
                    stage: "generate_pages".into(),
                    progress: (idx as f64 + 1.0) / total.max(1.0),
                },
            );
            continue;
        }

        let level = effective_level(row);
        let mut leveled = spec.clone();
        leveled.level = Some(level.clone());
        let out: GeneratedPage = ctx.runtime.generator.generate(leveled).await?;
        let html_path = write_page_files(ctx, &row.id, &spec.title, &level, &out)?;

        queries::set_page_status(&ctx.db, &row.id, "ready", None).await?;
        queries::set_page_html_path(&ctx.db, &row.id, &html_path).await?;
        // Cache this generated level so a future toggle back is
        // instant. We always write through `update_page_level_cache`
        // (rather than blindly overwriting) so the row's existing
        // entries for OTHER levels survive.
        queries::update_page_level_cache(&ctx.db, &row.id, &level, &html_path).await?;
        emit_pipeline(
            &ctx.handle,
            &PipelineEvent::PageReady {
                page_id: row.id.clone(),
                paper_id: ctx.paper_id.clone(),
            },
        );
        emit_pipeline(
            &ctx.handle,
            &PipelineEvent::StageProgress {
                job_id: ctx.job_id.clone(),
                stage: "generate_pages".into(),
                progress: (idx as f64 + 1.0) / total.max(1.0),
            },
        );
    }
    Ok(())
}

/// The level we should generate this row at — the row's `current_level`
/// when it's a known value, falling back to `default` when the column is
/// missing or junk (defensive: a user-edited DB shouldn't crash the
/// pipeline).
fn effective_level(row: &PageRow) -> String {
    let l = row.current_level.trim();
    if KNOWN_LEVELS.contains(&l) {
        l.to_string()
    } else {
        "default".to_string()
    }
}

/// Write the rendered HTML + source JSON for one page at one level.
///
/// Returns the absolute path to `index.html` so the caller can stamp it
/// into both `pages.html_path` (for the current view) and
/// `pages.level_cache[level]` (so the toggle is instant next time).
///
/// Layout: `<pages_dir>/<page_id>/<level>/index.html`. Writing under a
/// per-level subdirectory keeps each cached level independent — a
/// regenerate of `expert` does not clobber `default`.
fn write_page_files<R: Runtime>(
    ctx: &Ctx<R>,
    page_id: &str,
    fallback_title: &str,
    level: &str,
    page: &GeneratedPage,
) -> Result<String> {
    let level_dir = ctx.pages_dir()?.join(page_id).join(level);
    if level_dir.exists() {
        let _ = std::fs::remove_dir_all(&level_dir);
    }
    std::fs::create_dir_all(&level_dir)?;

    // Prefer the structured-block path: pure Rust rendering, no model
    // free-styling on HTML. Fall back to whatever HTML the model emitted
    // only when blocks are absent (legacy / rough fallback).
    let raw_html = match &page.blocks {
        Some(blocks) => crate::pipeline::render::render_page(page_id, fallback_title, blocks),
        None => page.html.clone(),
    };
    let embedded = embed_page_sdk(&raw_html);
    let html_path = level_dir.join("index.html");
    std::fs::write(&html_path, embedded)?;
    std::fs::write(level_dir.join("source.json"), &page.source_json)?;
    Ok(html_path.to_string_lossy().to_string())
}
