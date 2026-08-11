//! Paper-level commands: ingest, list, get, delete.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use super::{CmdError, CmdResult};
use crate::app_state::AppState;
use crate::db::queries;
use crate::events::emit_paper_updated;
use crate::ingest;

#[derive(Debug, Serialize)]
pub struct PaperSummary {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub page_count: i64,
    pub processing_status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PageDto {
    pub id: String,
    pub paper_id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub page_type: String,
    pub source_sections: Vec<String>,
    pub source_pages: Vec<i64>,
    pub html_path: String,
    pub status: String,
    pub validation_status: Option<String>,
    pub order_index: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// Reading level currently displayed for this page — one of
    /// `eli5|default|expert`. Migration 005 backfills `default` for
    /// existing rows.
    pub current_level: String,
    /// Levels that have already been generated and cached server-side.
    /// Switching to one of these is instant; anything else triggers
    /// regeneration.
    pub cached_levels: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct JobDto {
    pub id: String,
    pub paper_id: String,
    pub stage: String,
    pub status: String,
    pub progress: f64,
    pub current_model: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i64,
}

#[derive(Debug, Serialize)]
pub struct PaperDetail {
    pub paper: PaperSummary,
    pub pages: Vec<PageDto>,
    pub active_job: Option<JobDto>,
}

#[tauri::command]
pub async fn ingest_pdf(file_path: String, state: State<'_, Arc<AppState>>) -> CmdResult<String> {
    let path = PathBuf::from(file_path);
    let paper_id = ingest::import_pdf(&state.db, &state.paths.papers_dir, &path).await?;

    // Kick off the pipeline. Do not await — the command returns the id
    // immediately so the UI can navigate.
    state.jobs.start(&paper_id).await?;
    emit_paper_updated(&state.handle, &paper_id);
    Ok(paper_id)
}

#[tauri::command]
pub async fn list_papers(state: State<'_, Arc<AppState>>) -> CmdResult<Vec<PaperSummary>> {
    let rows = queries::list_papers(&state.db).await?;
    Ok(rows.into_iter().map(to_summary).collect())
}

#[tauri::command]
pub async fn get_paper(id: String, state: State<'_, Arc<AppState>>) -> CmdResult<PaperDetail> {
    let paper = queries::get_paper(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError(format!("paper not found: {id}")))?;
    let pages = queries::list_pages_for_paper(&state.db, &id).await?;
    let job = queries::get_active_job(&state.db, &id).await?;

    Ok(PaperDetail {
        paper: to_summary(paper),
        pages: pages.into_iter().map(to_page_dto).collect(),
        active_job: job.map(to_job_dto),
    })
}

/// Return the absolute filesystem path of the original ingested PDF for
/// a paper. The frontend wraps this with `convertFileSrc(path, "asset")`
/// to load it inside the PDF viewer iframe — `tauri.conf.json`'s
/// `assetProtocol.scope` already covers the papers dir.
#[tauri::command]
pub async fn get_paper_pdf_url(
    paper_id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<String> {
    let row = queries::get_paper(&state.db, &paper_id)
        .await?
        .ok_or_else(|| CmdError(format!("paper not found: {paper_id}")))?;
    if row.file_path.trim().is_empty() {
        return Err(CmdError(format!(
            "paper {paper_id} has no ingested file_path yet"
        )));
    }
    Ok(row.file_path)
}

/// Open the paper's original PDF in the OS's default app. Bypasses
/// `tauri-plugin-shell::Shell::open` (deprecated in 2.3.x and silently
/// failing on macOS in some setups) by spawning the platform's standard
/// "open this file" CLI directly. No new dep, no plugin permission to
/// configure.
#[tauri::command]
pub async fn open_paper_in_default_app(
    paper_id: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<()> {
    let row = queries::get_paper(&state.db, &paper_id)
        .await?
        .ok_or_else(|| CmdError(format!("paper not found: {paper_id}")))?;
    if row.file_path.trim().is_empty() {
        return Err(CmdError(format!(
            "paper {paper_id} has no ingested file_path yet"
        )));
    }
    spawn_default_open(&row.file_path)
        .map_err(|e| CmdError(format!("could not launch default app: {e}")))
}

#[cfg(target_os = "macos")]
fn spawn_default_open(path: &str) -> std::io::Result<()> {
    std::process::Command::new("/usr/bin/open")
        .arg(path)
        .spawn()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn spawn_default_open(path: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn spawn_default_open(path: &str) -> std::io::Result<()> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", path])
        .spawn()?;
    Ok(())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn spawn_default_open(_path: &str) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "open-in-default-app is desktop-only",
    ))
}

#[tauri::command]
pub async fn delete_paper(id: String, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    queries::delete_paper(&state.db, &id).await?;
    // Best-effort cleanup of on-disk artifacts.
    let _ = std::fs::remove_file(state.paths.papers_dir.join(format!("{id}.pdf")));
    let _ = std::fs::remove_dir_all(state.paths.page_images_dir.join(&id));
    // Tell the frontend the library changed so it can refresh.
    emit_paper_updated(&state.handle, &id);
    Ok(())
}

fn to_summary(row: crate::db::PaperRow) -> PaperSummary {
    let authors: Vec<String> = serde_json::from_str(&row.authors).unwrap_or_default();
    PaperSummary {
        id: row.id,
        title: row.title,
        authors,
        page_count: row.page_count,
        processing_status: row.processing_status,
        created_at: row.created_at,
        updated_at: row.updated_at,
        thumbnail_path: row.thumbnail_path,
    }
}

pub(super) fn to_page_dto(row: crate::db::PageRow) -> PageDto {
    let sections: Vec<String> = serde_json::from_str(&row.source_sections).unwrap_or_default();
    let pages: Vec<i64> = serde_json::from_str(&row.source_pages).unwrap_or_default();
    let cached_levels = parse_cached_levels(&row.level_cache);
    PageDto {
        id: row.id,
        paper_id: row.paper_id,
        title: row.title,
        page_type: row.page_type,
        source_sections: sections,
        source_pages: pages,
        html_path: row.html_path,
        status: row.status,
        validation_status: row.validation_status,
        order_index: row.order_index,
        created_at: row.created_at,
        updated_at: row.updated_at,
        current_level: row.current_level,
        cached_levels,
    }
}

/// Parse the JSON keys out of the `level_cache` blob. Defensive: a
/// malformed value falls back to an empty vec so a corrupt row never
/// kills the page list.
pub(super) fn parse_cached_levels(level_cache_json: &str) -> Vec<String> {
    let parsed: serde_json::Value =
        serde_json::from_str(level_cache_json).unwrap_or_else(|_| serde_json::json!({}));
    match parsed {
        serde_json::Value::Object(map) => map.into_iter().map(|(k, _)| k).collect(),
        _ => Vec::new(),
    }
}

fn to_job_dto(row: crate::db::JobRow) -> JobDto {
    JobDto {
        id: row.id,
        paper_id: row.paper_id,
        stage: row.stage,
        status: row.status,
        progress: row.progress,
        current_model: row.current_model,
        error_message: row.error_message,
        retry_count: row.retry_count,
    }
}
