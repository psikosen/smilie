//! Mobile share-sheet / deep-link → ingest pipeline glue.
//!
//! Compiled only on iOS / Android. The platform projects (created by
//! `tauri ios init` / `tauri android init`) declare a `CFBundleDocumentTypes`
//! entry for `com.adobe.pdf` (iOS) and an `<intent-filter>` for
//! `application/pdf` (Android); both forward the resulting URL through
//! `tauri-plugin-deep-link`'s subscriber API. Here we listen, copy the
//! file into our app data, and kick off the existing pipeline.
//!
//! The desktop build doesn't include this module — it has native drag-
//! drop instead.

#![cfg(any(target_os = "android", target_os = "ios"))]

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

use crate::app_state::AppState;
use crate::events::emit_paper_updated;
use crate::ingest;

/// Subscribe to deep-link / share-sheet events and route any incoming
/// `file://*.pdf` URLs into the ingest pipeline.
pub fn install(app: &AppHandle) {
    let handle = app.clone();
    let dl = app.deep_link();
    if let Err(e) = dl.on_open_url(move |event| {
        let urls: Vec<String> = event.urls().iter().map(|u| u.to_string()).collect();
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            for url in urls {
                if let Err(e) = ingest_one(&handle, &url).await {
                    tracing::warn!(error = ?e, %url, "share-intent ingest failed");
                }
            }
        });
    }) {
        tracing::warn!(error = ?e, "deep-link subscribe failed");
    }
}

async fn ingest_one(handle: &AppHandle, url: &str) -> anyhow::Result<()> {
    let path =
        url_to_path(url).ok_or_else(|| anyhow::anyhow!("unsupported share-intent url: {url}"))?;

    let state: tauri::State<'_, Arc<AppState>> = handle.state();
    let paper_id = ingest::import_pdf(&state.db, &state.paths.papers_dir, &path).await?;
    state.jobs.start(&paper_id).await?;
    emit_paper_updated(&state.handle, &paper_id);
    tracing::info!(%paper_id, "share-intent ingest queued");
    Ok(())
}

fn url_to_path(url: &str) -> Option<PathBuf> {
    // file:///... → strip scheme. content://... → not handled here; the
    // platform-side code is expected to copy the content:// stream into
    // a temp file before forwarding.
    if let Some(rest) = url.strip_prefix("file://") {
        // Drop the host portion if present.
        let rest = rest.trim_start_matches('/');
        return Some(PathBuf::from(format!("/{rest}")));
    }
    if url.starts_with('/') {
        return Some(PathBuf::from(url));
    }
    None
}
