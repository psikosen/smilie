//! `build_learning_plan` stage — runs reasoning over the structured
//! paper and inserts placeholder page rows for the UI to render.
//!
//! Idempotent on retry: clears `pages` for the paper before re-inserting
//! so a re-run after a partial failure doesn't accumulate orphan rows
//! with stale order indexes. The plan.json file is overwritten in
//! place, which is naturally idempotent.
//!
//! Single-page regeneration short-circuit: when the user clicks
//! "Regenerate" on an existing page we mark that one row `queued` and
//! re-run the pipeline. We must NOT re-derive the plan or wipe sibling
//! pages in that path. If `plan.json` already exists AND we already have
//! page rows in the DB, the plan is reused as-is and the only work we
//! defer to is `generate_pages`, which regenerates pages whose status is
//! not `ready`.

use anyhow::Result;
use tauri::Runtime;

use super::Ctx;
use crate::db::queries;
use crate::ingest::structure::PaperJson;
use crate::runtime::LearningPlan;

pub(super) async fn build_learning_plan<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    // Short-circuit: if a plan + page rows already exist, the user is
    // hitting this stage from a regenerate-single-page flow. Re-running
    // the model would replace the plan AND `clear_pages_for_paper` would
    // nuke ready siblings. Keep what we have and let `generate_pages`
    // rebuild only the queued page.
    let plan_path = ctx.plan_json_path()?;
    if plan_path.exists() {
        let existing_pages = queries::list_pages_for_paper(&ctx.db, &ctx.paper_id).await?;
        if !existing_pages.is_empty() {
            tracing::info!(
                paper_id = %ctx.paper_id,
                "plan.json + page rows exist; skipping reasoning re-run"
            );
            return Ok(());
        }
    }

    let raw = std::fs::read_to_string(ctx.paper_json_path()?)?;
    let paper: PaperJson = serde_json::from_str(&raw)?;
    let plan: LearningPlan = ctx.runtime.reasoning.reason(paper).await?;

    std::fs::write(&plan_path, serde_json::to_string_pretty(&plan)?)?;

    // Drop any placeholder rows from a prior attempt before re-inserting.
    queries::clear_pages_for_paper(&ctx.db, &ctx.paper_id).await?;

    // Insert placeholder page rows so sidebar can show them before generation.
    for (idx, spec) in plan.pages.iter().enumerate() {
        let sections_json = serde_json::to_string(&spec.source_sections)?;
        let pages_json = serde_json::to_string(&spec.source_pages)?;
        queries::insert_page(
            &ctx.db,
            &ctx.paper_id,
            &spec.title,
            &spec.page_type,
            &sections_json,
            &pages_json,
            idx as i64,
        )
        .await?;
    }
    Ok(())
}
