//! PDF page rendering via pdfium-render.
//!
//! `pdfium-render` is a wrapper over Google's pdfium C++ library. That
//! library is NOT statically linked — it must be present as a dynamic
//! library at runtime. We resolve the binding in this order:
//!
//!   1. `SMILIE_PDFIUM_LIB` env var (absolute path to the dylib)
//!   2. `<repo>/assets/pdfium/<platform-specific-name>` (shipped by the
//!      `scripts/install_pdfium.sh` helper)
//!   3. System library search path (what `Pdfium::default()` does)
//!
//! If none resolve, return a clean `Err` so the pipeline surfaces a
//! user-visible message instead of panicking the tokio worker.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use pdfium_render::prelude::*;

#[allow(dead_code)] // Fields read via Debug formatting in pipeline stages.
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

fn bind_pdfium() -> Result<Pdfium> {
    // 1. env override
    if let Ok(path) = std::env::var("SMILIE_PDFIUM_LIB") {
        if !path.is_empty() {
            let bindings = Pdfium::bind_to_library(&path)
                .with_context(|| format!("loading pdfium from SMILIE_PDFIUM_LIB={path}"))?;
            return Ok(Pdfium::new(bindings));
        }
    }

    // 2. shipped dylib under the repo/assets
    for candidate in shipped_candidates() {
        if candidate.exists() {
            let bindings = Pdfium::bind_to_library(&candidate)
                .with_context(|| format!("loading bundled pdfium from {}", candidate.display()))?;
            return Ok(Pdfium::new(bindings));
        }
    }

    // 3. system path — use the non-panicking variant
    let bindings = Pdfium::bind_to_system_library().map_err(|e| {
        anyhow!(
            "libpdfium not found. Install it with `scripts/install_pdfium.sh`, \
             set SMILIE_PDFIUM_LIB to an explicit path, or place the dylib \
             on the system library search path. underlying: {e}"
        )
    })?;
    Ok(Pdfium::new(bindings))
}

fn shipped_candidates() -> Vec<PathBuf> {
    let names: &[&str] = if cfg!(target_os = "macos") {
        &["libpdfium.dylib"]
    } else if cfg!(target_os = "windows") {
        &["pdfium.dll"]
    } else {
        &["libpdfium.so"]
    };

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.join("assets/pdfium"));
        roots.push(cwd.join("../assets/pdfium"));
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    roots.push(manifest_dir.join("../assets/pdfium"));
    roots.push(manifest_dir.join("assets/pdfium"));

    let mut out = Vec::new();
    for root in roots {
        for n in names {
            out.push(root.join(n));
        }
    }
    out
}

/// Renders every page of `pdf_path` into `out_dir` as `page_<n>.png`.
pub fn render_pages(
    pdf_path: &Path,
    out_dir: &Path,
    target_height_px: u32,
) -> Result<RenderOutput> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let pdfium = bind_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(pdf_path, None)
        .with_context(|| format!("loading pdf {}", pdf_path.display()))?;

    let mut out_pages = Vec::new();
    let mut thumbnail: Option<PathBuf> = None;

    let config = PdfRenderConfig::new()
        .set_target_height(target_height_px as i32)
        .render_form_data(false)
        .render_annotations(true);

    for (index, page) in document.pages().iter().enumerate() {
        let page_number = (index as u32) + 1;
        let bitmap = page
            .render_with_config(&config)
            .with_context(|| format!("rendering page {page_number}"))?;

        let image = bitmap.as_image();
        let rgba = image.to_rgba8();
        let (w, h) = rgba.dimensions();

        let png_path = out_dir.join(format!("page_{page_number:04}.png"));
        rgba.save(&png_path)
            .with_context(|| format!("saving page image {}", png_path.display()))?;

        if thumbnail.is_none() {
            let thumb_path = out_dir.join("thumbnail.png");
            let thumb = image::imageops::thumbnail(&rgba, 480, 600);
            thumb
                .save(&thumb_path)
                .with_context(|| format!("saving thumbnail {}", thumb_path.display()))?;
            thumbnail = Some(thumb_path);
        }

        out_pages.push(RenderedPage {
            page_number,
            png_path,
            width: w,
            height: h,
        });

        drop(image);
    }

    Ok(RenderOutput {
        pages: out_pages,
        thumbnail: thumbnail.unwrap_or_else(|| out_dir.join("thumbnail.png")),
    })
}
