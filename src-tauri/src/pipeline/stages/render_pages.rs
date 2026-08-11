//! `render_pages` stage — PDF → PNG per page.
//!
//! File is named `render_pages.rs` (not `render.rs`) on purpose: the
//! parent `pipeline::render` module already owns structured-block HTML
//! rendering. Keeping the stage file name distinct avoids any path /
//! module ambiguity in `pipeline::stages::render_pages` vs
//! `pipeline::render`.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use tauri::Runtime;

use super::Ctx;
use crate::db::queries;
use crate::ingest;

pub(super) async fn render_pages<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    let paper = queries::get_paper(&ctx.db, &ctx.paper_id)
        .await?
        .ok_or_else(|| anyhow!("paper row missing"))?;
    let out_dir = ctx.page_images_dir()?;
    let pdf_path = PathBuf::from(&paper.file_path);
    let height = pdf_render_height();

    let out =
        tokio::task::spawn_blocking(move || ingest::pdf::render_pages(&pdf_path, &out_dir, height))
            .await??;

    queries::update_paper_page_count(&ctx.db, &ctx.paper_id, out.pages.len() as i64).await?;
    queries::update_paper_thumbnail(&ctx.db, &ctx.paper_id, &out.thumbnail.to_string_lossy())
        .await?;
    Ok(())
}

/// Render height for the PDF→PNG step. Default 1280 (down from 1536):
/// produces page images that are still very legible for OCR at roughly
/// 30% less RAM + disk per page. Override with `SMILIE_PDF_HEIGHT=N` if
/// a particular paper needs higher fidelity (small dense figures).
fn pdf_render_height() -> u32 {
    std::env::var("SMILIE_PDF_HEIGHT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|h| *h >= 512 && *h <= 4096)
        .unwrap_or(1280)
}
