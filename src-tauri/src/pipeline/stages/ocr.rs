//! `ocr_extract` stage — runs the OCR runtime over each rendered page
//! image and persists the resulting blocks.
//!
//! Idempotent on retry: each page is replaced atomically. Successfully
//! processed page images are deleted, so a restart keeps their committed
//! OCR blocks and continues with the remaining images.

use anyhow::{Context, Result};
use tauri::Runtime;

use super::Ctx;
use crate::db::Db;
use crate::events::{emit_pipeline, PipelineEvent};
use crate::runtime::{OcrBlocks, PageImage};

pub(super) async fn ocr_extract<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    let img_dir = ctx.page_images_dir()?;
    let mut entries: Vec<_> = std::fs::read_dir(&img_dir)
        .with_context(|| format!("listing {}", img_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| page_number_from_path(&e.path()).is_some())
        .collect();
    entries.sort_by_key(|e| page_number_from_path(&e.path()).unwrap_or(u32::MAX));

    let total = entries.len() as f64;
    for (i, entry) in entries.iter().enumerate() {
        let path = entry.path();
        let page_number = page_number_from_path(&path)
            .with_context(|| format!("invalid rendered page filename: {}", path.display()))?;
        let page = PageImage {
            path: path.to_string_lossy().to_string(),
            page_number,
            width: 0,
            height: 0,
        };
        let blocks: OcrBlocks = ctx.runtime.ocr.ocr(page).await?;
        persist_blocks(&ctx.db, &ctx.paper_id, page_number, &blocks).await?;

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

fn page_number_from_path(path: &std::path::Path) -> Option<u32> {
    if path.extension().and_then(|value| value.to_str()) != Some("png") {
        return None;
    }
    path.file_stem()
        .and_then(|value| value.to_str())
        .and_then(|value| value.strip_prefix("page_"))
        .and_then(|value| value.parse().ok())
}

async fn persist_blocks(
    db: &Db,
    paper_id: &str,
    page_number: u32,
    blocks: &OcrBlocks,
) -> Result<()> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM ocr_blocks WHERE paper_id = ?1 AND page_number = ?2")
        .bind(paper_id)
        .bind(i64::from(page_number))
        .execute(&mut *tx)
        .await?;

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
        .bind(i64::from(page_number))
        .bind(&block.block_type)
        .bind(&block.text)
        .bind(bbox)
        .bind(block.confidence.map(f64::from))
        .bind("glm-ocr")
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{page_number_from_path, persist_blocks};
    use crate::db::{queries, Db};
    use crate::runtime::{OcrBlock, OcrBlocks};

    #[test]
    fn parses_original_page_number_from_rendered_filename() {
        assert_eq!(page_number_from_path(Path::new("page_0014.png")), Some(14));
        assert_eq!(page_number_from_path(Path::new("page_0001.png")), Some(1));
    }

    #[test]
    fn ignores_thumbnails_and_unrelated_images() {
        assert_eq!(page_number_from_path(Path::new("thumbnail.png")), None);
        assert_eq!(page_number_from_path(Path::new("figure_0001.png")), None);
        assert_eq!(page_number_from_path(Path::new("page_0001.jpg")), None);
    }

    #[tokio::test]
    async fn replacing_one_page_preserves_other_committed_pages() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = Db::open(&temp.path().join("smilie.db")).await.unwrap();
        db.run_migrations().await.unwrap();
        let paper_id = queries::insert_paper(&db, "Paper", "[]", "/tmp/paper.pdf", None)
            .await
            .unwrap();
        let blocks = |text: &str| OcrBlocks {
            blocks: vec![OcrBlock {
                page_number: 999,
                block_type: "text".into(),
                text: Some(text.into()),
                bbox: None,
                confidence: Some(1.0),
            }],
        };

        persist_blocks(&db, &paper_id, 14, &blocks("old page fourteen"))
            .await
            .unwrap();
        persist_blocks(&db, &paper_id, 15, &blocks("page fifteen"))
            .await
            .unwrap();
        persist_blocks(&db, &paper_id, 14, &blocks("new page fourteen"))
            .await
            .unwrap();

        let rows: Vec<(i64, Option<String>)> = sqlx::query_as(
            "SELECT page_number, text FROM ocr_blocks WHERE paper_id = ?1 ORDER BY page_number",
        )
        .bind(&paper_id)
        .fetch_all(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![
                (14, Some("new page fourteen".into())),
                (15, Some("page fifteen".into())),
            ]
        );
    }
}
