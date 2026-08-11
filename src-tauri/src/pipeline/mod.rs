//! Pipeline orchestrator + job queue.
//!
//! One `JobQueue` owns a tokio task per in-flight paper. Stages execute
//! sequentially per job; jobs themselves run in parallel. Each stage writes
//! progress to the `jobs` row and emits events to the frontend so the UI
//! stays live without polling.

mod orchestrator;
pub mod render;
mod stages;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tauri::{AppHandle, Runtime, Wry};
use tokio::sync::{Mutex, Notify};

use crate::db::Db;
use crate::runtime::RuntimeSelector;

pub use orchestrator::run_pipeline;

// `JobQueue` is generic over the Tauri runtime (defaulting to `Wry`) so
// the integration smoke test can construct one wired to a `MockRuntime`
// AppHandle. Production call sites use `JobQueue` (= `JobQueue<Wry>`)
// unchanged.
pub struct JobQueue<R: Runtime = Wry> {
    pub db: Db,
    pub runtime: Arc<RuntimeSelector>,
    pub handle: AppHandle<R>,
    /// Paused jobs — resumed by a call to `resume_job`.
    inner: Mutex<Inner>,
}

struct Inner {
    pause_gates: HashMap<String, Arc<PauseGate>>,
    /// Per-job cancel flags. The orchestrator polls these between
    /// stages; setting one to true makes run_pipeline bail out at the
    /// next checkpoint. Stored on the queue (not in run_pipeline's
    /// scope) so cancel_job can reach in from a different task.
    cancels: HashMap<String, Arc<AtomicBool>>,
}

struct PauseGate {
    paused: AtomicBool,
    notify: Notify,
}

impl PauseGate {
    fn paused() -> Self {
        Self {
            paused: AtomicBool::new(true),
            notify: Notify::new(),
        }
    }

    fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        while self.paused.load(Ordering::SeqCst) {
            let notified = self.notify.notified();
            if !self.paused.load(Ordering::SeqCst) {
                break;
            }
            notified.await;
        }
    }
}

impl<R: Runtime> JobQueue<R> {
    /// Get-or-insert the cancel flag for a job. Used by both the
    /// orchestrator (to register and poll) and by cancel_job (to
    /// signal). Always returns the same Arc per job_id.
    pub(crate) async fn cancel_flag(&self, job_id: &str) -> Arc<AtomicBool> {
        let mut inner = self.inner.lock().await;
        Arc::clone(
            inner
                .cancels
                .entry(job_id.to_string())
                .or_insert_with(|| Arc::new(AtomicBool::new(false))),
        )
    }

    /// Drop transient controls when a pipeline ends so a later retry
    /// on the same job id always starts from a clean state.
    pub(crate) async fn clear_job_controls(&self, job_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.cancels.remove(job_id);
        inner.pause_gates.remove(job_id);
    }

    /// Block between stages while a job is paused. Long-running stages
    /// finish their current checkpoint before honoring the pause.
    pub(crate) async fn wait_if_paused(&self, job_id: &str) {
        let gate = {
            let inner = self.inner.lock().await;
            inner.pause_gates.get(job_id).cloned()
        };
        if let Some(gate) = gate {
            gate.wait().await;
        }
    }
}

impl<R: Runtime> JobQueue<R> {
    pub fn new(db: Db, runtime: Arc<RuntimeSelector>, handle: AppHandle<R>) -> Self {
        Self {
            db,
            runtime,
            handle,
            inner: Mutex::new(Inner {
                pause_gates: HashMap::new(),
                cancels: HashMap::new(),
            }),
        }
    }

    /// Kick off a fresh pipeline run for a paper.
    pub async fn start(self: &Arc<Self>, paper_id: &str) -> Result<()> {
        let me = Arc::clone(self);
        let paper_id = paper_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = run_pipeline(&me, &paper_id).await {
                tracing::error!(error = ?e, %paper_id, "pipeline failed");
            }
        });
        Ok(())
    }

    /// On startup, find any jobs in a non-terminal status and resume them.
    ///
    /// Jobs left in `running` from a prior process are crashed — there's
    /// no live coroutine to resume. Mark them `failed` first so the
    /// orchestrator's resume-from-last-stage logic kicks in (it only
    /// fires for `failed`/`paused`). Without this we'd re-run the
    /// pipeline from stage 0 and waste the OCR + render work the
    /// previous run already completed.
    pub async fn resume_on_startup(self: &Arc<Self>) -> Result<()> {
        let rows = crate::db::queries::list_resumable_jobs(&self.db).await?;
        if rows.is_empty() {
            return Ok(());
        }
        tracing::info!(count = rows.len(), "resuming N interrupted jobs");
        for row in &rows {
            if row.status == "running" || row.status == "pending" {
                crate::db::queries::update_job(
                    &self.db,
                    &row.id,
                    None,
                    Some("failed"),
                    None,
                    Some("interrupted by app restart"),
                )
                .await?;
            }
        }
        for row in rows {
            self.start(&row.paper_id).await?;
        }
        Ok(())
    }

    pub async fn pause_job(&self, job_id: &str) -> Result<()> {
        let job = crate::db::queries::get_job(&self.db, job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("job not found: {job_id}"))?;
        if job.status != "running" && job.status != "pending" {
            return Err(anyhow::anyhow!(
                "cannot pause job {job_id} while status is {}",
                job.status
            ));
        }

        let mut inner = self.inner.lock().await;
        inner
            .pause_gates
            .entry(job_id.to_string())
            .or_insert_with(|| Arc::new(PauseGate::paused()));
        drop(inner);
        crate::db::queries::update_job(&self.db, job_id, None, Some("paused"), None, None).await?;
        Ok(())
    }

    pub async fn resume_job(self: &Arc<Self>, job_id: &str) -> Result<()> {
        // Look up the job's current state. Paused jobs just need
        // their gate notified; failed jobs need a fresh pipeline run
        // (the prior coroutine already exited when the stage errored).
        let job = crate::db::queries::get_job(&self.db, job_id).await?;
        let Some(job) = job else {
            return Err(anyhow::anyhow!("job not found: {job_id}"));
        };

        match job.status.as_str() {
            "failed" => {
                // Keep the failed state until the orchestrator reads it.
                // That state is what tells run_pipeline to resume from
                // the failed stage instead of starting from import_pdf.
                self.start(&job.paper_id).await?;
            }
            "paused" => {
                let gate = {
                    let mut inner = self.inner.lock().await;
                    inner.pause_gates.remove(job_id)
                };
                if let Some(gate) = gate {
                    crate::db::queries::update_job(
                        &self.db,
                        job_id,
                        None,
                        Some("running"),
                        None,
                        None,
                    )
                    .await?;
                    gate.resume();
                } else {
                    // A paused row without an in-memory gate is an
                    // interrupted process. Start a new orchestrator,
                    // which will resume from the persisted stage.
                    self.start(&job.paper_id).await?;
                }
            }
            "running" | "pending" => {}
            status => {
                return Err(anyhow::anyhow!(
                    "cannot resume job {job_id} while status is {status}"
                ));
            }
        }
        Ok(())
    }

    pub async fn cancel_job(&self, job_id: &str) -> Result<()> {
        let job = crate::db::queries::get_job(&self.db, job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("job not found: {job_id}"))?;
        if job.status == "completed" {
            return Err(anyhow::anyhow!("cannot cancel completed job {job_id}"));
        }

        // Set the cancel flag the orchestrator polls between stages
        // so a long-running run actually stops, then mark the job
        // failed in the DB so the UI updates immediately.
        let flag = self.cancel_flag(job_id).await;
        flag.store(true, Ordering::SeqCst);
        let pause_gate = {
            let mut inner = self.inner.lock().await;
            inner.pause_gates.remove(job_id)
        };
        if let Some(gate) = pause_gate {
            gate.resume();
        }
        crate::db::queries::update_job(
            &self.db,
            job_id,
            None,
            Some("failed"),
            None,
            Some("cancelled by user"),
        )
        .await?;
        crate::db::queries::update_paper_status(&self.db, &job.paper_id, "failed").await?;
        crate::events::emit_paper_updated(&self.handle, &job.paper_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::PauseGate;

    #[tokio::test]
    async fn paused_gate_blocks_until_resumed() {
        let gate = Arc::new(PauseGate::paused());
        let waiting_gate = Arc::clone(&gate);
        let waiter = tokio::spawn(async move {
            waiting_gate.wait().await;
        });

        tokio::task::yield_now().await;
        assert!(
            !waiter.is_finished(),
            "paused gate should block the pipeline"
        );

        gate.resume();
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("resume should wake the pipeline")
            .expect("wait task should complete");
    }
}
