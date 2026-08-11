//! Pipeline integration smoke test.
//!
//! Exercises the real 10-stage orchestrator end-to-end without needing
//! model weights, real PDFs, or libpdfium.
//!
//! ## Integration path chosen
//!
//! - Production code change: minimal generic-ization. `JobQueue`,
//!   `Ctx`, `run_pipeline`, and the `emit_*` helpers gained an `<R:
//!   tauri::Runtime>` parameter (with `Wry` as the default for
//!   `JobQueue`) so the test can wire a `tauri::test::MockRuntime`
//!   handle through the orchestrator. Existing call sites are
//!   unchanged because `Wry` is inferred everywhere.
//! - `Ctx::app_data()` honours a `SMILIE_APP_DATA_OVERRIDE` env var so
//!   the test can redirect the pipeline at a tempdir without touching
//!   `~/Library/Application Support/`.
//! - `lib.rs` exposes `db`, `pipeline`, and `runtime` as `pub mod` so
//!   this integration test can reach the orchestrator from outside the
//!   crate. No production behaviour change.
//!
//! ## What's mocked vs real
//!
//! - **Real**: SQLite DB + migrations, the 10-stage orchestrator, the
//!   structure assembler, the deterministic block→HTML renderer, the
//!   FTS indexer, the validation stage.
//! - **Mocked**: the three `ModelRuntime` impls (OCR, reasoning,
//!   generator) return canned payloads; `render_pages` is skipped
//!   entirely by pre-seeding the jobs table so the orchestrator
//!   resumes from `ocr_extract` (no libpdfium dependency).
//!
//! ## Test cases
//!
//! - `pipeline_runs_end_to_end_with_mock_runtime` — full happy path,
//!   asserts pages.status='ready', html_path is non-empty, the HTML
//!   contains the planned title, and `pages_fts` indexes the page.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tempfile::TempDir;

use smilie_lib::db::queries;
use smilie_lib::db::Db;
use smilie_lib::pipeline::{run_pipeline, JobQueue};
use smilie_lib::runtime::{
    Block, ExperimentAnalysis, ExperimentArtifacts, GeneratedPage, LearningPlan, ModelRuntime,
    OcrBlock, OcrBlocks, PageBlocks, PageImage, PageSpec, PaperDocument, PriorAttempt,
    RuntimeError, RuntimeSelector, RuntimeStatus,
};

const PLANNED_TITLE: &str = "Mock Page: Cosmic Bagels";

// ── Mock runtime ────────────────────────────────────────────────────

struct MockRuntime;

#[async_trait]
impl ModelRuntime for MockRuntime {
    async fn ocr(&self, page: PageImage) -> Result<OcrBlocks, RuntimeError> {
        Ok(OcrBlocks {
            blocks: vec![
                OcrBlock {
                    page_number: page.page_number,
                    block_type: "heading".into(),
                    text: Some("On the Topology of Bagels".into()),
                    bbox: None,
                    confidence: Some(0.99),
                },
                OcrBlock {
                    page_number: page.page_number,
                    block_type: "text".into(),
                    text: Some(
                        "A bagel is a torus with a hole, distinct from doughnuts in cooking method.".into(),
                    ),
                    bbox: None,
                    confidence: Some(0.95),
                },
            ],
        })
    }

    async fn reason(&self, _paper: PaperDocument) -> Result<LearningPlan, RuntimeError> {
        Ok(LearningPlan {
            summary: "Bagels are tori. Discuss.".into(),
            pages: vec![PageSpec {
                title: PLANNED_TITLE.into(),
                page_type: "concept".into(),
                source_sections: vec!["s1".into()],
                source_pages: vec![1],
                source_content: "A bagel is a torus.".into(),
                assets: Default::default(),
                level: None,
            }],
        })
    }

    async fn generate(&self, spec: PageSpec) -> Result<GeneratedPage, RuntimeError> {
        let blocks = PageBlocks {
            title: spec.title.clone(),
            blocks: vec![
                Block::Heading {
                    level: 2,
                    text: "What is a bagel?".into(),
                },
                Block::Paragraph {
                    text: "It is a torus baked then boiled.".into(),
                },
            ],
        };
        let source_json = serde_json::to_string(&blocks).unwrap();
        Ok(GeneratedPage {
            html: String::new(),
            blocks: Some(blocks),
            source_json,
            model_id: "mock-generator-v0".into(),
        })
    }

    async fn analyze_experiment(
        &self,
        _paper: PaperDocument,
    ) -> Result<ExperimentAnalysis, RuntimeError> {
        Ok(ExperimentAnalysis {
            plausible: true,
            score: 80,
            reasoning: "mocked".into(),
            approach_summary: "mocked approach".into(),
            suggested_title: "mocked title".into(),
        })
    }

    async fn generate_experiment(
        &self,
        _paper: PaperDocument,
        _approach: &str,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        Ok(ExperimentArtifacts {
            python_code: "OUTPUTS['x'] = 1".into(),
            python_tests: "assert OUTPUTS['x'] == 1".into(),
            inputs_schema: serde_json::json!([]),
            outputs_schema: serde_json::json!([]),
        })
    }

    async fn refine_experiment(
        &self,
        _paper: PaperDocument,
        _prior: &PriorAttempt,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        Ok(ExperimentArtifacts {
            python_code: "OUTPUTS['x'] = 1".into(),
            python_tests: "assert OUTPUTS['x'] == 1".into(),
            inputs_schema: serde_json::json!([]),
            outputs_schema: serde_json::json!([]),
        })
    }

    async fn compare_papers(
        &self,
        _paper_a: PaperDocument,
        _paper_b: PaperDocument,
    ) -> Result<PageBlocks, RuntimeError> {
        Ok(PageBlocks {
            title: "Mock Comparison".into(),
            blocks: vec![],
        })
    }

    fn status(&self) -> RuntimeStatus {
        RuntimeStatus::Ready {
            runtime: "mock".into(),
            model: "mock".into(),
        }
    }

    fn label(&self) -> &'static str {
        "mock"
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// 1×1 transparent PNG. The OCR stage reads the file's path (it does
/// not actually decode the image because the mocked runtime ignores
/// the bytes), but the file must exist so `read_dir` finds it.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

fn write_tiny_png(path: &PathBuf) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, TINY_PNG).unwrap();
}

// ── The actual test ─────────────────────────────────────────────────

// NOTE: this test mutates the process-global `SMILIE_APP_DATA_OVERRIDE`
// env var. It is the only test in this binary that does so, so there is
// no race today. If/when a second test that touches that env var is
// added, both must be serialized (introduce `serial_test` or move the
// override to a test-only AppHandle parameter) — `cargo test` runs
// integration tests within the same binary in parallel.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipeline_runs_end_to_end_with_mock_runtime() {
    let tmp = TempDir::new().expect("tempdir");
    let app_data = tmp.path().to_path_buf();

    // Redirect Ctx::app_data() so every stage works inside the tempdir
    // instead of the user's real Application Support.
    std::env::set_var("SMILIE_APP_DATA_OVERRIDE", &app_data);

    // Create the directories the pipeline assumes exist.
    for sub in ["papers", "page_images", "pages"] {
        std::fs::create_dir_all(app_data.join(sub)).unwrap();
    }

    // 1) Open + migrate the DB.
    let db_path = app_data.join("smilie.db");
    let db = Db::open(&db_path).await.expect("open db");
    db.run_migrations().await.expect("run migrations");

    // 2) Insert a paper row. file_path is empty + we skip render_pages
    //    so we never need the bytes on disk.
    let paper_id = queries::insert_paper(&db, "Mock Paper", "[]", "", Some("mock.pdf"))
        .await
        .expect("insert paper");

    // 3) Pre-seed the page-image dir so OCR's `read_dir` finds a page.
    let page_dir = app_data.join("page_images").join(&paper_id);
    write_tiny_png(&page_dir.join("page_0001.png"));

    // 4) Pre-seed a failed `ocr_extract` job. The orchestrator's resume
    //    logic positions to that stage and runs STAGES[2..] = OCR →
    //    structure → plan → generate → validate → save → index →
    //    complete. `import_pdf` and `render_pages` are skipped.
    let job_id = queries::insert_job(&db, &paper_id, "ocr_extract")
        .await
        .expect("insert job");
    queries::update_job(
        &db,
        &job_id,
        Some("ocr_extract"),
        Some("failed"),
        None,
        None,
    )
    .await
    .expect("mark job failed at ocr_extract");

    // 5) Build the JobQueue with a MockRuntime AppHandle. mock_app
    //    must outlive the pipeline run, so we keep it in scope.
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();

    let mock = Arc::new(MockRuntime);
    let selector = Arc::new(RuntimeSelector {
        ocr: Arc::clone(&mock) as Arc<dyn ModelRuntime>,
        reasoning: Arc::clone(&mock) as Arc<dyn ModelRuntime>,
        generator: Arc::clone(&mock) as Arc<dyn ModelRuntime>,
    });

    let queue = Arc::new(JobQueue::new(db.clone(), selector, handle));

    // 6) Drive the pipeline directly (not via `start`) so we can await
    //    completion inside the test.
    run_pipeline(&queue, &paper_id)
        .await
        .expect("pipeline ran to completion");

    // ── Assertions ─────────────────────────────────────────────

    // (a) at least one page row, status=ready, html_path non-empty.
    let pages = queries::list_pages_for_paper(&db, &paper_id)
        .await
        .expect("list pages");
    assert!(!pages.is_empty(), "expected at least one page row");
    let page = &pages[0];
    assert_eq!(page.status, "ready", "page status should be 'ready'");
    assert!(
        !page.html_path.is_empty(),
        "html_path should be set by save_pages"
    );

    // (b) the rendered HTML on disk contains the planned title.
    let html = std::fs::read_to_string(&page.html_path).expect("read page html");
    assert!(
        html.contains(PLANNED_TITLE),
        "rendered HTML should contain the planned title; got:\n{html}"
    );

    // (c) FTS index has at least one row referring to that page id.
    let fts_hits = queries::search_pages_fts(&db, "bagel")
        .await
        .expect("fts search");
    assert!(
        fts_hits.iter().any(|id| id == &page.id),
        "pages_fts should contain row for page {}; hits={fts_hits:?}",
        page.id
    );

    // Cleanup the env var so a parallel test in the same process is
    // not affected. (`tempfile::TempDir` cleans the directory on drop.)
    std::env::remove_var("SMILIE_APP_DATA_OVERRIDE");
}
