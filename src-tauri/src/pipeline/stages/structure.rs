//! `structure_paper` stage — collapses persisted OCR blocks into a
//! single PaperJson document on disk.
//!
//! Read-only on the DB side except for the title backfill, so there's
//! nothing to clear on retry — overwriting the JSON file is naturally
//! idempotent.

use anyhow::{anyhow, Result};
use tauri::Runtime;

use super::Ctx;
use crate::db::queries;
use crate::ingest::structure::assemble;
use crate::runtime::OcrBlocks;

pub(super) async fn structure_paper<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    let paper = queries::get_paper(&ctx.db, &ctx.paper_id)
        .await?
        .ok_or_else(|| anyhow!("paper row missing"))?;

    let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT page_number, block_type, text FROM ocr_blocks
         WHERE paper_id = ?1 ORDER BY page_number ASC, id ASC",
    )
    .bind(&ctx.paper_id)
    .fetch_all(&ctx.db.pool)
    .await?;

    let mut by_page: std::collections::BTreeMap<u32, OcrBlocks> = Default::default();
    for (page_number, block_type, text) in rows {
        let entry = by_page.entry(page_number as u32).or_default();
        entry.blocks.push(crate::runtime::OcrBlock {
            page_number: page_number as u32,
            block_type,
            text,
            bbox: None,
            confidence: None,
        });
    }

    let pages: Vec<OcrBlocks> = by_page.into_values().collect();
    let mut doc = assemble(&ctx.paper_id, &pages);
    if doc.title.trim().is_empty() {
        doc.title = paper.title;
    } else {
        queries::update_paper_title(&ctx.db, &ctx.paper_id, &doc.title).await?;
    }

    let path = ctx.paper_json_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&doc)?)?;
    Ok(())
}
