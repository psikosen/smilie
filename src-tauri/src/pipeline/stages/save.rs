//! `save_pages` stage — stamps the on-disk html_path into each page
//! row now that the files are final.
//!
//! Pure update; idempotent by construction. Reading-level support lives
//! in `generate_pages` (which writes per-level subdirs), so this stage
//! resolves the path against the row's `current_level` to land at the
//! same file the generator just wrote.

use anyhow::Result;
use tauri::Runtime;

use super::Ctx;
use crate::db::queries;

pub(super) async fn save_pages<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    let rows = queries::list_pages_for_paper(&ctx.db, &ctx.paper_id).await?;
    for row in rows {
        let level = if row.current_level.trim().is_empty() {
            "default".to_string()
        } else {
            row.current_level.clone()
        };
        let path = ctx
            .pages_dir()?
            .join(&row.id)
            .join(&level)
            .join("index.html")
            .to_string_lossy()
            .to_string();
        queries::set_page_html_path(&ctx.db, &row.id, &path).await?;
    }
    Ok(())
}
