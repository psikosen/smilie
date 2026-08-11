//! Model runtime abstraction.
//!
//! One trait, multiple impls (mlx-vlm sidecar on macOS; llama.cpp via
//! Rust bindings elsewhere; llama-server as fallback). Selected per-role
//! by the manifest.
//!
//! v1 ships real code paths. If weights are absent, callers get
//! `RuntimeError::MissingWeights` and the pipeline surfaces it to the UI
//! so the user can drop the file and retry.

pub mod llama_cpp_local;
pub mod llama_cpp_server;
// mlx-vlm spawns a python sidecar — desktop only. Mobile selector
// substitutes a stub for any model role mapped to "mlx-vlm" so the
// build links and the UI surfaces a clear error.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub mod mlx_vlm;
pub mod selector;
pub mod types;

pub use selector::RuntimeSelector;
pub use types::*;

use async_trait::async_trait;

use crate::ingest::PaperJson;

#[async_trait]
pub trait ModelRuntime: Send + Sync {
    async fn ocr(&self, page: PageImage) -> Result<OcrBlocks, RuntimeError>;

    async fn reason(&self, paper: PaperJson) -> Result<LearningPlan, RuntimeError>;

    async fn generate(&self, spec: PageSpec) -> Result<GeneratedPage, RuntimeError>;

    /// Decide whether a tractable Python reproduction of the paper's
    /// central claim is feasible, and propose an approach.
    async fn analyze_experiment(
        &self,
        paper: PaperJson,
    ) -> Result<ExperimentAnalysis, RuntimeError>;

    /// Produce a self-contained Python module + matching tests + the
    /// typed inputs/outputs schemas the UI uses to build controls.
    async fn generate_experiment(
        &self,
        paper: PaperJson,
        approach: &str,
    ) -> Result<ExperimentArtifacts, RuntimeError>;

    /// Patch a prior attempt — either to fix a test failure (inner
    /// loop) or to apply a user prompt (refine fork).
    async fn refine_experiment(
        &self,
        paper: PaperJson,
        prior: &PriorAttempt,
    ) -> Result<ExperimentArtifacts, RuntimeError>;

    /// Two-paper diff. Returns `PageBlocks` so the comparison renders
    /// through the existing `pipeline::render::render_page` — the
    /// frontend treats the resulting HTML identically to a learning
    /// page. The model is asked to surface shared concepts,
    /// contradictions, unique contributions, and a `diagram_flow` when
    /// both papers describe similar pipelines.
    async fn compare_papers(
        &self,
        paper_a: PaperJson,
        paper_b: PaperJson,
    ) -> Result<PageBlocks, RuntimeError>;

    fn status(&self) -> RuntimeStatus;

    /// Human-readable label shown in Settings → Runtime.
    #[allow(dead_code)]
    fn label(&self) -> &'static str;
}
