//! llama.cpp-backed runtime for Windows/Linux desktop + mobile targets.
//!
//! Default build ships only the path-resolution + status surface; the real
//! inference call is gated behind the `llama-real` cargo feature so the
//! default macOS dev cycle does not pay the multi-minute llama.cpp cmake
//! compile. Turn it on with:
//!
//! ```text
//! cargo build --features llama-real
//! ```
//!
//! Without the feature the runtime still surfaces `MissingWeights`
//! correctly so the UI can guide the user. Vision OCR (`mmproj`) is not
//! wired here yet — multimodal pages should run on the macOS mlx-vlm
//! sidecar in v1.

use std::path::PathBuf;

use async_trait::async_trait;

use super::types::*;
use super::ModelRuntime;

#[cfg(feature = "llama-real")]
mod backend;

pub struct LlamaCppLocal {
    #[allow(dead_code)] // Surfaced via ModelRuntime::label once Settings reads it.
    pub label: &'static str,
    pub model_path: PathBuf,
    pub mmproj_path: Option<PathBuf>,
    #[cfg(feature = "llama-real")]
    backend: tokio::sync::OnceCell<std::sync::Arc<backend::LlamaBackend>>,
}

impl LlamaCppLocal {
    pub fn new(label: &'static str, model_path: PathBuf, mmproj_path: Option<PathBuf>) -> Self {
        Self {
            label,
            model_path,
            mmproj_path,
            #[cfg(feature = "llama-real")]
            backend: tokio::sync::OnceCell::new(),
        }
    }

    fn missing(&self) -> Option<RuntimeError> {
        if !self.model_path.exists() {
            return Some(RuntimeError::MissingWeights {
                path: self.model_path.display().to_string(),
            });
        }
        if let Some(m) = &self.mmproj_path {
            if !m.exists() {
                return Some(RuntimeError::MissingWeights {
                    path: m.display().to_string(),
                });
            }
        }
        None
    }

    #[cfg(feature = "llama-real")]
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, RuntimeError> {
        if let Some(e) = self.missing() {
            return Err(e);
        }
        let model_path = self.model_path.clone();
        let backend = self
            .backend
            .get_or_try_init(|| async move {
                backend::LlamaBackend::load(&model_path)
                    .map_err(|e| RuntimeError::Inference(e.to_string()))
            })
            .await?
            .clone();

        let prompt = prompt.to_string();
        tokio::task::spawn_blocking(move || backend.complete(&prompt, max_tokens))
            .await
            .map_err(|e| RuntimeError::Inference(e.to_string()))?
            .map_err(|e| RuntimeError::Inference(e.to_string()))
    }

    #[cfg(not(feature = "llama-real"))]
    async fn complete(&self, _prompt: &str, _max_tokens: u32) -> Result<String, RuntimeError> {
        if let Some(e) = self.missing() {
            return Err(e);
        }
        Err(RuntimeError::Inference(
            "llama.cpp runtime not compiled in: rebuild with --features llama-real".into(),
        ))
    }
}

#[async_trait]
impl ModelRuntime for LlamaCppLocal {
    async fn ocr(&self, _page: PageImage) -> Result<OcrBlocks, RuntimeError> {
        if let Some(e) = self.missing() {
            return Err(e);
        }
        // Vision via mmproj is not wired in v1 for the llama.cpp path; on
        // macOS the mlx-vlm sidecar handles OCR. Cross-platform vision
        // arrives in a follow-up.
        Err(RuntimeError::Inference(
            "vision OCR via llama.cpp (mmproj) not yet wired — use the macOS mlx-vlm runtime for OCR in v1".into(),
        ))
    }

    async fn reason(&self, paper: PaperDocument) -> Result<LearningPlan, RuntimeError> {
        let prompt = build_reason_prompt(&paper);
        let raw = self.complete(&prompt, 4096).await?;
        parse_plan(&raw)
    }

    async fn generate(&self, spec: PageSpec) -> Result<GeneratedPage, RuntimeError> {
        let prompt = build_generate_prompt(&spec);
        let raw = self.complete(&prompt, 6144).await?;
        let blocks = parse_blocks(&raw, &spec.title)?;
        Ok(GeneratedPage {
            html: String::new(),
            blocks: Some(blocks),
            source_json: raw,
            model_id: self
                .model_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
        })
    }

    async fn analyze_experiment(
        &self,
        paper: PaperDocument,
    ) -> Result<ExperimentAnalysis, RuntimeError> {
        let prompt = build_experiment_analyze_prompt(&paper);
        let raw = self.complete(&prompt, 1024).await?;
        parse_experiment_analysis(&raw)
    }

    async fn generate_experiment(
        &self,
        paper: PaperDocument,
        approach: &str,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        let prompt = build_experiment_generate_prompt(&paper, approach);
        let raw = self.complete(&prompt, 6144).await?;
        parse_experiment_artifacts(&raw)
    }

    async fn refine_experiment(
        &self,
        paper: PaperDocument,
        prior: &PriorAttempt,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        let prompt = build_experiment_refine_prompt(&paper, prior);
        let raw = self.complete(&prompt, 6144).await?;
        parse_experiment_artifacts(&raw)
    }

    async fn compare_papers(
        &self,
        paper_a: PaperDocument,
        paper_b: PaperDocument,
    ) -> Result<PageBlocks, RuntimeError> {
        let prompt = build_compare_prompt(&paper_a, &paper_b);
        let raw = self.complete(&prompt, 6144).await?;
        parse_blocks(&raw, "Comparison")
    }

    fn status(&self) -> RuntimeStatus {
        if let Some(RuntimeError::MissingWeights { path }) = self.missing() {
            return RuntimeStatus::MissingWeights {
                runtime: "llama.cpp".into(),
                expected_path: path,
            };
        }
        RuntimeStatus::Ready {
            runtime: "llama.cpp".into(),
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

fn build_reason_prompt(paper: &PaperDocument) -> String {
    let serialized = serde_json::to_string(paper).unwrap_or_default();
    let trimmed: String = serialized.chars().take(120_000).collect();
    format!(
        "You are a research tutor. Given this scientific paper JSON, produce a learning plan with 6-10 pages. Each page has: title, type (one of: informational, concept, formula, figure, table, quiz, simulation), source_sections, source_pages, source_content. Return JSON: {{\"summary\": \"...\", \"pages\": [...]}}. Output only JSON.\n\nPAPER JSON:\n{trimmed}"
    )
}

fn build_generate_prompt(spec: &PageSpec) -> String {
    let serialized = serde_json::to_string(spec).unwrap_or_default();
    format!(
        r#"You are a learning-page author for the Smilie app. Output ONE JSON object matching the schema and NOTHING else. No prose. No markdown fences.

SCHEMA: {{ "title": "...", "blocks": [<Block>...] }}
Each Block has a `type` discriminator. Allowed shapes:
  {{ "type": "heading", "level": 2|3, "text": "..." }}
  {{ "type": "paragraph", "text": "..." }}
  {{ "type": "callout", "kind": "info"|"warn"|"key"|"note", "title": "...", "body": "..." }}
  {{ "type": "key_term", "term": "...", "definition": "..." }}
  {{ "type": "formula", "latex": "...", "caption": "..." }}
  {{ "type": "list", "items": ["..."], "ordered": false }}
  {{ "type": "quiz", "questions": [{{ "q": "...", "choices": ["..."], "answer": 0, "explanation": "..." }}] }}
  {{ "type": "diagram_flow", "title": "...", "nodes": [{{ "id":"a", "label":"...", "note":"..." }}], "edges": [{{ "from":"a","to":"b","label":"..." }}] }}
  {{ "type": "source_ref", "section_ids": ["s1"], "page_numbers": [3] }}
  {{ "type": "code", "lang":"python", "code":"..." }}

RULES: 4-10 blocks. Plain text only inside fields (no HTML, no markdown). End with a `source_ref`. Start your response with `{{`.

PAGE SPEC:
{serialized}"#
    )
}

fn parse_plan(raw: &str) -> Result<LearningPlan, RuntimeError> {
    let start = raw
        .find('{')
        .ok_or_else(|| RuntimeError::Inference("model did not return JSON".into()))?;
    let end = raw
        .rfind('}')
        .ok_or_else(|| RuntimeError::Inference("model did not return JSON".into()))?;
    serde_json::from_str(&raw[start..=end])
        .map_err(|e| RuntimeError::Inference(format!("parsing plan: {e}")))
}

fn build_experiment_analyze_prompt(paper: &PaperDocument) -> String {
    let serialized = serde_json::to_string(paper).unwrap_or_default();
    let trimmed: String = serialized.chars().take(120_000).collect();
    format!(
        "You are a senior research engineer evaluating reproducibility. Given the paper JSON below, decide whether a self-contained Python (numpy + scipy) experiment under 200 lines could reproduce the central claim. Score 0-100 (50 = borderline plausible). Respond with EXACTLY one JSON object and nothing else: {{\"plausible\": true|false, \"score\": <int 0..100>, \"reasoning\": \"...\", \"approach_summary\": \"...\", \"suggested_title\": \"...\"}}.\n\nPAPER JSON:\n{trimmed}"
    )
}

fn build_experiment_generate_prompt(paper: &PaperDocument, approach: &str) -> String {
    let serialized = serde_json::to_string(paper).unwrap_or_default();
    let trimmed: String = serialized.chars().take(80_000).collect();
    format!(
        r#"You are writing a runnable Python experiment for the Smilie app. Output ONE JSON object and NOTHING else: {{"python_code": "...", "python_tests": "...", "inputs_schema": [...], "outputs_schema": [...]}}.

Constraints on `python_code`:
  • Self-contained module. Allowed imports: numpy, scipy, math, statistics, random, json. (micropip is fine if needed.)
  • Read inputs from a global `INPUTS: dict` and write results into a global `OUTPUTS: dict`. Do NOT call sys.exit, never read files.
  • Keep under 200 lines.

Constraints on `python_tests`:
  • Plain `assert` statements (NO `import pytest`, NO `def test_...:` wrappers required, the runner uses bare assertions).
  • Tests run AFTER `python_code` executes; reference `OUTPUTS` directly.

Schemas match this TypeScript shape:
  InputSpec  = {{name, kind:"number"|"int"|"choice"|"text"|"vector", default, label, ...}}
  OutputSpec = {{name, kind:"number"|"text"|"table"|"series", label, ...}}

APPROACH:
{approach}

PAPER JSON:
{trimmed}"#
    )
}

fn build_compare_prompt(paper_a: &PaperDocument, paper_b: &PaperDocument) -> String {
    let a = serde_json::to_string(paper_a).unwrap_or_default();
    let b = serde_json::to_string(paper_b).unwrap_or_default();
    let a_trim: String = a.chars().take(60_000).collect();
    let b_trim: String = b.chars().take(60_000).collect();
    format!(
        r#"You are a research tutor diffing two scientific papers. Output ONE JSON object matching the PageBlocks schema and NOTHING else. No prose, no markdown fences. Start with `{{`.

PageBlocks: {{ "title": "...", "blocks": [<Block>...] }}
Use these block shapes (same as a learning page):
  heading, paragraph, callout, key_term, formula, list, quiz, diagram_flow, source_ref, code

Cover, in order:
  • A heading with the comparison title.
  • A paragraph summarising the two papers' shared topic.
  • Headed sections for: Shared concepts, Contradictions, Unique to paper A, Unique to paper B.
  • If both papers describe a similar pipeline, include ONE `diagram_flow` with nodes that show overlapping steps.
  • End with a `source_ref` block.

Plain text only inside fields. ASCII math symbols inside paragraphs.

PAPER A:
{a_trim}

PAPER B:
{b_trim}"#
    )
}

fn build_experiment_refine_prompt(paper: &PaperDocument, prior: &PriorAttempt) -> String {
    let serialized = serde_json::to_string(paper).unwrap_or_default();
    let trimmed: String = serialized.chars().take(60_000).collect();
    let user_directive = match &prior.user_refine_prompt {
        Some(p) => format!("USER REFINE PROMPT (apply this verbatim, prefer code changes over test changes):\n{p}\n\n"),
        None => "Fix the code so the tests pass. DO NOT modify the tests.\n\n".to_string(),
    };
    format!(
        r#"You are repairing a failed Python experiment for the Smilie app. Output ONE JSON object and NOTHING else with the SAME shape as before: {{"python_code": "...", "python_tests": "...", "inputs_schema": [...], "outputs_schema": [...]}}.

{user_directive}PRIOR CODE:
{}

PRIOR TESTS:
{}

ERROR / OUTPUT:
{}

PAPER JSON:
{trimmed}"#,
        prior.code, prior.tests, prior.error_output
    )
}

fn parse_experiment_analysis(raw: &str) -> Result<ExperimentAnalysis, RuntimeError> {
    let start = raw
        .find('{')
        .ok_or_else(|| RuntimeError::Inference("analyze did not return JSON".into()))?;
    let end = raw
        .rfind('}')
        .ok_or_else(|| RuntimeError::Inference("analyze did not return JSON".into()))?;
    serde_json::from_str(&raw[start..=end])
        .map_err(|e| RuntimeError::Inference(format!("parsing analysis: {e}")))
}

fn parse_experiment_artifacts(raw: &str) -> Result<ExperimentArtifacts, RuntimeError> {
    let start = raw
        .find('{')
        .ok_or_else(|| RuntimeError::Inference("experiment gen did not return JSON".into()))?;
    let end = raw
        .rfind('}')
        .ok_or_else(|| RuntimeError::Inference("experiment gen did not return JSON".into()))?;
    serde_json::from_str(&raw[start..=end])
        .map_err(|e| RuntimeError::Inference(format!("parsing artifacts: {e}")))
}

fn parse_blocks(raw: &str, fallback_title: &str) -> Result<PageBlocks, RuntimeError> {
    // Same brace-slice tolerance as parse_plan; the prompt insists on
    // pure JSON so this rarely matters, but Qwen sometimes prefixes a
    // stray "Here is the page:".
    let start = raw
        .find('{')
        .ok_or_else(|| RuntimeError::Inference("generator did not return JSON".into()))?;
    let end = raw
        .rfind('}')
        .ok_or_else(|| RuntimeError::Inference("generator did not return JSON".into()))?;
    let mut blocks: PageBlocks = serde_json::from_str(&raw[start..=end])
        .map_err(|e| RuntimeError::Inference(format!("parsing blocks: {e}")))?;
    if blocks.title.trim().is_empty() {
        blocks.title = fallback_title.to_string();
    }
    Ok(blocks)
}
