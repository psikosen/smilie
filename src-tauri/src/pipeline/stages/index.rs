//! `index_pages` stage — populates the FTS table from the saved
//! structured-block JSON for each page.
//!
//! Idempotent on retry without an explicit clear: `index_page_fts`
//! deletes the prior FTS row by page_id before inserting, so re-running
//! the stage rebuilds each page's index in place.

use anyhow::Result;
use tauri::Runtime;

use super::Ctx;
use crate::db::queries;

pub(super) async fn index_pages<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    let rows = queries::list_pages_for_paper(&ctx.db, &ctx.paper_id).await?;
    for row in rows {
        // Reading-level support: artifacts live at
        // `<pages>/<id>/<level>/{index.html,source.json}`. Index the
        // currently-displayed level — switching levels later doesn't
        // re-index because the underlying source content is the same;
        // the slider only changes tone, not the indexed concepts.
        let level = if row.current_level.trim().is_empty() {
            "default"
        } else {
            row.current_level.as_str()
        };
        // Index from the structured-block JSON when we have it (cheap
        // plain text — no HTML, no SDK noise) and only fall back to the
        // rendered HTML for legacy pages with no source.json. This
        // keeps the FTS index much smaller than the prior "index the
        // whole document" path.
        let source_json_path = ctx
            .pages_dir()?
            .join(&row.id)
            .join(level)
            .join("source.json");
        let body = match std::fs::read_to_string(&source_json_path) {
            Ok(json) => extract_block_text(&json),
            Err(_) => {
                let html_path = ctx
                    .pages_dir()?
                    .join(&row.id)
                    .join(level)
                    .join("index.html");
                std::fs::read_to_string(&html_path).unwrap_or_default()
            }
        };
        queries::index_page_fts(&ctx.db, &row.id, &row.paper_id, &row.title, &body).await?;
    }
    Ok(())
}

/// Pull plain text out of a saved block-spec JSON for the FTS index.
/// Lenient — returns whatever string content it can find, joined with
/// spaces. Bad JSON returns "" so the page is still indexed by title.
fn extract_block_text(source_json: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(source_json) else {
        return String::new();
    };
    let mut out = String::new();
    fn walk(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::String(s) => {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(s);
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    // Skip metadata-only fields that aren't reading text.
                    if matches!(
                        k.as_str(),
                        "type" | "kind" | "id" | "level" | "ordered" | "answer"
                    ) {
                        continue;
                    }
                    walk(v, out);
                }
            }
            _ => {}
        }
    }
    walk(&value, &mut out);
    out
}
