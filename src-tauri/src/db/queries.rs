//! Focused query helpers. Keep each fn small; prefer composition in
//! pipeline/ingest over giant query monoliths here.

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::{ComparisonRow, Db, ExperimentRow, ExperimentRunRow, JobRow, PageRow, PaperRow};

fn now() -> i64 {
    Utc::now().timestamp()
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

// ── Papers ──────────────────────────────────────────────────

pub async fn insert_paper(
    db: &Db,
    title: &str,
    authors_json: &str,
    file_path: &str,
    original_filename: Option<&str>,
) -> Result<String> {
    let id = new_id();
    let ts = now();
    sqlx::query(
        "INSERT INTO papers (id, title, authors, file_path, original_filename,
                             page_count, processing_status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 'queued', ?6, ?6)",
    )
    .bind(&id)
    .bind(title)
    .bind(authors_json)
    .bind(file_path)
    .bind(original_filename)
    .bind(ts)
    .execute(&db.pool)
    .await?;
    Ok(id)
}

pub async fn list_papers(db: &Db) -> Result<Vec<PaperRow>> {
    let rows: Vec<PaperRow> = sqlx::query_as("SELECT * FROM papers ORDER BY created_at DESC")
        .fetch_all(&db.pool)
        .await?;
    Ok(rows)
}

pub async fn get_paper(db: &Db, id: &str) -> Result<Option<PaperRow>> {
    let row: Option<PaperRow> = sqlx::query_as("SELECT * FROM papers WHERE id = ?1")
        .bind(id)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row)
}

pub async fn update_paper_status(db: &Db, id: &str, status: &str) -> Result<()> {
    sqlx::query("UPDATE papers SET processing_status = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(status)
        .bind(now())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn update_paper_page_count(db: &Db, id: &str, count: i64) -> Result<()> {
    sqlx::query("UPDATE papers SET page_count = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(count)
        .bind(now())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn update_paper_thumbnail(db: &Db, id: &str, path: &str) -> Result<()> {
    sqlx::query("UPDATE papers SET thumbnail_path = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(path)
        .bind(now())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn update_paper_title(db: &Db, id: &str, title: &str) -> Result<()> {
    sqlx::query("UPDATE papers SET title = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(title)
        .bind(now())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn delete_paper(db: &Db, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM papers WHERE id = ?1")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

// ── Pages ───────────────────────────────────────────────────

pub async fn insert_page(
    db: &Db,
    paper_id: &str,
    title: &str,
    page_type: &str,
    source_sections_json: &str,
    source_pages_json: &str,
    order_index: i64,
) -> Result<String> {
    let id = new_id();
    let ts = now();
    sqlx::query(
        "INSERT INTO pages (id, paper_id, title, type, source_sections,
                            source_pages, order_index, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
    )
    .bind(&id)
    .bind(paper_id)
    .bind(title)
    .bind(page_type)
    .bind(source_sections_json)
    .bind(source_pages_json)
    .bind(order_index)
    .bind(ts)
    .execute(&db.pool)
    .await?;
    Ok(id)
}

pub async fn set_page_html_path(db: &Db, id: &str, path: &str) -> Result<()> {
    sqlx::query("UPDATE pages SET html_path = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(path)
        .bind(now())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn set_page_status(
    db: &Db,
    id: &str,
    status: &str,
    validation: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE pages SET status = ?1, validation_status = COALESCE(?2, validation_status),
         updated_at = ?3 WHERE id = ?4",
    )
    .bind(status)
    .bind(validation)
    .bind(now())
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn list_pages_for_paper(db: &Db, paper_id: &str) -> Result<Vec<PageRow>> {
    let rows: Vec<PageRow> =
        sqlx::query_as("SELECT * FROM pages WHERE paper_id = ?1 ORDER BY order_index ASC")
            .bind(paper_id)
            .fetch_all(&db.pool)
            .await?;
    Ok(rows)
}

pub async fn get_page(db: &Db, id: &str) -> Result<Option<PageRow>> {
    let row: Option<PageRow> = sqlx::query_as("SELECT * FROM pages WHERE id = ?1")
        .bind(id)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row)
}

/// Idempotent retry helper. The OCR stage owns every row in `ocr_blocks`
/// for a given paper — clearing them up front means a re-run after a
/// partial failure starts from an empty slate instead of accumulating
/// duplicate blocks the reasoning model would later see twice.
pub async fn clear_ocr_blocks_for_paper(db: &Db, paper_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM ocr_blocks WHERE paper_id = ?1")
        .bind(paper_id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Idempotent retry helper. `build_learning_plan` re-derives the page
/// roster from scratch, so any rows from a prior attempt would conflict
/// with the new placeholders. Drop them before re-inserting.
///
/// CONTRACT: only safe to call from a fresh-pipeline run that is about
/// to re-derive the entire page roster. Calling this from `regenerate_page`
/// (or any single-page retry path) would nuke ready pages the user can
/// see in the sidebar. The plan stage short-circuits when `plan.json`
/// already exists to avoid that — keep this fn only ever invoked from
/// the "no plan yet" branch.
pub async fn clear_pages_for_paper(db: &Db, paper_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM pages WHERE paper_id = ?1")
        .bind(paper_id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

// ── Jobs ────────────────────────────────────────────────────

pub async fn insert_job(db: &Db, paper_id: &str, stage: &str) -> Result<String> {
    let id = new_id();
    let ts = now();
    sqlx::query(
        "INSERT INTO jobs (id, paper_id, stage, status, progress,
                           retry_count, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'pending', 0, 0, ?4, ?4)",
    )
    .bind(&id)
    .bind(paper_id)
    .bind(stage)
    .bind(ts)
    .execute(&db.pool)
    .await?;
    Ok(id)
}

pub async fn update_job(
    db: &Db,
    id: &str,
    stage: Option<&str>,
    status: Option<&str>,
    progress: Option<f64>,
    error: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE jobs SET
            stage = COALESCE(?1, stage),
            status = COALESCE(?2, status),
            progress = COALESCE(?3, progress),
            error_message = ?4,
            updated_at = ?5
         WHERE id = ?6",
    )
    .bind(stage)
    .bind(status)
    .bind(progress)
    .bind(error)
    .bind(now())
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn get_job(db: &Db, job_id: &str) -> Result<Option<JobRow>> {
    let row: Option<JobRow> = sqlx::query_as("SELECT * FROM jobs WHERE id = ?1")
        .bind(job_id)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row)
}

/// `failed` is intentionally included alongside the live statuses: the UI
/// reuses this for both the progress display AND the "missing weights /
/// retry" banner, so a paper whose last job failed still needs to surface.
/// The orchestrator's resume logic also relies on finding a prior failed
/// row so it can pick up from the stage that died.
pub async fn get_active_job(db: &Db, paper_id: &str) -> Result<Option<JobRow>> {
    let row: Option<JobRow> = sqlx::query_as(
        "SELECT * FROM jobs WHERE paper_id = ?1
          AND status IN ('pending', 'running', 'paused', 'failed')
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(paper_id)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row)
}

pub async fn list_resumable_jobs(db: &Db) -> Result<Vec<JobRow>> {
    let rows: Vec<JobRow> = sqlx::query_as(
        "SELECT * FROM jobs WHERE status IN ('pending', 'running', 'paused')
         ORDER BY created_at ASC",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

// ── Settings ────────────────────────────────────────────────

pub async fn get_setting(db: &Db, key: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row.map(|r| r.0))
}

pub async fn set_setting(db: &Db, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(&db.pool)
    .await?;
    Ok(())
}

// ── FTS ────────────────────────────────────────────────────

pub async fn index_page_fts(
    db: &Db,
    page_id: &str,
    paper_id: &str,
    title: &str,
    body: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM pages_fts WHERE page_id = ?1")
        .bind(page_id)
        .execute(&db.pool)
        .await?;
    sqlx::query("INSERT INTO pages_fts (page_id, paper_id, title, body) VALUES (?1, ?2, ?3, ?4)")
        .bind(page_id)
        .bind(paper_id)
        .bind(title)
        .bind(body)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn search_pages_fts(db: &Db, query: &str) -> Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT page_id FROM pages_fts WHERE pages_fts MATCH ?1 LIMIT 100")
            .bind(query)
            .fetch_all(&db.pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

// ── SRS / quiz attempts ─────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DueRow {
    pub page_id: String,
    pub paper_id: String,
    pub title: String,
    pub next_due_at: i64,
}

pub async fn record_quiz_attempt(
    db: &Db,
    page_id: &str,
    ts: i64,
    correct: i64,
    total: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO quiz_attempts (page_id, ts, correct_count, total_count)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(page_id)
    .bind(ts)
    .bind(correct)
    .bind(total)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn upsert_srs_state(
    db: &Db,
    page_id: &str,
    ease: f64,
    interval_days: f64,
    next_due_at: i64,
    reps: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO srs_state (page_id, ease_factor, interval_days, next_due_at, reps, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(page_id) DO UPDATE SET
            ease_factor   = excluded.ease_factor,
            interval_days = excluded.interval_days,
            next_due_at   = excluded.next_due_at,
            reps          = excluded.reps,
            updated_at    = excluded.updated_at",
    )
    .bind(page_id)
    .bind(ease)
    .bind(interval_days)
    .bind(next_due_at)
    .bind(reps)
    .bind(now())
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Returns `(ease, interval_days, next_due_at, reps)` if a state exists.
pub async fn get_srs_state(db: &Db, page_id: &str) -> Result<Option<(f64, f64, i64, i64)>> {
    let row: Option<(f64, f64, i64, i64)> = sqlx::query_as(
        "SELECT ease_factor, interval_days, next_due_at, reps
           FROM srs_state WHERE page_id = ?1",
    )
    .bind(page_id)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row)
}

// ── Experiments ─────────────────────────────────────────────
//
// CRUD helpers for the per-paper experiment FSM. The pipeline owns
// status transitions; UI commands only ever write through one of these
// helpers so the FSM stays auditable from a single file.

#[allow(clippy::too_many_arguments)]
pub async fn insert_experiment(
    db: &Db,
    id: &str,
    paper_id: &str,
    parent_id: Option<&str>,
    version: i64,
    title: &str,
    status: &str,
    refine_prompt: Option<&str>,
) -> Result<()> {
    let ts = now();
    sqlx::query(
        "INSERT INTO experiments (
            id, paper_id, parent_experiment_id, version, title, status,
            inputs_schema, outputs_schema, test_attempts,
            refine_prompt, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '[]', '[]', 0, ?7, ?8, ?8)",
    )
    .bind(id)
    .bind(paper_id)
    .bind(parent_id)
    .bind(version)
    .bind(title)
    .bind(status)
    .bind(refine_prompt)
    .bind(ts)
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_experiment_status(
    db: &Db,
    id: &str,
    status: &str,
    plausibility_score: Option<i64>,
    plausibility_reason: Option<&str>,
    approach_summary: Option<&str>,
    error_message: Option<&str>,
) -> Result<()> {
    // We funnel `error_message` into `test_last_output` so the UI has a
    // single field to surface failure reason regardless of which stage
    // bombed. Score/reason/approach are upserted via COALESCE so callers
    // can patch one field at a time without clobbering earlier writes.
    sqlx::query(
        "UPDATE experiments SET
            status              = ?1,
            plausibility_score  = COALESCE(?2, plausibility_score),
            plausibility_reason = COALESCE(?3, plausibility_reason),
            approach_summary    = COALESCE(?4, approach_summary),
            test_last_output    = COALESCE(?5, test_last_output),
            updated_at          = ?6
         WHERE id = ?7",
    )
    .bind(status)
    .bind(plausibility_score)
    .bind(plausibility_reason)
    .bind(approach_summary)
    .bind(error_message)
    .bind(now())
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_experiment_artifacts(
    db: &Db,
    id: &str,
    inputs_schema: &str,
    outputs_schema: &str,
    python_code: &str,
    python_tests: &str,
    test_attempts: i64,
    test_last_output: Option<&str>,
    model_id: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE experiments SET
            inputs_schema    = ?1,
            outputs_schema   = ?2,
            python_code      = ?3,
            python_tests     = ?4,
            test_attempts    = ?5,
            test_last_output = ?6,
            model_id         = COALESCE(?7, model_id),
            updated_at       = ?8
         WHERE id = ?9",
    )
    .bind(inputs_schema)
    .bind(outputs_schema)
    .bind(python_code)
    .bind(python_tests)
    .bind(test_attempts)
    .bind(test_last_output)
    .bind(model_id)
    .bind(now())
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn get_experiment(db: &Db, id: &str) -> Result<Option<ExperimentRow>> {
    let row: Option<ExperimentRow> = sqlx::query_as("SELECT * FROM experiments WHERE id = ?1")
        .bind(id)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row)
}

pub async fn list_experiments_for_paper(db: &Db, paper_id: &str) -> Result<Vec<ExperimentRow>> {
    let rows: Vec<ExperimentRow> =
        sqlx::query_as("SELECT * FROM experiments WHERE paper_id = ?1 ORDER BY created_at DESC")
            .bind(paper_id)
            .fetch_all(&db.pool)
            .await?;
    Ok(rows)
}

pub async fn delete_experiment(db: &Db, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM experiments WHERE id = ?1")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn insert_experiment_run(
    db: &Db,
    experiment_id: &str,
    ts: i64,
    inputs_json: &str,
    outputs_json: Option<&str>,
    error_message: Option<&str>,
    duration_ms: Option<i64>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO experiment_runs
            (experiment_id, ts, inputs_json, outputs_json, error_message, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(experiment_id)
    .bind(ts)
    .bind(inputs_json)
    .bind(outputs_json)
    .bind(error_message)
    .bind(duration_ms)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn list_experiment_runs(
    db: &Db,
    experiment_id: &str,
    limit: i64,
) -> Result<Vec<ExperimentRunRow>> {
    let rows: Vec<ExperimentRunRow> = sqlx::query_as(
        "SELECT * FROM experiment_runs
          WHERE experiment_id = ?1
          ORDER BY ts DESC
          LIMIT ?2",
    )
    .bind(experiment_id)
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

// ── Comparisons ─────────────────────────────────────────────
//
// Two-paper diffs. Like experiments, status writes funnel through a
// single helper so the FSM stays auditable from one place.

pub async fn insert_comparison(
    db: &Db,
    id: &str,
    paper_a_id: &str,
    paper_b_id: &str,
    status: &str,
    ts: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO comparisons (id, paper_a_id, paper_b_id, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
    )
    .bind(id)
    .bind(paper_a_id)
    .bind(paper_b_id)
    .bind(status)
    .bind(ts)
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn update_comparison_status(
    db: &Db,
    id: &str,
    status: &str,
    html_path: Option<&str>,
    approach_summary: Option<&str>,
    error_message: Option<&str>,
    model_id: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE comparisons SET
            status            = ?1,
            html_path         = COALESCE(?2, html_path),
            approach_summary  = COALESCE(?3, approach_summary),
            error_message     = ?4,
            model_id          = COALESCE(?5, model_id),
            updated_at        = ?6
         WHERE id = ?7",
    )
    .bind(status)
    .bind(html_path)
    .bind(approach_summary)
    .bind(error_message)
    .bind(model_id)
    .bind(now())
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn get_comparison(db: &Db, id: &str) -> Result<Option<ComparisonRow>> {
    let row: Option<ComparisonRow> = sqlx::query_as("SELECT * FROM comparisons WHERE id = ?1")
        .bind(id)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row)
}

/// Idempotency: `compare_with_paper(a, b)` looks up the existing row before
/// inserting a new one. We DON'T normalize ordering — `(a, b)` and `(b, a)`
/// are treated as distinct directions because the diff text leads with
/// paper A and the user picks the direction.
pub async fn find_comparison_by_pair(
    db: &Db,
    paper_a_id: &str,
    paper_b_id: &str,
) -> Result<Option<ComparisonRow>> {
    let row: Option<ComparisonRow> =
        sqlx::query_as("SELECT * FROM comparisons WHERE paper_a_id = ?1 AND paper_b_id = ?2")
            .bind(paper_a_id)
            .bind(paper_b_id)
            .fetch_optional(&db.pool)
            .await?;
    Ok(row)
}

/// Joined row used by `list_comparisons_for_paper`. Carries the
/// denormalized titles so the picker can render without a follow-up
/// papers fetch.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ComparisonWithTitlesRow {
    pub id: String,
    pub paper_a_id: String,
    pub paper_b_id: String,
    pub status: String,
    pub html_path: Option<String>,
    pub approach_summary: Option<String>,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub paper_a_title: String,
    pub paper_b_title: String,
}

/// All comparisons whose paper-a OR paper-b matches `paper_id`. Joined
/// to `papers` twice to populate denormalized titles. Ordered by
/// `created_at DESC` to match the IPC contract.
pub async fn list_comparisons_for_paper(
    db: &Db,
    paper_id: &str,
) -> Result<Vec<ComparisonWithTitlesRow>> {
    let rows: Vec<ComparisonWithTitlesRow> = sqlx::query_as(
        "SELECT c.id              AS id,
                c.paper_a_id      AS paper_a_id,
                c.paper_b_id      AS paper_b_id,
                c.status          AS status,
                c.html_path       AS html_path,
                c.approach_summary AS approach_summary,
                c.error_message   AS error_message,
                c.created_at      AS created_at,
                c.updated_at      AS updated_at,
                pa.title          AS paper_a_title,
                pb.title          AS paper_b_title
           FROM comparisons c
           JOIN papers pa ON pa.id = c.paper_a_id
           JOIN papers pb ON pb.id = c.paper_b_id
          WHERE c.paper_a_id = ?1 OR c.paper_b_id = ?1
          ORDER BY c.created_at DESC",
    )
    .bind(paper_id)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}

pub async fn delete_comparison(db: &Db, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM comparisons WHERE id = ?1")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

// ── Page reading-level cache ────────────────────────────────
//
// The reading-level slider is INSTANT for previously-generated levels:
// the row carries a `level_cache` JSON map of `level → html_path`. When
// a level is cached we just flip `current_level` (and `html_path` to
// the cached one); when it's a miss the pipeline regenerates and merges
// the new entry in via `update_page_level_cache`.

pub async fn update_page_level(db: &Db, page_id: &str, current_level: &str) -> Result<()> {
    sqlx::query("UPDATE pages SET current_level = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(current_level)
        .bind(now())
        .bind(page_id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// Merge `{level: html_path}` into the page row's `level_cache` JSON
/// blob. Reads the current value, mutates it, writes it back.
/// Single-statement SQLite UPDATE so we don't need an explicit txn — the
/// row is read + written under the same connection lock.
pub async fn update_page_level_cache(
    db: &Db,
    page_id: &str,
    level: &str,
    html_path: &str,
) -> Result<()> {
    let row: Option<(String,)> = sqlx::query_as("SELECT level_cache FROM pages WHERE id = ?1")
        .bind(page_id)
        .fetch_optional(&db.pool)
        .await?;
    let cache_str = row.map(|r| r.0).unwrap_or_else(|| "{}".to_string());
    let mut cache: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&cache_str).unwrap_or_default();
    cache.insert(
        level.to_string(),
        serde_json::Value::String(html_path.to_string()),
    );
    let merged = serde_json::Value::Object(cache).to_string();
    sqlx::query("UPDATE pages SET level_cache = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&merged)
        .bind(now())
        .bind(page_id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn list_due_for_review(db: &Db, now_ts: i64) -> Result<Vec<DueRow>> {
    let rows: Vec<DueRow> = sqlx::query_as(
        "SELECT s.page_id      AS page_id,
                p.paper_id     AS paper_id,
                p.title        AS title,
                s.next_due_at  AS next_due_at
           FROM srs_state s
           JOIN pages p ON p.id = s.page_id
          WHERE s.next_due_at <= ?1
          ORDER BY s.next_due_at ASC
          LIMIT 50",
    )
    .bind(now_ts)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows)
}
