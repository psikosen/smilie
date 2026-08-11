//! Mobile shim for `ingest::pdf`.
//!
//! Same public surface as the desktop renderer (`render_pages`) so the
//! pipeline doesn't branch, but returns a structured error pointing the
//! user at the desktop until we wire native PDFKit (iOS) / PdfRenderer
//! (Android) shims. The shim still creates the output dir so the rest
//! of the pipeline can no-op cleanly.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[allow(dead_code)]
pub struct RenderedPage {
    pub page_number: u32,
    pub png_path: PathBuf,
    pub width: u32,
    pub height: u32,
}

pub struct RenderOutput {
    pub pages: Vec<RenderedPage>,
    pub thumbnail: PathBuf,
}

pub fn render_pages(
    _pdf_path: &Path,
    out_dir: &Path,
    _target_height_px: u32,
) -> Result<RenderOutput> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    Err(anyhow::anyhow!(
        "PDF rendering is not yet wired on this mobile platform. \
         Ingest the paper on a desktop instance for v1; mobile native \
         renderer (PDFKit / PdfRenderer) is on the post-MVP list. \
         See docs/mobile.md for the plan."
    ))
}
