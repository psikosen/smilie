//! Shared types for the runtime trait. Pure data; no IO.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ingest::PaperJson;
pub use crate::ingest::PaperJson as PaperDocument;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageImage {
    /// Absolute path to the rendered PDF-page image on disk (PNG).
    pub path: String,
    pub page_number: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OcrBlocks {
    pub blocks: Vec<OcrBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrBlock {
    pub page_number: u32,
    pub block_type: String, // text|heading|figure|table|equation|caption|reference
    pub text: Option<String>,
    pub bbox: Option<[f32; 4]>, // x, y, w, h
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningPlan {
    pub pages: Vec<PageSpec>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSpec {
    pub title: String,
    #[serde(rename = "type")]
    pub page_type: String,
    pub source_sections: Vec<String>,
    pub source_pages: Vec<u32>,
    pub source_content: String,
    #[serde(default)]
    pub assets: PageSpecAssets,
    /// Optional reading level the generator should target — one of
    /// `eli5|default|expert`. Threaded into the prompt so the same page
    /// blocks can be re-rendered at a different audience without losing
    /// the underlying source material. Absent (or `Some("default")`) is
    /// the existing prompt's audience.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageSpecAssets {
    #[serde(default)]
    pub figures: Vec<String>,
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default)]
    pub equations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedPage {
    /// Pre-rendered HTML, used only when `blocks` is absent (legacy
    /// free-form path). The pipeline prefers `blocks` whenever it is set
    /// and renders the final HTML deterministically on the Rust side.
    ///
    /// In practice this field is dead today — every shipping runtime
    /// impl (`mlx_vlm.rs`, `llama_cpp_local/`, `llama_cpp_server.rs`)
    /// sets `html` to `String::new()` and always populates `blocks`.
    /// The field and the `write_page_files` `None` branch are kept for
    /// wire compatibility until the legacy path is fully retired. See
    /// `WORKAROUNDS.md` ("`GeneratedPage.html` + `write_page_files`
    /// legacy fallback") for the sunset criterion.
    #[serde(default)]
    pub html: String,
    /// Structured page blocks. When present the Rust renderer turns
    /// these into the final HTML using vetted templates, eliminating the
    /// drift we used to see when the model wrote raw HTML/CSS.
    #[serde(default)]
    pub blocks: Option<PageBlocks>,
    /// Raw JSON returned by the model for debugging + auditing.
    pub source_json: String,
    pub model_id: String,
}

/// Structured page output the generator model is expected to produce.
/// Mirrors a *very* small DSL so Qwen / Gemma / mlx-vlm can hit it
/// reliably and the Rust renderer owns all visual styling.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageBlocks {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub blocks: Vec<Block>,
}

/// One unit of content. New variants must be added BOTH here and in
/// `pipeline::render` so the renderer never panics on something the
/// model emits — unknown variants fall through to a paragraph render.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Heading {
        #[serde(default = "default_heading_level")]
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    Callout {
        /// "info" | "warn" | "key" | "note" — anything else falls back
        /// to "info" in the renderer.
        #[serde(default)]
        kind: String,
        #[serde(default)]
        title: Option<String>,
        body: String,
    },
    KeyTerm {
        term: String,
        definition: String,
    },
    Formula {
        /// LaTeX source. The SDK lazy-renders with KaTeX when available;
        /// the renderer always provides a readable fallback.
        latex: String,
        #[serde(default)]
        caption: Option<String>,
    },
    List {
        items: Vec<String>,
        #[serde(default)]
        ordered: bool,
    },
    Quiz {
        questions: Vec<QuizQuestion>,
    },
    DiagramFlow {
        #[serde(default)]
        title: Option<String>,
        nodes: Vec<DiagramNode>,
        #[serde(default)]
        edges: Vec<DiagramEdge>,
    },
    SourceRef {
        #[serde(default)]
        section_ids: Vec<String>,
        #[serde(default)]
        page_numbers: Vec<u32>,
    },
    Code {
        #[serde(default)]
        lang: Option<String>,
        code: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub q: String,
    pub choices: Vec<String>,
    pub answer: usize,
    #[serde(default)]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramNode {
    pub id: String,
    pub label: String,
    /// Optional secondary line under the label (e.g. role / formula).
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
}

fn default_heading_level() -> u8 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeStatus {
    Ready {
        runtime: String,
        model: String,
    },
    MissingWeights {
        runtime: String,
        expected_path: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("model weights not found at {path}")]
    MissingWeights { path: String },
    #[error("runtime '{runtime}' is not available on this platform")]
    Unavailable { runtime: String },
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("sidecar io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Ensure we always carry the paper json type (used by the trait).
#[allow(dead_code)]
fn _assert_paper_json(_p: &PaperJson) {}

// ── Experiment runtime payloads ────────────────────────────────────
//
// Three small data containers shuttled between the runtime trait and
// the experiment pipeline. The pipeline never inspects model output
// directly — runtime impls own JSON parsing + tolerance and hand back
// validated structs.

/// Verdict from `analyze_experiment`. `score` is 0..=100; the pipeline
/// thresholds at 50 (anything below is `implausible`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentAnalysis {
    pub plausible: bool,
    pub score: u8,
    pub reasoning: String,
    pub approach_summary: String,
    pub suggested_title: String,
}

/// Output of `generate_experiment` / `refine_experiment`. The schemas
/// are kept as `serde_json::Value` because the typed UI shape lives in
/// `src/lib/ipc/index.ts` — Rust is just a passthrough here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentArtifacts {
    pub python_code: String,
    pub python_tests: String,
    pub inputs_schema: serde_json::Value,
    pub outputs_schema: serde_json::Value,
}

/// State passed back to the model when refining a prior attempt — both
/// during the inner test-fix loop (where `user_refine_prompt` is `None`)
/// and during user-driven refines (where it carries the prompt).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorAttempt {
    pub code: String,
    pub tests: String,
    pub error_output: String,
    pub user_refine_prompt: Option<String>,
}
