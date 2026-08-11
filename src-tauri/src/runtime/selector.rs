//! Wire the right runtime per role per platform.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use super::llama_cpp_local::LlamaCppLocal;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use super::mlx_vlm::MlxVlmSidecar;
use super::types::RuntimeStatus;
use super::ModelRuntime;
use crate::app_state::AppPaths;
use crate::manifest::{ModelManifest, ModelSpec, RuntimeKind};

pub struct RuntimeSelector {
    pub ocr: Arc<dyn ModelRuntime>,
    pub reasoning: Arc<dyn ModelRuntime>,
    pub generator: Arc<dyn ModelRuntime>,
}

impl RuntimeSelector {
    pub fn build(manifest: &ModelManifest, paths: &AppPaths) -> Result<Self> {
        let platform = detect_platform();
        tracing::info!(%platform, "selecting runtimes");

        let profile = manifest
            .platform_profiles
            .get(&platform)
            .cloned()
            .unwrap_or_default();

        let ocr_spec = manifest.models.get(&profile.ocr).cloned();
        let reasoning_spec = manifest.models.get(&profile.reasoning).cloned();
        let generator_spec = manifest.models.get(&profile.generator).cloned();

        Ok(Self {
            ocr: build_runtime("ocr", ocr_spec, paths),
            reasoning: build_runtime("reasoning", reasoning_spec, paths),
            generator: build_runtime("generator", generator_spec, paths),
        })
    }

    pub fn status_report(&self) -> RuntimeStatusReport {
        RuntimeStatusReport {
            ocr: self.ocr.status(),
            reasoning: self.reasoning.status(),
            generator: self.generator.status(),
        }
    }
}

/// Resolve the canonical absolute path of the assets/models directory used
/// to satisfy manifest specs. Mirrors the lookup in `resolve_path`: prefer
/// the bundled `<app resource>/../assets/models` location, fall back to the
/// app-data `models/` dir.
///
/// Used by the Settings "Reveal models folder" command and the
/// missing-weights banner so the UI can point users at one canonical
/// location for dropping weights.
pub fn resolved_models_dir(paths: &AppPaths) -> std::path::PathBuf {
    let bundled = paths.app_data.parent().map(|d| d.join("assets/models"));
    if let Some(b) = bundled {
        if b.exists() {
            return b;
        }
    }
    paths.models_dir.clone()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeStatusReport {
    pub ocr: RuntimeStatus,
    pub reasoning: RuntimeStatus,
    pub generator: RuntimeStatus,
}

fn build_runtime(
    role: &'static str,
    spec: Option<ModelSpec>,
    paths: &AppPaths,
) -> Arc<dyn ModelRuntime> {
    let Some(spec) = spec else {
        // No model mapped for this role on this platform. Return a llama
        // runtime pointed at a nonexistent path — status() will surface
        // `MissingWeights` and the UI will prompt the user to edit the
        // manifest.
        let dummy = paths.models_dir.join(format!("{role}/<unset>.gguf"));
        return Arc::new(LlamaCppLocal::new(role, dummy, None));
    };

    let resolved = resolve_path(paths, &spec.path);
    match spec.runtime {
        RuntimeKind::MlxVlm => mlx_runtime(role, resolved),
        RuntimeKind::LlamaCpp => {
            let mmproj = spec.mmproj.as_deref().map(|m| resolve_path(paths, m));
            Arc::new(LlamaCppLocal::new(role, resolved, mmproj))
        }
        RuntimeKind::LlamaServer => Arc::new(super::llama_cpp_server::LlamaCppServer::new(
            role,
            spec.endpoint
                .unwrap_or_else(|| "http://127.0.0.1:8080".to_string()),
            resolved,
        )),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn mlx_runtime(role: &'static str, model_path: std::path::PathBuf) -> Arc<dyn ModelRuntime> {
    Arc::new(MlxVlmSidecar::new(role, model_path))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn mlx_runtime(role: &'static str, model_path: std::path::PathBuf) -> Arc<dyn ModelRuntime> {
    // mlx-vlm runs only on macOS Apple Silicon. On mobile we substitute
    // a llama.cpp runtime pointed at the same path so the manifest
    // surfaces a sensible error (MissingWeights or "switch profile to a
    // GGUF model") without panicking. This lets the same manifest file
    // ship across platforms.
    Arc::new(LlamaCppLocal::new(role, model_path, None))
}

fn resolve_path(paths: &AppPaths, relative_or_abs: &str) -> std::path::PathBuf {
    let p = Path::new(relative_or_abs);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    // Prefer app bundle assets dir; fall back to app_data/models.
    let bundled = paths.app_data.parent().map(|d| d.join(relative_or_abs));
    if let Some(b) = bundled {
        if b.exists() {
            return b;
        }
    }
    // Default: resolve under the user's assets/models dir bundled with the
    // build. We stash this as a convention: anything that begins with
    // `assets/models/` lives under `<app resource>/../assets/models/`.
    paths.models_dir.parent().map_or_else(
        || paths.app_data.join(relative_or_abs),
        |root| root.join(relative_or_abs),
    )
}

fn detect_platform() -> String {
    #[cfg(target_os = "macos")]
    {
        "macos".into()
    }
    #[cfg(target_os = "windows")]
    {
        "windows".into()
    }
    #[cfg(target_os = "linux")]
    {
        "linux".into()
    }
    #[cfg(target_os = "ios")]
    {
        "ios".into()
    }
    #[cfg(target_os = "android")]
    {
        "android".into()
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "ios",
        target_os = "android"
    )))]
    {
        "unknown".into()
    }
}
