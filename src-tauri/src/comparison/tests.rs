//! Tests for the comparison pipeline. Lives in its own file so the
//! pipeline module stays under the CLAUDE.md §1 service cap of 300
//! lines without sacrificing test coverage of the orchestrator. The
//! `mod tests` declaration in `comparison/mod.rs` is itself gated
//! behind `#[cfg(test)]`, so this file is only compiled in test
//! builds.

use std::sync::Arc;

use async_trait::async_trait;
use tempfile::TempDir;

use super::pipeline::run_comparison_pipeline;
use crate::db::{comparison_status as status, queries, Db};
use crate::ingest::structure::PaperJson;
use crate::runtime::{
    Block, ExperimentAnalysis, ExperimentArtifacts, GeneratedPage, LearningPlan, ModelRuntime,
    OcrBlocks, PageBlocks, PageImage, PageSpec, PriorAttempt, RuntimeError, RuntimeSelector,
    RuntimeStatus,
};

/// Minimal mock that returns canned blocks from `compare_papers` and
/// errors on every other method so the test only exercises the
/// comparison-pipeline path.
struct MockRuntime {
    title: String,
}

#[async_trait]
impl ModelRuntime for MockRuntime {
    async fn ocr(&self, _page: PageImage) -> Result<OcrBlocks, RuntimeError> {
        Err(RuntimeError::Inference("unused in this test".into()))
    }
    async fn reason(&self, _paper: PaperJson) -> Result<LearningPlan, RuntimeError> {
        Err(RuntimeError::Inference("unused".into()))
    }
    async fn generate(&self, _spec: PageSpec) -> Result<GeneratedPage, RuntimeError> {
        Err(RuntimeError::Inference("unused".into()))
    }
    async fn analyze_experiment(
        &self,
        _paper: PaperJson,
    ) -> Result<ExperimentAnalysis, RuntimeError> {
        Err(RuntimeError::Inference("unused".into()))
    }
    async fn generate_experiment(
        &self,
        _paper: PaperJson,
        _approach: &str,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        Err(RuntimeError::Inference("unused".into()))
    }
    async fn refine_experiment(
        &self,
        _paper: PaperJson,
        _prior: &PriorAttempt,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        Err(RuntimeError::Inference("unused".into()))
    }
    async fn compare_papers(
        &self,
        _paper_a: PaperJson,
        _paper_b: PaperJson,
    ) -> Result<PageBlocks, RuntimeError> {
        Ok(PageBlocks {
            title: self.title.clone(),
            blocks: vec![
                Block::Heading {
                    level: 2,
                    text: "Shared concepts".into(),
                },
                Block::Paragraph {
                    text: "Both papers describe a torus.".into(),
                },
            ],
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

/// Round-trip the comparison pipeline against a mock runtime: the row
/// should land in `ready`, the rendered HTML should exist on disk under
/// the override-app-data path, and the html_path recorded on the row
/// should match.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comparison_pipeline_writes_html_and_marks_ready() {
    let tmp = TempDir::new().expect("tempdir");
    let app_data = tmp.path().to_path_buf();
    std::env::set_var("SMILIE_APP_DATA_OVERRIDE", &app_data);

    let db = Db::open(&app_data.join("smilie.db"))
        .await
        .expect("open db");
    db.run_migrations().await.expect("migrate");

    let a_id = queries::insert_paper(&db, "Paper A", "[]", "", None)
        .await
        .expect("insert a");
    let b_id = queries::insert_paper(&db, "Paper B", "[]", "", None)
        .await
        .expect("insert b");

    let comparison_id = uuid::Uuid::new_v4().to_string();
    let ts = chrono::Utc::now().timestamp();
    queries::insert_comparison(&db, &comparison_id, &a_id, &b_id, status::ANALYZING, ts)
        .await
        .expect("insert comparison");

    let mock: Arc<dyn ModelRuntime> = Arc::new(MockRuntime {
        title: "Comparison: A vs B".into(),
    });
    let selector = Arc::new(RuntimeSelector {
        ocr: Arc::clone(&mock),
        reasoning: Arc::clone(&mock),
        generator: Arc::clone(&mock),
    });

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();

    run_comparison_pipeline(
        db.clone(),
        selector,
        handle,
        comparison_id.clone(),
        a_id.clone(),
        b_id.clone(),
    )
    .await;

    let row = queries::get_comparison(&db, &comparison_id)
        .await
        .expect("get")
        .expect("row exists");
    assert_eq!(row.status, status::READY, "status should be ready");
    let path = row.html_path.expect("html_path set");
    assert!(
        std::path::Path::new(&path).exists(),
        "html file should be written: {path}"
    );
    let html = std::fs::read_to_string(&path).expect("read html");
    assert!(
        html.contains("Shared concepts"),
        "html should contain rendered heading; got {} chars",
        html.len()
    );

    std::env::remove_var("SMILIE_APP_DATA_OVERRIDE");
}
