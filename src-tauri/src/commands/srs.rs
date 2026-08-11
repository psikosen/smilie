//! Spaced-repetition commands.
//!
//! Two endpoints:
//!   * `record_quiz_attempt(page_id, correct, total)` — appends an attempt
//!     row and advances the page's SRS state.
//!   * `list_due_pages()` — returns pages whose `next_due_at <= now`.

use std::sync::Arc;

use chrono::Utc;
use serde::Serialize;
use tauri::State;

use super::CmdResult;
use crate::app_state::AppState;
use crate::db::queries;
use crate::srs::{next_state, AttemptOutcome};

#[derive(Debug, Serialize)]
pub struct DuePageDto {
    pub page_id: String,
    pub paper_id: String,
    pub title: String,
    pub due_at: i64,
}

#[tauri::command]
pub async fn record_quiz_attempt(
    page_id: String,
    correct: i64,
    total: i64,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<()> {
    let now_ts = Utc::now().timestamp();

    // 1) Persist the attempt.
    queries::record_quiz_attempt(&state.db, &page_id, now_ts, correct, total).await?;

    // 2) Advance SRS state.
    let prior = queries::get_srs_state(&state.db, &page_id).await?;
    let prior_tuple = prior.map(|(ease, interval, _due, reps)| (ease, interval, reps));
    let update = next_state(prior_tuple, now_ts, AttemptOutcome { correct, total });
    queries::upsert_srs_state(
        &state.db,
        &page_id,
        update.ease,
        update.interval_days,
        update.next_due_at_secs,
        update.reps,
    )
    .await?;

    Ok(())
}

#[tauri::command]
pub async fn list_due_pages(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<DuePageDto>> {
    let now_ts = Utc::now().timestamp();
    let rows = queries::list_due_for_review(&state.db, now_ts).await?;
    Ok(rows
        .into_iter()
        .map(|r| DuePageDto {
            page_id: r.page_id,
            paper_id: r.paper_id,
            title: r.title,
            due_at: r.next_due_at,
        })
        .collect())
}
