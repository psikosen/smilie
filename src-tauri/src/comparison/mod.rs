//! Two-paper comparison pipeline.
//!
//! Mirrors the experiment pipeline shape: a single orchestrator entry
//! point spawned via `tokio::spawn` from the Tauri command. Comparisons
//! produce one structured-block HTML page on disk that the frontend
//! loads in an iframe — the diff "page" reuses the existing
//! `pipeline::render::render_page` so the visual treatment matches a
//! learning page.
//!
//! Status FSM:
//!   `analyzing → ready | failed`

mod pipeline;
#[cfg(test)]
mod tests;

pub use pipeline::run_comparison_pipeline;
