//! PDF ingestion + page rendering.
//!
//! The page-image renderer (`pdf`) is desktop-only because it links
//! Google's pdfium via `dlopen`, which iOS App Sandbox forbids. Mobile
//! targets get the `pdf_mobile` shim that returns a clear "ingest from
//! desktop" error so the pipeline halts cleanly. A future revision will
//! plug iOS PDFKit / Android PdfRenderer behind the same surface.

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub mod pdf;

#[cfg(any(target_os = "android", target_os = "ios"))]
#[path = "pdf_mobile.rs"]
pub mod pdf;

pub mod structure;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
pub use structure::PaperJson;

use crate::db::{queries, Db};

/// Copy a user-dropped PDF into our app_data storage and insert a papers row.
pub async fn import_pdf(db: &Db, papers_dir: &Path, source_path: &Path) -> Result<String> {
    if !source_path.exists() {
        anyhow::bail!("file does not exist: {}", source_path.display());
    }
    if !source_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
    {
        anyhow::bail!("not a pdf: {}", source_path.display());
    }

    let original = source_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string());

    let provisional_title = source_path
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    // Insert row first so we have an id.
    let paper_id =
        queries::insert_paper(db, &provisional_title, "[]", "", original.as_deref()).await?;

    // Copy file into papers_dir/<id>.pdf
    fs::create_dir_all(papers_dir).with_context(|| format!("creating {}", papers_dir.display()))?;
    let dest = papers_dir.join(format!("{paper_id}.pdf"));
    fs::copy(source_path, &dest).with_context(|| format!("copying pdf to {}", dest.display()))?;

    // Patch in the resolved file path.
    sqlx::query("UPDATE papers SET file_path = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(dest.to_string_lossy().as_ref())
        .bind(Utc::now().timestamp())
        .bind(&paper_id)
        .execute(&db.pool)
        .await?;

    Ok(paper_id)
}
