//! Row types mirrored from the SQLite schema.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PaperRow {
    pub id: String,
    pub title: String,
    pub authors: String, // JSON array
    pub file_path: String,
    pub original_filename: Option<String>,
    pub page_count: i64,
    pub thumbnail_path: Option<String>,
    pub processing_status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PageRow {
    pub id: String,
    pub paper_id: String,
    pub title: String,
    #[sqlx(rename = "type")]
    pub page_type: String,
    pub source_sections: String,
    pub source_pages: String,
    pub html_path: String,
    pub source_json: Option<String>,
    pub model_id: Option<String>,
    pub prompt_hash: Option<String>,
    pub validation_status: Option<String>,
    pub status: String,
    pub order_index: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// Currently displayed reading level — one of `eli5|default|expert`.
    /// Migration 005 adds this column with `DEFAULT 'default'` so existing
    /// rows are valid without a backfill.
    #[serde(default = "default_reading_level")]
    pub current_level: String,
    /// JSON object mapping a reading level to the absolute html path that
    /// was generated for it. Populated lazily as the user requests new
    /// levels; the "default" key is written on the very first generate
    /// pass so switching back is always instant.
    #[serde(default = "default_level_cache")]
    pub level_cache: String,
}

fn default_reading_level() -> String {
    "default".to_string()
}

fn default_level_cache() -> String {
    "{}".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct JobRow {
    pub id: String,
    pub paper_id: String,
    pub stage: String,
    pub status: String,
    pub progress: f64,
    pub current_model: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[allow(dead_code)] // Wired up once the OCR-block read commands land.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OcrBlockRow {
    pub id: String,
    pub paper_id: String,
    pub page_number: i64,
    pub block_type: String,
    pub text: Option<String>,
    pub bbox: Option<String>,
    pub confidence: Option<f64>,
    pub source_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExperimentRow {
    pub id: String,
    pub paper_id: String,
    pub parent_experiment_id: Option<String>,
    pub version: i64,
    pub title: String,
    pub status: String,
    pub plausibility_score: Option<i64>,
    pub plausibility_reason: Option<String>,
    pub approach_summary: Option<String>,
    pub inputs_schema: String,  // JSON array (InputSpec[])
    pub outputs_schema: String, // JSON array (OutputSpec[])
    pub python_code: Option<String>,
    pub python_tests: Option<String>,
    pub test_attempts: i64,
    pub test_last_output: Option<String>,
    pub refine_prompt: Option<String>,
    pub model_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One row of the `comparisons` table — a directional pair of papers
/// the user has asked to diff. Status FSM: `analyzing → ready | failed`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ComparisonRow {
    pub id: String,
    pub paper_a_id: String,
    pub paper_b_id: String,
    pub status: String,
    pub html_path: Option<String>,
    pub approach_summary: Option<String>,
    pub error_message: Option<String>,
    pub model_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Status labels for the `comparisons` row's FSM. Mirrors the values the
/// frontend expects in `ComparisonStatus` (see `src/lib/ipc/index.ts`).
pub mod comparison_status {
    pub const ANALYZING: &str = "analyzing";
    pub const READY: &str = "ready";
    pub const FAILED: &str = "failed";
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExperimentRunRow {
    pub id: i64,
    pub experiment_id: String,
    pub ts: i64,
    pub inputs_json: String,
    pub outputs_json: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
}
