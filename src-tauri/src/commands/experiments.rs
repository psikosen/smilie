//! Experiments IPC surface — see `src/lib/ipc/index.ts` "Experiments"
//! block for the matching TS contract. All Tauri arg names are camelCase
//! to match the JS-side `invoke` payloads exactly.

use std::sync::Arc;
use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use super::{CmdError, CmdResult};
use crate::app_state::AppState;
use crate::db::{queries, ExperimentRow, ExperimentRunRow};
use crate::experiment::{run_experiment_pipeline, ExperimentPipelineHandle};
use crate::runtime::PriorAttempt;

// ── DTOs ────────────────────────────────────────────────────────
//
// Mirrors of `ExperimentSummary` / `ExperimentDetail` / `ExperimentRun`
// in `src/lib/ipc/index.ts`. Kept narrowly here so the DB row schema can
// drift independently from the wire shape.

#[derive(Debug, Serialize)]
pub struct ExperimentSummaryDto {
    pub id: String,
    pub paper_id: String,
    pub parent_experiment_id: Option<String>,
    pub version: i64,
    pub title: String,
    pub status: String,
    pub plausibility_score: Option<i64>,
    pub refine_prompt: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ExperimentDetailDto {
    #[serde(flatten)]
    pub summary: ExperimentSummaryDto,
    pub plausibility_reason: Option<String>,
    pub approach_summary: Option<String>,
    pub inputs_schema: serde_json::Value,
    pub outputs_schema: serde_json::Value,
    pub python_code: Option<String>,
    pub python_tests: Option<String>,
    pub test_attempts: i64,
    pub test_last_output: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExperimentRunDto {
    pub id: i64,
    pub experiment_id: String,
    pub ts: i64,
    pub inputs: serde_json::Value,
    pub outputs: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
}

fn row_to_summary(row: ExperimentRow) -> ExperimentSummaryDto {
    ExperimentSummaryDto {
        id: row.id,
        paper_id: row.paper_id,
        parent_experiment_id: row.parent_experiment_id,
        version: row.version,
        title: row.title,
        status: row.status,
        plausibility_score: row.plausibility_score,
        refine_prompt: row.refine_prompt,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn row_to_detail(row: ExperimentRow) -> ExperimentDetailDto {
    let inputs_schema: serde_json::Value =
        serde_json::from_str(&row.inputs_schema).unwrap_or_else(|_| serde_json::json!([]));
    let outputs_schema: serde_json::Value =
        serde_json::from_str(&row.outputs_schema).unwrap_or_else(|_| serde_json::json!([]));
    let summary = ExperimentSummaryDto {
        id: row.id,
        paper_id: row.paper_id,
        parent_experiment_id: row.parent_experiment_id,
        version: row.version,
        title: row.title,
        status: row.status,
        plausibility_score: row.plausibility_score,
        refine_prompt: row.refine_prompt,
        created_at: row.created_at,
        updated_at: row.updated_at,
    };
    ExperimentDetailDto {
        summary,
        plausibility_reason: row.plausibility_reason,
        approach_summary: row.approach_summary,
        inputs_schema,
        outputs_schema,
        python_code: row.python_code,
        python_tests: row.python_tests,
        test_attempts: row.test_attempts,
        test_last_output: row.test_last_output,
        model_id: row.model_id,
    }
}

fn run_row_to_dto(row: ExperimentRunRow) -> ExperimentRunDto {
    let inputs: serde_json::Value =
        serde_json::from_str(&row.inputs_json).unwrap_or_else(|_| serde_json::json!({}));
    let outputs: Option<serde_json::Value> = row
        .outputs_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    ExperimentRunDto {
        id: row.id,
        experiment_id: row.experiment_id,
        ts: row.ts,
        inputs,
        outputs,
        error_message: row.error_message,
        duration_ms: row.duration_ms,
    }
}

// ── Commands ────────────────────────────────────────────────────

#[tauri::command]
pub async fn analyze_experiment(
    paper_id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<ExperimentSummaryDto> {
    // Quick existence check so the user gets a clean error before we
    // create a phantom experiment row pointing at a missing paper.
    let paper = queries::get_paper(&state.db, &paper_id)
        .await?
        .ok_or_else(|| CmdError(format!("paper not found: {paper_id}")))?;

    let id = Uuid::new_v4().to_string();
    let title = format!("Experiment from “{}”", paper.title);
    queries::insert_experiment(
        &state.db,
        &id,
        &paper_id,
        None,
        1,
        &title,
        "analyzing",
        None,
    )
    .await?;

    spawn_pipeline(
        &state,
        id.clone(),
        paper_id.clone(),
        ExperimentPipelineHandle::FreshAnalyze,
    );

    let row = queries::get_experiment(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError("just-inserted experiment vanished".into()))?;
    Ok(row_to_summary(row))
}

#[tauri::command]
pub async fn list_experiments_for_paper(
    paper_id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<Vec<ExperimentSummaryDto>> {
    let rows = queries::list_experiments_for_paper(&state.db, &paper_id).await?;
    Ok(rows.into_iter().map(row_to_summary).collect())
}

#[tauri::command]
pub async fn get_experiment(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<ExperimentDetailDto> {
    let row = queries::get_experiment(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError(format!("experiment not found: {id}")))?;
    Ok(row_to_detail(row))
}

#[tauri::command]
pub async fn refine_experiment(
    parent_id: String,
    prompt: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<ExperimentSummaryDto> {
    let parent = queries::get_experiment(&state.db, &parent_id)
        .await?
        .ok_or_else(|| CmdError(format!("parent experiment not found: {parent_id}")))?;

    let id = Uuid::new_v4().to_string();
    // Refines bump the version chain; the first refine of v1 → v2.
    let version = parent.version + 1;
    let title = format!("{} (v{})", strip_version_suffix(&parent.title), version);
    queries::insert_experiment(
        &state.db,
        &id,
        &parent.paper_id,
        Some(&parent_id),
        version,
        &title,
        "analyzing",
        Some(&prompt),
    )
    .await?;

    let approach = parent.approach_summary.clone().unwrap_or_else(|| {
        "(no parent approach summary; treat the prompt as the directive)".into()
    });
    let prior = PriorAttempt {
        code: parent.python_code.clone().unwrap_or_default(),
        tests: parent.python_tests.clone().unwrap_or_default(),
        error_output: "(user refine)".into(),
        user_refine_prompt: Some(prompt),
    };
    spawn_pipeline(
        &state,
        id.clone(),
        parent.paper_id.clone(),
        ExperimentPipelineHandle::Refine {
            approach_summary: approach,
            prior,
        },
    );

    let row = queries::get_experiment(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError("just-inserted refine vanished".into()))?;
    Ok(row_to_summary(row))
}

#[tauri::command]
pub async fn delete_experiment(id: String, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    queries::delete_experiment(&state.db, &id).await?;
    Ok(())
}

/// User override on a borderline-implausible experiment. Allowed only
/// when the model returned `score >= 50` but flagged `plausible: false`
/// — the user is saying "I disagree with the verdict, run it anyway".
/// Reuses the analyze pass's `approach_summary` (already on the row)
/// and skips straight into the generate stage.
#[tauri::command]
pub async fn force_generate_experiment(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<ExperimentSummaryDto> {
    let row = queries::get_experiment(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError(format!("experiment not found: {id}")))?;

    if row.status != "implausible" {
        return Err(CmdError(format!(
            "force_generate only allowed on implausible experiments, got status={}",
            row.status
        )));
    }
    let score = row.plausibility_score.unwrap_or(0);
    if score < 50 {
        return Err(CmdError(format!(
            "force_generate refused: score {score} is below the 50 floor"
        )));
    }
    let Some(approach) = row.approach_summary.clone() else {
        return Err(CmdError(
            "force_generate refused: no approach_summary on the row \
             (analyze stage didn't write one — refine instead)"
                .into(),
        ));
    };

    let paper_id = row.paper_id.clone();
    spawn_pipeline(
        &state,
        id.clone(),
        paper_id,
        ExperimentPipelineHandle::ForceGenerate {
            approach_summary: approach,
        },
    );

    let row = queries::get_experiment(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError("just-updated experiment vanished".into()))?;
    Ok(row_to_summary(row))
}

#[tauri::command]
pub async fn record_experiment_run(
    experiment_id: String,
    inputs: serde_json::Value,
    outputs: Option<serde_json::Value>,
    error_message: Option<String>,
    duration_ms: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<()> {
    let inputs_json = serde_json::to_string(&inputs)?;
    let outputs_json = match &outputs {
        Some(v) => Some(serde_json::to_string(v)?),
        None => None,
    };
    queries::insert_experiment_run(
        &state.db,
        &experiment_id,
        chrono::Utc::now().timestamp(),
        &inputs_json,
        outputs_json.as_deref(),
        error_message.as_deref(),
        duration_ms,
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn list_experiment_runs(
    experiment_id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<Vec<ExperimentRunDto>> {
    let rows = queries::list_experiment_runs(&state.db, &experiment_id, 50).await?;
    Ok(rows.into_iter().map(run_row_to_dto).collect())
}

fn spawn_pipeline(
    state: &Arc<AppState>,
    experiment_id: String,
    paper_id: String,
    entry: ExperimentPipelineHandle,
) {
    // NOTE: design §2 wants this routed through the existing JobQueue
    // with a synthetic `paper_id = format!("experiment:{experiment_id}")`,
    // but `jobs.paper_id REFERENCES papers(id)` (migrations/001_init.sql)
    // rejects that key today. We use a bare `tokio::spawn` while the
    // FK deviation logged in WORKAROUNDS.md is in place; the JobQueue
    // resume-on-startup path will pick "experiment:*" rows once the FK
    // is lifted in a future migration.
    let db = state.db.clone();
    let runtime = Arc::clone(&state.runtime);
    let handle = state.handle.clone();
    tokio::spawn(async move {
        run_experiment_pipeline(db, runtime, handle, experiment_id, paper_id, entry).await;
    });
}

/// Strips a trailing ` (v<digits>)` from a title so refines don't
/// accumulate `Title (v2) (v3) (v4)`. Targets ONLY the literal version
/// suffix — preserves any other trailing parens or trailing letters
/// (the prior char-class trim ate "TVs" → "TV", parens-with-content,
/// etc.).
fn strip_version_suffix(s: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\s*\(v\d+\)\s*$").expect("compile"));
    re.replace(s.trim_end(), "").trim_end().to_string()
}

/// Returns the absolute filesystem path of the bundled
/// `experiment-runner.html`. The frontend wraps this with
/// `convertFileSrc(path, "asset")` to produce the iframe URL — the
/// returned path lives under the Tauri resource dir, so
/// `tauri.conf.json`'s `assetProtocol.scope` MUST include `$RESOURCE/**`
/// for the asset protocol to serve it.
///
/// Implemented as a command (rather than a hardcoded JS string) so the
/// path stays correct across dev (`target/debug/.../resources`) and
/// installed (`Smilie.app/Contents/Resources`) layouts.
#[tauri::command]
pub async fn experiment_runner_url(handle: AppHandle) -> CmdResult<String> {
    let resource_dir = handle
        .path()
        .resource_dir()
        .map_err(|e| CmdError(format!("resolving resource_dir: {e}")))?;
    let path = resource_dir.join("assets").join("experiment-runner.html");
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::strip_version_suffix;

    #[test]
    fn strips_simple_version_suffix() {
        assert_eq!(strip_version_suffix("Foo (v2)"), "Foo");
        assert_eq!(strip_version_suffix("Foo (v12)"), "Foo");
    }

    #[test]
    fn idempotent_on_clean_titles() {
        // Real titles with parens / trailing letters survive.
        assert_eq!(
            strip_version_suffix("Pluto's TV (study)"),
            "Pluto's TV (study)"
        );
        assert_eq!(strip_version_suffix("TVs"), "TVs");
        assert_eq!(
            strip_version_suffix("Replicating Figure 4"),
            "Replicating Figure 4"
        );
    }

    #[test]
    fn handles_only_one_layer() {
        // After one strip, repeated refines build the chain via the
        // `(v<n>)` formatter — we only ever see at most one suffix.
        assert_eq!(strip_version_suffix("Foo (v2) (v3)"), "Foo (v2)");
    }

    #[test]
    fn trims_trailing_whitespace() {
        assert_eq!(strip_version_suffix("Foo   (v3)   "), "Foo");
    }
}
