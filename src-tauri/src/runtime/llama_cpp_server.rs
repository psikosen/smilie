//! Optional fallback runtime that talks to a standalone `llama-server`
//! process over HTTP. Useful when the Rust bindings have platform build
//! issues. Not wired in v1 beyond the trait shape.

use std::path::PathBuf;

use async_trait::async_trait;

use super::types::*;
use super::ModelRuntime;

#[allow(dead_code)] // `label` + `endpoint` wired once the HTTP fallback ships.
pub struct LlamaCppServer {
    pub label: &'static str,
    pub endpoint: String,
    pub model_path: PathBuf,
}

impl LlamaCppServer {
    pub fn new(label: &'static str, endpoint: String, model_path: PathBuf) -> Self {
        Self {
            label,
            endpoint,
            model_path,
        }
    }
}

#[async_trait]
impl ModelRuntime for LlamaCppServer {
    async fn ocr(&self, _page: PageImage) -> Result<OcrBlocks, RuntimeError> {
        Err(RuntimeError::Inference(
            "llama-server runtime not wired yet".into(),
        ))
    }

    async fn reason(&self, _paper: PaperDocument) -> Result<LearningPlan, RuntimeError> {
        Err(RuntimeError::Inference(
            "llama-server runtime not wired yet".into(),
        ))
    }

    async fn generate(&self, _spec: PageSpec) -> Result<GeneratedPage, RuntimeError> {
        Err(RuntimeError::Inference(
            "llama-server runtime not wired yet".into(),
        ))
    }

    async fn analyze_experiment(
        &self,
        _paper: PaperDocument,
    ) -> Result<ExperimentAnalysis, RuntimeError> {
        Err(RuntimeError::Inference(
            "llama-server runtime not wired yet".into(),
        ))
    }

    async fn generate_experiment(
        &self,
        _paper: PaperDocument,
        _approach: &str,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        Err(RuntimeError::Inference(
            "llama-server runtime not wired yet".into(),
        ))
    }

    async fn refine_experiment(
        &self,
        _paper: PaperDocument,
        _prior: &PriorAttempt,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        Err(RuntimeError::Inference(
            "llama-server runtime not wired yet".into(),
        ))
    }

    async fn compare_papers(
        &self,
        _paper_a: PaperDocument,
        _paper_b: PaperDocument,
    ) -> Result<PageBlocks, RuntimeError> {
        Err(RuntimeError::Inference(
            "llama-server runtime not wired yet".into(),
        ))
    }

    fn status(&self) -> RuntimeStatus {
        if !self.model_path.exists() {
            return RuntimeStatus::MissingWeights {
                runtime: "llama-server".into(),
                expected_path: self.model_path.display().to_string(),
            };
        }
        RuntimeStatus::Ready {
            runtime: "llama-server".into(),
            model: self
                .model_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
        }
    }

    fn label(&self) -> &'static str {
        self.label
    }
}
