//! Local Ollama runtime.
//!
//! Smilie already has structured prompts and parsers in the llama.cpp
//! runtime. This adapter reuses those contracts while delegating model
//! residency and request scheduling to the user's local Ollama service.
//! Multiple commands can submit work concurrently; Ollama reuses the one
//! loaded model and applies its configured parallelism/queue limits.

use std::process::{Command as StdCommand, Stdio as StdStdio};
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use super::llama_cpp_local::{
    build_compare_prompt, build_experiment_analyze_prompt, build_experiment_generate_prompt,
    build_experiment_refine_prompt, build_generate_prompt, build_reason_prompt, parse_blocks,
    parse_experiment_analysis, parse_experiment_artifacts, parse_plan,
};
use super::types::*;
use super::ModelRuntime;

const DEFAULT_TIMEOUT_SECS: u64 = 600;

pub struct OllamaRuntime {
    label: &'static str,
    model: String,
    host: String,
    client: reqwest::Client,
}

impl OllamaRuntime {
    pub fn new(label: &'static str, model: String, host: String) -> Self {
        Self {
            label,
            model,
            host,
            client: reqwest::Client::new(),
        }
    }

    async fn complete(&self, prompt: &str) -> Result<String, RuntimeError> {
        let endpoint = format!("{}/api/generate", self.host.trim_end_matches('/'));
        let request = self.client.post(endpoint).json(&serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "format": "json",
            "think": false,
            "keep_alive": "15m",
            "options": {
                "temperature": 0.2,
                "top_p": 0.9
            }
        }));
        let response = tokio::time::timeout(Duration::from_secs(timeout_secs()), request.send())
            .await
            .map_err(|_| {
                RuntimeError::Inference(format!(
                    "Ollama request timed out after {} seconds",
                    timeout_secs()
                ))
            })?
            .map_err(|error| RuntimeError::Inference(format!("calling Ollama: {error}")))?;
        let response = response.error_for_status().map_err(|error| {
            RuntimeError::Inference(format!("Ollama {} failed: {error}", self.model))
        })?;
        let payload: GenerateResponse = response.json().await.map_err(|error| {
            RuntimeError::Inference(format!("decoding Ollama response: {error}"))
        })?;
        let cleaned = payload.response.trim().to_string();
        if cleaned.is_empty() {
            return Err(RuntimeError::Inference(format!(
                "Ollama {} returned an empty response",
                self.model
            )));
        }
        Ok(cleaned)
    }

    fn is_available(&self) -> Result<(), String> {
        let output = StdCommand::new("ollama")
            .arg("show")
            .arg(&self.model)
            .env("OLLAMA_HOST", &self.host)
            .stdin(StdStdio::null())
            .stdout(StdStdio::null())
            .stderr(StdStdio::piped())
            .output()
            .map_err(|error| format!("Ollama is not installed: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}

#[async_trait]
impl ModelRuntime for OllamaRuntime {
    async fn ocr(&self, _page: PageImage) -> Result<OcrBlocks, RuntimeError> {
        Err(RuntimeError::Inference(
            "Ollama is not configured for Smilie's OCR role".into(),
        ))
    }

    async fn reason(&self, paper: PaperDocument) -> Result<LearningPlan, RuntimeError> {
        parse_plan(&self.complete(&build_reason_prompt(&paper)).await?)
    }

    async fn generate(&self, spec: PageSpec) -> Result<GeneratedPage, RuntimeError> {
        let raw = self.complete(&build_generate_prompt(&spec)).await?;
        let blocks = parse_blocks(&raw, &spec.title)?;
        Ok(GeneratedPage {
            html: String::new(),
            blocks: Some(blocks),
            source_json: raw,
            model_id: self.model.clone(),
        })
    }

    async fn analyze_experiment(
        &self,
        paper: PaperDocument,
    ) -> Result<ExperimentAnalysis, RuntimeError> {
        parse_experiment_analysis(
            &self
                .complete(&build_experiment_analyze_prompt(&paper))
                .await?,
        )
    }

    async fn generate_experiment(
        &self,
        paper: PaperDocument,
        approach: &str,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        parse_experiment_artifacts(
            &self
                .complete(&build_experiment_generate_prompt(&paper, approach))
                .await?,
        )
    }

    async fn refine_experiment(
        &self,
        paper: PaperDocument,
        prior: &PriorAttempt,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        parse_experiment_artifacts(
            &self
                .complete(&build_experiment_refine_prompt(&paper, prior))
                .await?,
        )
    }

    async fn compare_papers(
        &self,
        paper_a: PaperDocument,
        paper_b: PaperDocument,
    ) -> Result<PageBlocks, RuntimeError> {
        parse_blocks(
            &self
                .complete(&build_compare_prompt(&paper_a, &paper_b))
                .await?,
            "Comparison",
        )
    }

    fn status(&self) -> RuntimeStatus {
        match self.is_available() {
            Ok(()) => RuntimeStatus::Ready {
                runtime: "ollama".into(),
                model: self.model.clone(),
            },
            Err(message) => RuntimeStatus::Error { message },
        }
    }

    fn label(&self) -> &'static str {
        self.label
    }
}

fn timeout_secs() -> u64 {
    std::env::var("SMILIE_OLLAMA_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
}

#[derive(Deserialize)]
struct GenerateResponse {
    #[serde(default)]
    response: String,
}

#[cfg(test)]
mod tests {
    use super::OllamaRuntime;
    use crate::runtime::{ModelRuntime, PageSpec, PageSpecAssets};

    #[tokio::test]
    #[ignore = "requires the local qwen3.6:35b-a3b Ollama model"]
    async fn live_ollama_returns_structured_json() {
        let runtime = OllamaRuntime::new(
            "test",
            "qwen3.6:35b-a3b".into(),
            "http://127.0.0.1:11434".into(),
        );
        let response = runtime
            .complete(r#"Return exactly one JSON object: {"ok":true}"#)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["ok"], true);
    }

    #[tokio::test]
    #[ignore = "requires the local qwen3.6:35b-a3b Ollama model"]
    async fn live_ollama_accepts_parallel_structured_requests() {
        let runtime = OllamaRuntime::new(
            "test",
            "qwen3.6:35b-a3b".into(),
            "http://127.0.0.1:11434".into(),
        );
        let (first, second) = tokio::join!(
            runtime.complete(r#"Return exactly one JSON object: {"batch":1}"#),
            runtime.complete(r#"Return exactly one JSON object: {"batch":2}"#),
        );
        let first: serde_json::Value = serde_json::from_str(&first.unwrap()).unwrap();
        let second: serde_json::Value = serde_json::from_str(&second.unwrap()).unwrap();
        assert_eq!(first["batch"], 1);
        assert_eq!(second["batch"], 2);
    }

    #[tokio::test]
    #[ignore = "requires the local qwen3.6:35b-a3b Ollama model"]
    async fn live_ollama_generates_a_structured_learning_page() {
        let runtime = OllamaRuntime::new(
            "test",
            "qwen3.6:35b-a3b".into(),
            "http://127.0.0.1:11434".into(),
        );
        let page = runtime
            .generate(PageSpec {
                title: "Why field of view matters".into(),
                page_type: "concept".into(),
                source_sections: vec!["Introduction".into()],
                source_pages: vec![1],
                source_content: "Humans see more than the robot camera, so requests can point outside the robot field of view.".into(),
                assets: PageSpecAssets::default(),
                level: Some("default".into()),
            })
            .await
            .unwrap();
        assert!(!page.blocks.unwrap().blocks.is_empty());
    }
}
