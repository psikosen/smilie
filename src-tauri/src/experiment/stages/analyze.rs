//! Stage 1 — feasibility analysis.
//!
//! Runs the reasoning model's `analyze_experiment` RPC and applies the
//! plausibility threshold (design §2: score < 50 → implausible). On
//! success returns the approach summary the generate stage feeds the
//! generator model. On implausible the row is committed terminal and
//! we return `Ok(None)` so the orchestrator stops.

use std::sync::Arc;

use anyhow::Result;
use tauri::{AppHandle, Runtime};

use super::save::{emit_status, status};
use crate::db::{queries, Db};
use crate::ingest::structure::PaperJson;
use crate::runtime::RuntimeSelector;

/// Score below which we mark `implausible` and stop. Load-bearing
/// constant from the design doc — locked by a unit test.
pub const PLAUSIBILITY_THRESHOLD: u8 = 50;

pub async fn run_analyze_stage<R: Runtime>(
    db: &Db,
    runtime: &Arc<RuntimeSelector>,
    handle: &AppHandle<R>,
    experiment_id: &str,
    paper: PaperJson,
) -> Result<Option<String>> {
    emit_status(handle, experiment_id, status::ANALYZING, None, None);

    let analysis = runtime.reasoning.analyze_experiment(paper).await?;
    let plausible = analysis.plausible && analysis.score >= PLAUSIBILITY_THRESHOLD;

    let next_status = if plausible {
        status::GENERATING
    } else {
        status::IMPLAUSIBLE
    };
    queries::update_experiment_status(
        db,
        experiment_id,
        next_status,
        Some(analysis.score as i64),
        Some(&analysis.reasoning),
        Some(&analysis.approach_summary),
        None,
    )
    .await?;
    if !analysis.suggested_title.trim().is_empty() {
        update_title(db, experiment_id, &analysis.suggested_title).await?;
    }
    emit_status(
        handle,
        experiment_id,
        next_status,
        Some(analysis.score),
        None,
    );

    if plausible {
        Ok(Some(analysis.approach_summary))
    } else {
        Ok(None)
    }
}

async fn update_title(db: &Db, experiment_id: &str, suggested: &str) -> Result<()> {
    // The command pre-seeds a placeholder title; if the model offers
    // something better we adopt it. Called from analyze only — refines
    // inherit the parent title.
    let trimmed = suggested.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE experiments SET title = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(trimmed)
        .bind(chrono::Utc::now().timestamp())
        .bind(experiment_id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plausibility threshold is the load-bearing constant from the
    /// design doc §2 ("score < 50 → implausible"). Lock it down.
    #[test]
    fn plausibility_threshold_is_50() {
        assert_eq!(PLAUSIBILITY_THRESHOLD, 50);
    }
}
