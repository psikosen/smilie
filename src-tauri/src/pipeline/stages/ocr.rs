//! `ocr_extract` stage — runs the OCR runtime over each rendered page
//! image and persists the resulting blocks.
//!
//! Idempotent on retry: clears `ocr_blocks` for the paper on entry so a
//! re-run after a partial failure starts from an empty slate. Without
//! this, the structure stage would feed the reasoning model duplicated
//! content for any pages OCR'd on the prior attempt.

use anyhow::{Context, Result};
use tauri::Runtime;

use super::Ctx;
use crate::db::{queries, Db};
use crate::events::{emit_pipeline, PipelineEvent};
use crate::runtime::{OcrBlocks, PageImage};

pub(super) async fn ocr_extract<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    queries::clear_ocr_blocks_for_paper(&ctx.db, &ctx.paper_id).await?;

    let img_dir = ctx.page_images_dir()?;
    let mut entries: Vec<_> = std::fs::read_dir(&img_dir)
        .with_context(|| format!("listing {}", img_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "png")
                .unwrap_or(false)
                && !e.file_name().to_string_lossy().starts_with("thumbnail")
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let total = entries.len() as f64;
    for (i, entry) in entries.iter().enumerate() {
        let page_number = (i as u32) + 1;
        let path = entry.path();
        let page = PageImage {
            path: path.to_string_lossy().to_string(),
            page_number,
            width: 0,
            height: 0,
        };
        let blocks: OcrBlocks = ctx.runtime.ocr.ocr(page).await?;
        persist_blocks(&ctx.db, &ctx.paper_id, &blocks).await?;

        // Each page-image PNG is multi-MB at 1280px+. Once we've
        // extracted the blocks we never read it again — drop it to
        // free disk + page cache. The thumbnail (used by the library
        // card) stays.
        let _ = std::fs::remove_file(&path);

        emit_pipeline(
            &ctx.handle,
            &PipelineEvent::StageProgress {
                job_id: ctx.job_id.clone(),
                stage: "ocr_extract".into(),
                progress: (i as f64 + 1.0) / total.max(1.0),
            },
        );
    }
    Ok(())
}

async fn persist_blocks(db: &Db, paper_id: &str, blocks: &OcrBlocks) -> Result<()> {
    for block in &blocks.blocks {
        let bbox = block
            .bbox
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_default());
        sqlx::query(
            "INSERT INTO ocr_blocks (id, paper_id, page_number, block_type, text, bbox, confidence, source_model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(paper_id)
        .bind(block.page_number as i64)
        .bind(&block.block_type)
        .bind(&block.text)
        .bind(bbox)
        .bind(block.confidence.map(f64::from))
        .bind("glm-ocr")
        .execute(&db.pool)
        .await?;
    }
    Ok(())
}
