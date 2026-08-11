//! Wire the right runtime per role per platform.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

        // Reuse one runtime when multiple roles point at the same actual
        // model. This is common for reasoning + generation and prevents two
        // copies of a 20+ GiB model from loading. Different models still own
        // independent runtimes and can make progress in parallel.
        let mut cache: HashMap<String, Arc<dyn ModelRuntime>> = HashMap::new();
        let ocr = build_cached_runtime("ocr", ocr_spec, paths, &mut cache);
        let reasoning = build_cached_runtime("reasoning", reasoning_spec, paths, &mut cache);
        let generator = build_cached_runtime("generator", generator_spec, paths, &mut cache);

        Ok(Self {
            ocr,
            reasoning,
            generator,
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
        RuntimeKind::Ollama => ollama_runtime(
            role,
            spec.path,
            spec.endpoint
                .unwrap_or_else(|| "http://127.0.0.1:11434".to_string()),
            paths,
        ),
    }
}

fn build_cached_runtime(
    role: &'static str,
    spec: Option<ModelSpec>,
    paths: &AppPaths,
    cache: &mut HashMap<String, Arc<dyn ModelRuntime>>,
) -> Arc<dyn ModelRuntime> {
    let Some(spec) = spec else {
        return build_runtime(role, None, paths);
    };
    let key = runtime_cache_key(&spec, paths);
    if let Some(runtime) = cache.get(&key) {
        tracing::info!(%role, %key, "sharing configured model runtime");
        return Arc::clone(runtime);
    }
    let runtime = build_runtime(role, Some(spec), paths);
    cache.insert(key, Arc::clone(&runtime));
    runtime
}

fn runtime_cache_key(spec: &ModelSpec, paths: &AppPaths) -> String {
    match spec.runtime {
        RuntimeKind::Ollama => format!(
            "ollama|{}|{}",
            spec.endpoint.as_deref().unwrap_or("http://127.0.0.1:11434"),
            spec.path
        ),
        RuntimeKind::MlxVlm | RuntimeKind::LlamaCpp | RuntimeKind::LlamaServer => {
            let model = canonical_or_resolved(resolve_path(paths, &spec.path));
            let projection = spec
                .mmproj
                .as_deref()
                .map(|path| canonical_or_resolved(resolve_path(paths, path)))
                .unwrap_or_default();
            let kind = match spec.runtime {
                RuntimeKind::MlxVlm => "mlx-vlm",
                RuntimeKind::LlamaCpp => "llama.cpp",
                RuntimeKind::LlamaServer => "llama-server",
                RuntimeKind::Ollama => unreachable!(),
            };
            format!(
                "{kind}|{}|{}|{}",
                spec.endpoint.as_deref().unwrap_or_default(),
                model.display(),
                projection.display()
            )
        }
    }
}

fn canonical_or_resolved(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn ollama_runtime(
    role: &'static str,
    model: String,
    endpoint: String,
    _paths: &AppPaths,
) -> Arc<dyn ModelRuntime> {
    Arc::new(super::ollama::OllamaRuntime::new(role, model, endpoint))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn ollama_runtime(
    role: &'static str,
    model: String,
    _endpoint: String,
    paths: &AppPaths,
) -> Arc<dyn ModelRuntime> {
    Arc::new(LlamaCppLocal::new(
        role,
        paths.models_dir.join(format!("{model}.gguf")),
        None,
    ))
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::manifest::PlatformProfile;

    #[test]
    fn shares_runtime_when_reasoning_and_generator_use_same_model() {
        let temp = tempfile::TempDir::new().unwrap();
        let model_path = temp.path().join("shared-model");
        std::fs::create_dir_all(&model_path).unwrap();
        let app_data = temp.path().join("app-data");
        let paths = AppPaths {
            papers_dir: app_data.join("papers"),
            page_images_dir: app_data.join("page-images"),
            pages_dir: app_data.join("pages"),
            models_dir: app_data.join("models"),
            db_path: app_data.join("smilie.db"),
            manifest_path: app_data.join("manifest.json"),
            app_data,
        };
        let model = ModelSpec {
            runtime: RuntimeKind::MlxVlm,
            path: model_path.to_string_lossy().to_string(),
            mmproj: None,
            precision: None,
            endpoint: None,
        };
        let mut models = HashMap::new();
        models.insert("shared".into(), model);
        let mut profiles = HashMap::new();
        profiles.insert(
            detect_platform(),
            PlatformProfile {
                ocr: "shared".into(),
                reasoning: "shared".into(),
                generator: "shared".into(),
            },
        );
        let selector = RuntimeSelector::build(
            &ModelManifest {
                version: 1,
                platform_profiles: profiles,
                models,
            },
            &paths,
        )
        .unwrap();

        assert!(Arc::ptr_eq(&selector.reasoning, &selector.generator));
        assert!(Arc::ptr_eq(&selector.ocr, &selector.reasoning));
    }
}
