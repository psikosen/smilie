//! Smilie — scientific paper → interactive learning pages.
//!
//! This is the Tauri v2 Rust backend. The frontend (SvelteKit) talks to it
//! via `tauri::invoke` commands declared in `commands::*`.

#![allow(clippy::module_name_repetitions)]

mod app_state;
mod commands;
mod comparison;
// `db`, `pipeline`, and `runtime` are exposed as `pub` so the
// `tests/pipeline_smoke.rs` integration test (which only sees the
// public crate surface) can wire up its own DB + JobQueue + mock
// runtime. No frontend caller goes through the `smilie_lib::*` path,
// so this only affects test linkage.
pub mod db;
mod events;
mod experiment;
mod ingest;
mod manifest;
#[cfg(test)]
mod manifest_tests;
#[cfg(any(target_os = "android", target_os = "ios"))]
mod mobile_intake;
pub mod pipeline;
pub mod runtime;
mod srs;
#[cfg(test)]
mod structure_tests;
mod validation;

use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::app_state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .try_init();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init());

    // Shell plugin spawns the python sidecar (mlx-vlm). Mobile builds
    // never run python — they go through llama.cpp Rust bindings — so
    // we leave the plugin off the mobile build to keep the binary lean
    // and avoid linking process-spawn helpers that App Sandbox forbids.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder.plugin(tauri_plugin_shell::init());
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        builder = builder.plugin(tauri_plugin_deep_link::init());
    }

    builder
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let state = AppState::bootstrap(handle.clone()).await?;
                app.manage(Arc::new(state));

                // Mobile-only: subscribe to share-sheet / deep-link
                // events so a PDF picked from Files / Photos lands in
                // the ingest pipeline.
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    crate::mobile_intake::install(app.handle());
                }

                Ok::<(), anyhow::Error>(())
            })
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ingest_pdf,
            commands::list_papers,
            commands::get_paper,
            commands::get_page,
            commands::regenerate_page,
            commands::export_page,
            commands::delete_paper,
            commands::pause_job,
            commands::resume_job,
            commands::cancel_job,
            commands::get_settings,
            commands::update_settings,
            commands::list_runtime_status,
            commands::recheck_runtime_status,
            commands::reveal_models_folder,
            commands::search_pages,
            commands::record_quiz_attempt,
            commands::list_due_pages,
            commands::analyze_experiment,
            commands::list_experiments_for_paper,
            commands::get_experiment,
            commands::refine_experiment,
            commands::delete_experiment,
            commands::force_generate_experiment,
            commands::record_experiment_run,
            commands::list_experiment_runs,
            commands::experiment_runner_url,
            commands::get_paper_pdf_url,
            commands::open_paper_in_default_app,
            commands::set_page_reading_level,
            commands::compare_with_paper,
            commands::list_comparisons_for_paper,
            commands::get_comparison,
            commands::delete_comparison,
        ])
        .run(tauri::generate_context!())
        .expect("failed to launch Smilie");
}
