//! `validate_pages` stage — runs structural validation over each
//! ready page's HTML and stamps the result on the page row.
//!
//! Read-then-update on `pages.validation_status`; no rows owned, so
//! nothing to clear on retry.

use anyhow::Result;
use tauri::Runtime;

use super::Ctx;
use crate::db::queries;
use crate::validation;

pub(super) async fn validate_pages<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    let rows = queries::list_pages_for_paper(&ctx.db, &ctx.paper_id).await?;
    for row in rows {
        if row.status != "ready" {
            continue;
        }
        // Reading-level paths live at `<pages>/<id>/<level>/index.html`.
        // Resolve against the row's `current_level` (defaulting to
        // `default` for legacy / blank rows) so this stage targets the
        // file the generator just wrote.
        let level = if row.current_level.trim().is_empty() {
            "default"
        } else {
            row.current_level.as_str()
        };
        let path = ctx
            .pages_dir()?
            .join(&row.id)
            .join(level)
            .join("index.html");
        let html = std::fs::read_to_string(&path)?;
        let status = validation::validate_page_html(&html);
        queries::set_page_status(&ctx.db, &row.id, &row.status, Some(&status)).await?;
    }
    Ok(())
}
