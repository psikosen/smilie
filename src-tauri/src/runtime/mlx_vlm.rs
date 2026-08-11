//! macOS runtime: talks to a Python sidecar that wraps Blaizzy/mlx-vlm.
//!
//! Protocol: newline-delimited JSON over the sidecar's stdin/stdout.
//! Request:  `{"id": "<uuid>", "method": "ocr|reason|generate", "params": {...}}`
//! Response: `{"id": "<uuid>", "ok": true, "result": {...}}`
//!           or `{"id": "<uuid>", "ok": false, "error": "..."}`
//!
//! The sidecar does its own weight-presence check and reports
//! `MissingWeights` back in `error` with a prefix of
//! `"missing_weights:<absolute path>"`, which we surface as
//! `RuntimeError::MissingWeights`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

use super::types::*;
use super::ModelRuntime;

/// Default idle window before the sidecar gets reaped. Override with
/// `SMILIE_SIDECAR_IDLE_SECS=N`. Set to `0` to disable shutdown
/// entirely.
const DEFAULT_IDLE_SECS: u64 = 300;
/// How often the janitor wakes to check idleness. Cheap; small enough
/// that the kill is responsive but big enough not to thrash.
const JANITOR_TICK_SECS: u64 = 30;
/// Default per-RPC timeout for sidecar method calls. Override with
/// `SMILIE_SIDECAR_TIMEOUT_SECS=N`. Set to `0` to wait forever (useful
/// for very long generation runs).
const DEFAULT_RPC_TIMEOUT_SECS: u64 = 180;

#[derive(Clone)]
pub struct MlxVlmSidecar {
    inner: Arc<Mutex<SidecarState>>,
    #[allow(dead_code)] // Returned via ModelRuntime::label; allow until wired.
    role_label: &'static str,
    expected_path: PathBuf,
}

struct SidecarState {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    next_id: u64,
    waiters: std::collections::HashMap<String, oneshot::Sender<RpcResponse>>,
    #[allow(dead_code)] // Settings UI will surface this once wired.
    status: RuntimeStatus,
    /// Updated whenever a new RPC is dispatched OR a response lands.
    /// Used by the janitor task to decide when the model can be reaped.
    last_active: Instant,
    /// Whether a janitor task is already polling for this sidecar. We
    /// only ever want one (otherwise spawn-respawn ratchets).
    janitor_running: bool,
}

#[derive(Serialize)]
struct RpcRequest<'a> {
    id: String,
    method: &'a str,
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    #[serde(default)]
    id: String,
    ok: bool,
    #[serde(default)]
    result: serde_json::Value,
    #[serde(default)]
    error: Option<String>,
}

impl MlxVlmSidecar {
    /// Construct a sidecar handle. Does *not* spawn; first call spawns lazily
    /// so we don't pay python startup unless the runtime is actually used.
    pub fn new(role_label: &'static str, expected_path: PathBuf) -> Self {
        let status = if expected_path.exists() {
            RuntimeStatus::Ready {
                runtime: "mlx-vlm".into(),
                model: expected_path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "<unknown>".into()),
            }
        } else {
            RuntimeStatus::MissingWeights {
                runtime: "mlx-vlm".into(),
                expected_path: expected_path.display().to_string(),
            }
        };

        Self {
            inner: Arc::new(Mutex::new(SidecarState {
                child: None,
                stdin: None,
                next_id: 0,
                waiters: Default::default(),
                status,
                last_active: Instant::now(),
                janitor_running: false,
            })),
            role_label,
            expected_path,
        }
    }

    async fn ensure_spawned(&self) -> Result<(), RuntimeError> {
        let mut guard = self.inner.lock().await;
        if guard.child.is_some() {
            return Ok(());
        }

        if !self.expected_path.exists() {
            return Err(RuntimeError::MissingWeights {
                path: self.expected_path.display().to_string(),
            });
        }

        let sidecar_script = sidecar_script_path()?;
        // `sidecar_script` is the directory containing `mlx_sidecar/`. Use
        // it AS the cwd (not its parent) so `python -m mlx_sidecar.server`
        // can find the module on the default `sys.path`. Also set
        // PYTHONPATH explicitly so the import works even if a future shell
        // rewrites cwd between fork and exec.
        let mut cmd = Command::new(python_exe());
        cmd.arg("-u")
            .arg("-m")
            .arg("mlx_sidecar.server")
            .arg("--model-path")
            .arg(&self.expected_path)
            .current_dir(&sidecar_script)
            .env("PYTHONPATH", &sidecar_script)
            // Tauri watches src-tauri during development. Python bytecode
            // written beside the sidecar used to trigger a mid-pipeline app
            // rebuild, terminating the in-flight request.
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            RuntimeError::Io(std::io::Error::new(
                e.kind(),
                format!("spawning mlx-vlm sidecar: {e}"),
            ))
        })?;

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let parsed: Result<RpcResponse, _> = serde_json::from_str(&line);
                match parsed {
                    Ok(resp) => {
                        let mut st = inner.lock().await;
                        st.last_active = Instant::now();
                        if let Some(tx) = st.waiters.remove(&resp.id) {
                            let _ = tx.send(resp);
                        }
                    }
                    Err(_) => tracing::debug!(target: "mlx_sidecar", "stdout: {line}"),
                }
            }
        });

        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::info!(target: "mlx_sidecar", "{line}");
            }
        });

        guard.child = Some(child);
        guard.stdin = Some(stdin);
        guard.last_active = Instant::now();

        // Start the idle janitor exactly once per sidecar lifetime.
        // It exits when it observes the child is gone, so a future
        // spawn just starts a fresh one.
        if !guard.janitor_running {
            guard.janitor_running = true;
            spawn_janitor(Arc::clone(&self.inner));
        }

        Ok(())
    }

    async fn rpc(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, RuntimeError> {
        self.ensure_spawned().await?;

        let (tx, rx) = oneshot::channel();
        let id = {
            let mut guard = self.inner.lock().await;
            guard.next_id = guard.next_id.wrapping_add(1);
            guard.last_active = Instant::now();
            let id = format!("r{}", guard.next_id);
            guard.waiters.insert(id.clone(), tx);
            let req = RpcRequest {
                id: id.clone(),
                method,
                params,
            };
            let line =
                serde_json::to_string(&req).map_err(|e| RuntimeError::Inference(e.to_string()))?;
            if let Some(stdin) = guard.stdin.as_mut() {
                stdin.write_all(line.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                stdin.flush().await?;
            } else {
                return Err(RuntimeError::Unavailable {
                    runtime: "mlx-vlm".into(),
                });
            }
            id
        };

        let timeout_secs = rpc_timeout_secs();
        let resp = if timeout_secs == 0 {
            rx.await
                .map_err(|_| RuntimeError::Inference("sidecar dropped before responding".into()))?
        } else {
            match tokio::time::timeout(Duration::from_secs(timeout_secs), rx).await {
                Ok(Ok(resp)) => resp,
                Ok(Err(_)) => {
                    return Err(RuntimeError::Inference(
                        "sidecar dropped before responding".into(),
                    ));
                }
                Err(_) => {
                    // Stop waiting; clear the waiter so a late response
                    // from the sidecar is dropped silently by the stdout
                    // reader. Do NOT kill the sidecar — python may still
                    // recover. The idle janitor will reap it later.
                    let mut guard = self.inner.lock().await;
                    guard.waiters.remove(&id);
                    return Err(RuntimeError::Inference(format!(
                        "sidecar RPC timed out after {timeout_secs}s"
                    )));
                }
            }
        };

        if resp.ok {
            Ok(resp.result)
        } else {
            let err = resp.error.unwrap_or_else(|| "unknown".into());
            if let Some(rest) = err.strip_prefix("missing_weights:") {
                return Err(RuntimeError::MissingWeights {
                    path: rest.trim().to_string(),
                });
            }
            Err(RuntimeError::Inference(format!("[{id}] {err}")))
        }
    }
}

#[async_trait]
impl ModelRuntime for MlxVlmSidecar {
    async fn ocr(&self, page: PageImage) -> Result<OcrBlocks, RuntimeError> {
        let v = self
            .rpc("ocr", serde_json::to_value(&page).unwrap())
            .await?;
        serde_json::from_value(v).map_err(|e| RuntimeError::Inference(e.to_string()))
    }

    async fn reason(&self, paper: PaperDocument) -> Result<LearningPlan, RuntimeError> {
        let v = self
            .rpc("reason", serde_json::to_value(&paper).unwrap())
            .await?;
        let mut plan: LearningPlan =
            serde_json::from_value(v).map_err(|e| RuntimeError::Inference(e.to_string()))?;
        enrich_learning_plan_sources(&mut plan, &paper);
        Ok(plan)
    }

    async fn generate(&self, spec: PageSpec) -> Result<GeneratedPage, RuntimeError> {
        let v = self
            .rpc("generate", serde_json::to_value(&spec).unwrap())
            .await?;
        serde_json::from_value(v).map_err(|e| RuntimeError::Inference(e.to_string()))
    }

    async fn analyze_experiment(
        &self,
        paper: PaperDocument,
    ) -> Result<ExperimentAnalysis, RuntimeError> {
        let v = self
            .rpc("analyze_experiment", serde_json::to_value(&paper).unwrap())
            .await?;
        serde_json::from_value(v).map_err(|e| RuntimeError::Inference(e.to_string()))
    }

    async fn generate_experiment(
        &self,
        paper: PaperDocument,
        approach: &str,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        let params = serde_json::json!({
            "paper": paper,
            "approach": approach,
        });
        let v = self.rpc("generate_experiment", params).await?;
        serde_json::from_value(v).map_err(|e| RuntimeError::Inference(e.to_string()))
    }

    async fn refine_experiment(
        &self,
        paper: PaperDocument,
        prior: &PriorAttempt,
    ) -> Result<ExperimentArtifacts, RuntimeError> {
        let params = serde_json::json!({
            "paper": paper,
            "prior": prior,
        });
        let v = self.rpc("refine_experiment", params).await?;
        serde_json::from_value(v).map_err(|e| RuntimeError::Inference(e.to_string()))
    }

    async fn compare_papers(
        &self,
        paper_a: PaperDocument,
        paper_b: PaperDocument,
    ) -> Result<PageBlocks, RuntimeError> {
        let params = serde_json::json!({ "a": paper_a, "b": paper_b });
        let v = self.rpc("compare_papers", params).await?;
        serde_json::from_value(v).map_err(|e| RuntimeError::Inference(e.to_string()))
    }

    fn status(&self) -> RuntimeStatus {
        // Re-check weight presence on every call so the settings UI reflects
        // the user dropping files at runtime.
        if self.expected_path.exists() {
            RuntimeStatus::Ready {
                runtime: "mlx-vlm".into(),
                model: self
                    .expected_path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
            }
        } else {
            RuntimeStatus::MissingWeights {
                runtime: "mlx-vlm".into(),
                expected_path: self.expected_path.display().to_string(),
            }
        }
    }

    fn label(&self) -> &'static str {
        self.role_label
    }
}

/// Watch a sidecar's `last_active` timestamp; once it has been idle for
/// longer than the configured limit, kill the child to release the
/// model from RAM. The next RPC call will lazy-respawn it.
fn spawn_janitor(state: Arc<Mutex<SidecarState>>) {
    let idle_secs = std::env::var("SMILIE_SIDECAR_IDLE_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_IDLE_SECS);

    if idle_secs == 0 {
        // Disabled; clear the flag we set in ensure_spawned so a future
        // re-spawn isn't blocked from starting a janitor if the user
        // changes the env at runtime.
        let state2 = Arc::clone(&state);
        tokio::spawn(async move {
            let mut g = state2.lock().await;
            g.janitor_running = false;
        });
        return;
    }
    let idle_limit = Duration::from_secs(idle_secs);

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(JANITOR_TICK_SECS)).await;

            let mut guard = state.lock().await;

            // Sidecar already gone (e.g. crashed) — exit the janitor;
            // ensure_spawned will start a fresh one on the next request.
            if guard.child.is_none() {
                guard.janitor_running = false;
                return;
            }

            // Skip the kill while requests are still in flight — we
            // don't want to drop a long OCR call to claw back RAM. The
            // janitor loops, so it'll catch the idle window after the
            // last response lands.
            if !guard.waiters.is_empty() {
                continue;
            }

            if guard.last_active.elapsed() >= idle_limit {
                tracing::info!(
                    target: "mlx_sidecar",
                    "idle for {}s — reaping sidecar to free model RAM",
                    idle_limit.as_secs()
                );
                // Drop stdin first so the python loop sees EOF and can
                // exit cleanly; then kill the child to be sure. The
                // `kill_on_drop(true)` set at spawn time also fires if
                // we just drop the Child.
                guard.stdin.take();
                if let Some(mut child) = guard.child.take() {
                    let _ = child.start_kill();
                }
                guard.janitor_running = false;
                return;
            }
        }
    });
}

fn python_exe() -> PathBuf {
    if let Some(explicit) = std::env::var_os("SMILIE_PYTHON").filter(|value| !value.is_empty()) {
        return PathBuf::from(explicit);
    }

    let local_python = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .join(".venv")
        .join("bin")
        .join("python3");
    if local_python.is_file() {
        return local_python;
    }

    PathBuf::from("python3")
}

/// Resolve the per-RPC timeout in seconds. Reads `SMILIE_SIDECAR_TIMEOUT_SECS`;
/// falls back to `DEFAULT_RPC_TIMEOUT_SECS` if unset or unparseable. A value
/// of `0` means "wait forever" — callers must handle that explicitly.
fn rpc_timeout_secs() -> u64 {
    std::env::var("SMILIE_SIDECAR_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_RPC_TIMEOUT_SECS)
}

fn sidecar_script_path() -> Result<PathBuf, RuntimeError> {
    // Dev layout: src-tauri/python/mlx_sidecar/server.py
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let p = Path::new(manifest_dir).join("python");
    if p.exists() {
        return Ok(p);
    }
    Err(RuntimeError::Other(anyhow::anyhow!(
        "sidecar script dir not found at {}",
        p.display()
    )))
    .context("locating mlx sidecar")
    .map_err(RuntimeError::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirm the env-var parser accepts plausible values, rejects nonsense
    /// (falling back to the default), and treats `0` as a valid disable
    /// signal. We touch the global env, so the test is `#[ignore]`-friendly
    /// in parallel suites — but cargo serializes by default within a single
    /// process and we restore after each step.
    #[test]
    fn rpc_timeout_from_env_parses_or_defaults() {
        // SAFETY: tests in this crate run single-threaded by default; we
        // also restore/remove the var at every step so other tests in the
        // same module don't observe stale state.
        let key = "SMILIE_SIDECAR_TIMEOUT_SECS";
        let prev = std::env::var(key).ok();

        // Unset → default.
        unsafe {
            std::env::remove_var(key);
        }
        assert_eq!(rpc_timeout_secs(), DEFAULT_RPC_TIMEOUT_SECS);

        // Valid integer → parsed.
        unsafe {
            std::env::set_var(key, "42");
        }
        assert_eq!(rpc_timeout_secs(), 42);

        // Zero → zero (caller treats as "wait forever").
        unsafe {
            std::env::set_var(key, "0");
        }
        assert_eq!(rpc_timeout_secs(), 0);

        // Whitespace tolerated.
        unsafe {
            std::env::set_var(key, "  90  ");
        }
        assert_eq!(rpc_timeout_secs(), 90);

        // Garbage → fallback to default.
        unsafe {
            std::env::set_var(key, "not-a-number");
        }
        assert_eq!(rpc_timeout_secs(), DEFAULT_RPC_TIMEOUT_SECS);

        // Negative → fallback (u64 parse fails).
        unsafe {
            std::env::set_var(key, "-5");
        }
        assert_eq!(rpc_timeout_secs(), DEFAULT_RPC_TIMEOUT_SECS);

        // Restore prior state to be a polite test citizen.
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
