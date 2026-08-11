//! `import_pdf` stage.
//!
//! The actual file copy happened during `ingest::import_pdf` before the
//! pipeline was scheduled — this stage just verifies the row + file are
//! still present so we fail fast (and visibly) before kicking off the
//! expensive render/OCR work.

use anyhow::{anyhow, Result};
use tauri::Runtime;

use super::Ctx;
use crate::db::queries;

pub(super) async fn import_pdf<R: Runtime>(ctx: &Ctx<R>) -> Result<()> {
    let paper = queries::get_paper(&ctx.db, &ctx.paper_id)
        .await?
        .ok_or_else(|| anyhow!("paper row missing"))?;
    if paper.file_path.is_empty() || !std::path::Path::new(&paper.file_path).exists() {
        return Err(anyhow!("paper file missing on disk: {}", paper.file_path));
    }
    Ok(())
}
