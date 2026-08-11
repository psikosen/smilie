//! Two-paper comparison commands. Mirrors `src/lib/ipc/index.ts` —
//! `compareWithPaper`, `listComparisonsForPaper`, `getComparison`,
//! `deleteComparison`. Tauri arg names are camelCase so the JS-side
//! `invoke({ paperAId, paperBId })` calls land on the right params.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use super::{CmdError, CmdResult};
use crate::app_state::AppState;
use crate::comparison::run_comparison_pipeline;
use crate::db::{
    comparison_status as status,
    queries::{self, ComparisonWithTitlesRow},
    ComparisonRow,
};

#[derive(Debug, Serialize)]
pub struct ComparisonSummaryDto {
    pub id: String,
    pub paper_a_id: String,
    pub paper_b_id: String,
    pub paper_a_title: String,
    pub paper_b_title: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ComparisonDetailDto {
    #[serde(flatten)]
    pub summary: ComparisonSummaryDto,
    pub html_path: Option<String>,
    pub approach_summary: Option<String>,
    pub error_message: Option<String>,
}

fn list_row_to_summary(row: ComparisonWithTitlesRow) -> ComparisonSummaryDto {
    ComparisonSummaryDto {
        id: row.id,
        paper_a_id: row.paper_a_id,
        paper_b_id: row.paper_b_id,
        paper_a_title: row.paper_a_title,
        paper_b_title: row.paper_b_title,
        status: row.status,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Resolve the paper titles for a `ComparisonRow`. Used by the single-row
/// readers (`compare_with_paper` / `get_comparison`) where we don't have
/// the joined view from `list_comparisons_for_paper`.
async fn fetch_titles(
    state: &Arc<AppState>,
    paper_a_id: &str,
    paper_b_id: &str,
) -> CmdResult<(String, String)> {
    let a = queries::get_paper(&state.db, paper_a_id)
        .await?
        .ok_or_else(|| CmdError(format!("paper not found: {paper_a_id}")))?;
    let b = queries::get_paper(&state.db, paper_b_id)
        .await?
        .ok_or_else(|| CmdError(format!("paper not found: {paper_b_id}")))?;
    Ok((a.title, b.title))
}

fn row_to_summary_with_titles(
    row: ComparisonRow,
    paper_a_title: String,
    paper_b_title: String,
) -> ComparisonSummaryDto {
    ComparisonSummaryDto {
        id: row.id,
        paper_a_id: row.paper_a_id,
        paper_b_id: row.paper_b_id,
        paper_a_title,
        paper_b_title,
        status: row.status,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn row_to_detail(
    row: ComparisonRow,
    paper_a_title: String,
    paper_b_title: String,
) -> ComparisonDetailDto {
    let html_path = row.html_path.clone();
    let approach_summary = row.approach_summary.clone();
    let error_message = row.error_message.clone();
    let summary = row_to_summary_with_titles(row, paper_a_title, paper_b_title);
    ComparisonDetailDto {
        summary,
        html_path,
        approach_summary,
        error_message,
    }
}

/// Spawn the comparison pipeline. See WORKAROUNDS.md
/// "Experiment pipeline does not flow through `JobQueue`" — the same FK
/// constraint blocks routing comparisons through the JobQueue, so we
/// use a bare `tokio::spawn` here.
fn spawn_pipeline(
    state: &Arc<AppState>,
    comparison_id: String,
    paper_a_id: String,
    paper_b_id: String,
) {
    let db = state.db.clone();
    let runtime = Arc::clone(&state.runtime);
    let handle = state.handle.clone();
    tokio::spawn(async move {
        run_comparison_pipeline(db, runtime, handle, comparison_id, paper_a_id, paper_b_id).await;
    });
}

#[tauri::command]
pub async fn compare_with_paper(
    paper_a_id: String,
    paper_b_id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<ComparisonSummaryDto> {
    if paper_a_id == paper_b_id {
        return Err(CmdError(
            "compare_with_paper: paper A and paper B must be different".into(),
        ));
    }

    // Idempotency: if a comparison row already exists for this exact
    // direction, return it. The frontend treats `(a, b)` and `(b, a)` as
    // distinct (the diff text leads with paper A) so we DON'T normalize.
    if let Some(existing) =
        queries::find_comparison_by_pair(&state.db, &paper_a_id, &paper_b_id).await?
    {
        let (a_title, b_title) =
            fetch_titles(&state, &existing.paper_a_id, &existing.paper_b_id).await?;
        return Ok(row_to_summary_with_titles(existing, a_title, b_title));
    }

    // Quick existence check before we create a row pointing at a missing paper.
    let _ = queries::get_paper(&state.db, &paper_a_id)
        .await?
        .ok_or_else(|| CmdError(format!("paper not found: {paper_a_id}")))?;
    let _ = queries::get_paper(&state.db, &paper_b_id)
        .await?
        .ok_or_else(|| CmdError(format!("paper not found: {paper_b_id}")))?;

    let id = Uuid::new_v4().to_string();
    let ts = chrono::Utc::now().timestamp();
    queries::insert_comparison(
        &state.db,
        &id,
        &paper_a_id,
        &paper_b_id,
        status::ANALYZING,
        ts,
    )
    .await?;

    spawn_pipeline(&state, id.clone(), paper_a_id.clone(), paper_b_id.clone());

    let row = queries::get_comparison(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError("just-inserted comparison vanished".into()))?;
    let (a_title, b_title) = fetch_titles(&state, &row.paper_a_id, &row.paper_b_id).await?;
    Ok(row_to_summary_with_titles(row, a_title, b_title))
}

#[tauri::command]
pub async fn list_comparisons_for_paper(
    paper_id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<Vec<ComparisonSummaryDto>> {
    let rows = queries::list_comparisons_for_paper(&state.db, &paper_id).await?;
    Ok(rows.into_iter().map(list_row_to_summary).collect())
}

#[tauri::command]
pub async fn get_comparison(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<ComparisonDetailDto> {
    let row = queries::get_comparison(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError(format!("comparison not found: {id}")))?;
    let (a_title, b_title) = fetch_titles(&state, &row.paper_a_id, &row.paper_b_id).await?;
    Ok(row_to_detail(row, a_title, b_title))
}

#[tauri::command]
pub async fn delete_comparison(id: String, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    queries::delete_comparison(&state.db, &id).await?;
    // Best-effort cleanup of the rendered HTML on disk. We tolerate
    // failures because the row deletion is the load-bearing step — the
    // file might already be gone, or the override app_data dir might
    // have been wiped.
    let app_data = state.paths.app_data.clone();
    let _ = std::fs::remove_dir_all(app_data.join("comparisons").join(&id));
    Ok(())
}
