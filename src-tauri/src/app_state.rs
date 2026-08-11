//! Shared application state — owned by Tauri, cloned into commands.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use tauri::{AppHandle, Manager};
use tokio::sync::RwLock;

use crate::db::Db;
use crate::manifest::ModelManifest;
use crate::pipeline::JobQueue;
use crate::runtime::RuntimeSelector;

/// Global handles passed to every command.
pub struct AppState {
    pub db: Db,
    /// Held in app state so future Settings commands can edit the model
    /// manifest at runtime (swap roles, repoint to user-provided weights)
    /// without restarting the backend. No live readers yet — keep wired.
    #[allow(dead_code)]
    pub manifest: RwLock<ModelManifest>,
    pub runtime: Arc<RuntimeSelector>,
    pub jobs: Arc<JobQueue>,
    pub paths: AppPaths,
    pub handle: AppHandle,
}

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub app_data: PathBuf,
    pub papers_dir: PathBuf,
    pub page_images_dir: PathBuf,
    /// Pipeline stages currently re-derive this from `app_data` directly.
    /// Held here so future commands (export, search) have one canonical
    /// location; remove if the stages stop drifting.
    #[allow(dead_code)]
    pub pages_dir: PathBuf,
    pub models_dir: PathBuf,
    pub db_path: PathBuf,
    pub manifest_path: PathBuf,
}

impl AppPaths {
    fn bootstrap(handle: &AppHandle) -> anyhow::Result<Self> {
        let app_data = handle
            .path()
            .app_data_dir()
            .context("resolving app_data_dir")?;
        std::fs::create_dir_all(&app_data)?;

        let papers_dir = app_data.join("papers");
        let page_images_dir = app_data.join("page_images");
        let pages_dir = app_data.join("pages");
        let models_dir = app_data.join("models");

        for p in [&papers_dir, &page_images_dir, &pages_dir, &models_dir] {
            std::fs::create_dir_all(p)?;
        }

        let db_path = app_data.join("smilie.db");
        // Manifest lives under assets/models/manifest.json, shipped with the
        // app. We locate it relative to the resource dir, and if absent fall
        // back to the model dir inside app_data.
        let manifest_path = handle
            .path()
            .resource_dir()
            .ok()
            .map(|r| r.join("../assets/models/manifest.json"))
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or_else(|| app_data.join("models/manifest.json"));

        Ok(Self {
            app_data,
            papers_dir,
            page_images_dir,
            pages_dir,
            models_dir,
            db_path,
            manifest_path,
        })
    }
}

impl AppState {
    pub async fn bootstrap(handle: AppHandle) -> anyhow::Result<Self> {
        let paths = AppPaths::bootstrap(&handle)?;
        tracing::info!(?paths, "app paths resolved");

        let db = Db::open(&paths.db_path).await?;
        db.run_migrations().await?;
        match crate::pipeline::render::refresh_saved_pages(&db).await {
            Ok(count) if count > 0 => {
                tracing::info!(count, "upgraded saved learning pages");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(?error, "saved page upgrade skipped");
            }
        }

        let manifest = ModelManifest::load_or_default(&paths.manifest_path)?;
        let runtime = Arc::new(RuntimeSelector::build(&manifest, &paths)?);
        let jobs = Arc::new(JobQueue::new(
            db.clone(),
            Arc::clone(&runtime),
            handle.clone(),
        ));

        jobs.resume_on_startup().await?;

        Ok(Self {
            db,
            manifest: RwLock::new(manifest),
            runtime,
            jobs,
            paths,
            handle,
        })
    }
}
