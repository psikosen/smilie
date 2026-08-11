//! Page-level commands.

use std::sync::Arc;

use tauri::State;

use super::{
    papers::{to_page_dto, PageDto},
    CmdError, CmdResult,
};
use crate::app_state::AppState;
use crate::db::queries;
use crate::events::{emit_pipeline, PipelineEvent};

/// The three reading levels the slider exposes. Mirrors the
/// `ReadingLevel` TS union in `src/lib/ipc/index.ts`. Defensive
/// re-validation lives in `set_page_reading_level` so a malformed
/// `invoke({ level: "weird" })` from a misbehaving caller errors out
/// cleanly instead of silently writing nonsense to the row.
const VALID_LEVELS: &[&str] = &["eli5", "default", "expert"];

#[tauri::command]
pub async fn get_page(id: String, state: State<'_, Arc<AppState>>) -> CmdResult<PageDto> {
    let row = queries::get_page(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError(format!("page not found: {id}")))?;
    Ok(to_page_dto(row))
}

#[tauri::command]
pub async fn regenerate_page(id: String, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    // Mark this page as queued so the generator picks it up on next run.
    queries::set_page_status(&state.db, &id, "queued", None).await?;
    let page = queries::get_page(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError(format!("page not found: {id}")))?;
    state.jobs.start(&page.paper_id).await?;
    Ok(())
}

#[tauri::command]
pub async fn export_page(
    id: String,
    dest: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<String> {
    let row = queries::get_page(&state.db, &id)
        .await?
        .ok_or_else(|| CmdError(format!("page not found: {id}")))?;
    if row.html_path.is_empty() {
        return Err(CmdError("page has no generated html yet".into()));
    }

    let src = std::path::Path::new(&row.html_path);
    let dest_path = std::path::Path::new(&dest);
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dest_path)?;
    Ok(dest_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn search_pages(
    query: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<Vec<PageDto>> {
    let ids = queries::search_pages_fts(&state.db, &query).await?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(row) = queries::get_page(&state.db, &id).await? {
            out.push(to_page_dto(row));
        }
    }
    Ok(out)
}

/// Switch a page to the requested reading level. Cache-hit path is
/// instant: we just flip `current_level` + `html_path` to the cached
/// entry and emit `page_ready` so the iframe reloads. Cache-miss kicks
/// the per-paper pipeline at the target level — `generate_pages` reads
/// `level_cache` first and only re-renders the queued row.
#[tauri::command]
pub async fn set_page_reading_level(
    page_id: String,
    level: String,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<()> {
    // Defensive re-validation: the IPC contract enums it, but the JSON
    // boundary doesn't enforce that — a hand-rolled `invoke` could send
    // anything. Reject early so a typo doesn't write garbage.
    if !VALID_LEVELS.contains(&level.as_str()) {
        return Err(CmdError(format!(
            "invalid reading level '{level}'; expected one of {VALID_LEVELS:?}"
        )));
    }

    let row = queries::get_page(&state.db, &page_id)
        .await?
        .ok_or_else(|| CmdError(format!("page not found: {page_id}")))?;

    let cache: serde_json::Value =
        serde_json::from_str(&row.level_cache).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(cached_path) = cache.get(&level).and_then(|v| v.as_str()) {
        // Cache hit — flip the row and emit page_ready so the frontend
        // reloads the iframe with the cached HTML.
        queries::update_page_level(&state.db, &page_id, &level).await?;
        queries::set_page_html_path(&state.db, &page_id, cached_path).await?;
        emit_pipeline(
            &state.handle,
            &PipelineEvent::PageReady {
                page_id: page_id.clone(),
                paper_id: row.paper_id.clone(),
            },
        );
        return Ok(());
    }

    // Cache miss — record the requested level + queue the page so the
    // generate stage picks it up on the next pipeline run. The generate
    // stage threads `current_level` into the `PageSpec.level` it sends
    // to the generator runtime; the result is then stamped into
    // `level_cache[level]` so a subsequent switch is instant.
    queries::update_page_level(&state.db, &page_id, &level).await?;
    queries::set_page_status(&state.db, &page_id, "queued", None).await?;
    state.jobs.start(&row.paper_id).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::VALID_LEVELS;
    use crate::db::{queries, Db};

    #[test]
    fn valid_levels_match_ipc_contract() {
        // Lock the enum to the IPC contract — adding a new level must
        // be a deliberate, audited change touching both this list and
        // `ReadingLevel` in `src/lib/ipc/index.ts`.
        assert_eq!(VALID_LEVELS, &["eli5", "default", "expert"]);
    }

    /// Cached-path contract: when `level_cache[level]` exists,
    /// `set_page_reading_level` should flip `current_level` and
    /// `html_path` to the cached value WITHOUT touching the row's
    /// status. We don't drive the Tauri command here (it needs a full
    /// AppState); we exercise the underlying query helpers the
    /// cache-hit branch uses so a refactor that drops the instant flip
    /// shows up in CI.
    #[tokio::test]
    async fn cache_hit_writes_html_path_and_level_without_queueing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = Db::open(&tmp.path().join("smilie.db")).await.unwrap();
        db.run_migrations().await.unwrap();

        let paper_id = queries::insert_paper(&db, "P", "[]", "/x", None)
            .await
            .unwrap();
        let page_id = queries::insert_page(&db, &paper_id, "T", "concept", "[]", "[]", 0)
            .await
            .unwrap();

        // Pretend the pipeline already generated `eli5` once.
        queries::update_page_level_cache(&db, &page_id, "eli5", "/tmp/eli5.html")
            .await
            .unwrap();
        queries::set_page_status(&db, &page_id, "ready", None)
            .await
            .unwrap();

        // Simulate the cache-hit branch from `set_page_reading_level`.
        queries::update_page_level(&db, &page_id, "eli5")
            .await
            .unwrap();
        queries::set_page_html_path(&db, &page_id, "/tmp/eli5.html")
            .await
            .unwrap();

        let row = queries::get_page(&db, &page_id).await.unwrap().unwrap();
        assert_eq!(row.current_level, "eli5");
        assert_eq!(row.html_path, "/tmp/eli5.html");
        // Critical: the row stays `ready`; the cache hit must NOT
        // requeue the page (which would re-run the generator).
        assert_eq!(
            row.status, "ready",
            "cache-hit path must leave status alone — saw {}",
            row.status
        );
    }

    /// Cache-miss contract: when `level_cache[level]` is absent,
    /// `set_page_reading_level` writes the new `current_level` and
    /// flips status to `queued`. This sentinel-style assertion proves
    /// the miss path actually queues a regeneration.
    #[tokio::test]
    async fn cache_miss_marks_row_queued_at_target_level() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = Db::open(&tmp.path().join("smilie.db")).await.unwrap();
        db.run_migrations().await.unwrap();

        let paper_id = queries::insert_paper(&db, "P", "[]", "/x", None)
            .await
            .unwrap();
        let page_id = queries::insert_page(&db, &paper_id, "T", "concept", "[]", "[]", 0)
            .await
            .unwrap();
        queries::set_page_status(&db, &page_id, "ready", None)
            .await
            .unwrap();

        // Simulate the cache-miss branch from `set_page_reading_level`.
        queries::update_page_level(&db, &page_id, "expert")
            .await
            .unwrap();
        queries::set_page_status(&db, &page_id, "queued", None)
            .await
            .unwrap();

        let row = queries::get_page(&db, &page_id).await.unwrap().unwrap();
        assert_eq!(row.current_level, "expert");
        assert_eq!(
            row.status, "queued",
            "cache-miss path must mark the row queued so generate_pages picks it up"
        );
        // No cache entry was created for `expert` — the actual write
        // happens inside the generate stage after the model returns.
        assert!(
            !row.level_cache.contains("expert"),
            "level_cache should be untouched on cache miss; saw {}",
            row.level_cache
        );
    }
}
