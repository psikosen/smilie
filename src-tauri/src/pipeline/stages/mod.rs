//! Per-stage runners. Each stage reads the current paper state from the DB
//! and moves it forward one step. Stages that use runtimes will surface
//! `MissingWeights` errors intact so the UI can guide the user.
//!
//! Each stage that owns rows in a stage-specific table clears them on
//! entry — re-runs after a mid-stage failure must be idempotent so the
//! reasoning model never sees duplicate content from a partial prior
//! attempt. See per-stage modules for the exact contract.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tauri::{AppHandle, Manager, Runtime};

use crate::db::Db;
use crate::runtime::RuntimeSelector;

mod generate;
mod import;
mod index;
mod ocr;
mod plan;
mod render_pages;
mod save;
mod structure;
mod validate;

// `Ctx` is generic over the Tauri runtime so the smoke test can build
// it on top of `MockRuntime`. Production code uses `Ctx<Wry>` (inferred
// from the `JobQueue<Wry>` it lives inside).
pub struct Ctx<R: Runtime> {
    pub db: Db,
    pub runtime: Arc<RuntimeSelector>,
    pub handle: AppHandle<R>,
    pub paper_id: String,
    pub job_id: String,
}

impl<R: Runtime> Ctx<R> {
    pub(super) fn app_data(&self) -> Result<PathBuf> {
        // Test override: integration tests set this to a tempdir so the
        // pipeline never touches the user's real Application Support
        // directory. Gated behind `debug_assertions` so a malicious or
        // accidental env var in a release shell can't redirect the
        // production data path. Tests run with `cfg(test)` which implies
        // `debug_assertions`, so the smoke test is unaffected.
        #[cfg(debug_assertions)]
        {
            if let Ok(p) = std::env::var("SMILIE_APP_DATA_OVERRIDE") {
                if !p.is_empty() {
                    return Ok(PathBuf::from(p));
                }
            }
        }
        Ok(self.handle.path().app_data_dir()?)
    }

    pub(super) fn page_images_dir(&self) -> Result<PathBuf> {
        Ok(self.app_data()?.join("page_images").join(&self.paper_id))
    }

    pub(super) fn pages_dir(&self) -> Result<PathBuf> {
        Ok(self.app_data()?.join("pages"))
    }

    pub(super) fn paper_json_path(&self) -> Result<PathBuf> {
        Ok(self
            .app_data()?
            .join(format!("papers/{}.paper.json", self.paper_id)))
    }

    pub(super) fn plan_json_path(&self) -> Result<PathBuf> {
        Ok(self
            .app_data()?
            .join(format!("papers/{}.plan.json", self.paper_id)))
    }
}

pub async fn run_stage<R: Runtime>(stage: &str, ctx: &mut Ctx<R>) -> Result<()> {
    match stage {
        "import_pdf" => import::import_pdf(ctx).await,
        "render_pages" => render_pages::render_pages(ctx).await,
        "ocr_extract" => ocr::ocr_extract(ctx).await,
        "structure_paper" => structure::structure_paper(ctx).await,
        "build_learning_plan" => plan::build_learning_plan(ctx).await,
        "generate_pages" => generate::generate_pages(ctx).await,
        "validate_pages" => validate::validate_pages(ctx).await,
        "save_pages" => save::save_pages(ctx).await,
        "index_pages" => index::index_pages(ctx).await,
        "complete" => Ok(()),
        _ => Err(anyhow!("unknown stage: {stage}")),
    }
}
