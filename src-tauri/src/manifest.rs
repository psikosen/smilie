//! Loads + persists `assets/models/manifest.json`.
//!
//! Shape mirrors docs/plans/2026-04-24-smilie-design.md §5.3.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub version: u32,
    #[serde(default)]
    pub platform_profiles: HashMap<String, PlatformProfile>,
    #[serde(default)]
    pub models: HashMap<String, ModelSpec>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlatformProfile {
    #[serde(default)]
    pub ocr: String,
    #[serde(default)]
    pub reasoning: String,
    #[serde(default)]
    pub generator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub runtime: RuntimeKind,
    pub path: String,
    #[serde(default)]
    pub mmproj: Option<String>,
    #[serde(default)]
    pub precision: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeKind {
    #[serde(rename = "mlx-vlm")]
    MlxVlm,
    #[serde(rename = "llama.cpp")]
    LlamaCpp,
    #[serde(rename = "llama-server")]
    LlamaServer,
}

impl ModelManifest {
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            let src = fs::read_to_string(path)
                .with_context(|| format!("reading manifest {}", path.display()))?;
            return serde_json::from_str(&src)
                .with_context(|| format!("parsing manifest {}", path.display()));
        }
        tracing::warn!(
            path = %path.display(),
            "model manifest not found, using defaults"
        );
        Ok(Self::default_builtin())
    }

    #[allow(dead_code)] // Used by future Settings → write-back path.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn default_builtin() -> Self {
        let mut models = HashMap::new();
        models.insert(
            "ocr_mlx".into(),
            ModelSpec {
                runtime: RuntimeKind::MlxVlm,
                path: "assets/models/ocr/glm-ocr-0.9b".into(),
                mmproj: None,
                precision: Some("bf16".into()),
                endpoint: None,
            },
        );
        models.insert(
            "reasoning_mlx".into(),
            ModelSpec {
                runtime: RuntimeKind::MlxVlm,
                path: "assets/models/reasoning/reasoning-mlx".into(),
                mmproj: None,
                precision: Some("bf16".into()),
                endpoint: None,
            },
        );
        models.insert(
            "generator_mlx".into(),
            ModelSpec {
                runtime: RuntimeKind::MlxVlm,
                path: "assets/models/generator/generator-mlx".into(),
                mmproj: None,
                precision: Some("bf16".into()),
                endpoint: None,
            },
        );
        models.insert(
            "ocr_llama".into(),
            ModelSpec {
                runtime: RuntimeKind::LlamaCpp,
                path: "assets/models/ocr/glm-ocr-0.9b.gguf".into(),
                mmproj: Some("assets/models/ocr/mmproj.gguf".into()),
                precision: None,
                endpoint: None,
            },
        );
        models.insert(
            "reasoning_llama".into(),
            ModelSpec {
                runtime: RuntimeKind::LlamaCpp,
                path: "assets/models/reasoning/reasoning.gguf".into(),
                mmproj: None,
                precision: None,
                endpoint: None,
            },
        );
        models.insert(
            "generator_llama".into(),
            ModelSpec {
                runtime: RuntimeKind::LlamaCpp,
                path: "assets/models/generator/generator.gguf".into(),
                mmproj: None,
                precision: None,
                endpoint: None,
            },
        );
        models.insert(
            "ocr_llama_mobile".into(),
            ModelSpec {
                runtime: RuntimeKind::LlamaCpp,
                path: "assets/models/ocr/glm-ocr-0.9b-mobile.gguf".into(),
                mmproj: Some("assets/models/ocr/mmproj.gguf".into()),
                precision: None,
                endpoint: None,
            },
        );
        models.insert(
            "reasoning_llama_mobile".into(),
            ModelSpec {
                runtime: RuntimeKind::LlamaCpp,
                path: "assets/models/reasoning/gemma-4-e4b.gguf".into(),
                mmproj: None,
                precision: None,
                endpoint: None,
            },
        );

        let mut profiles = HashMap::new();
        profiles.insert(
            "macos".into(),
            PlatformProfile {
                ocr: "ocr_mlx".into(),
                reasoning: "reasoning_mlx".into(),
                generator: "generator_mlx".into(),
            },
        );
        profiles.insert(
            "windows".into(),
            PlatformProfile {
                ocr: "ocr_llama".into(),
                reasoning: "reasoning_llama".into(),
                generator: "generator_llama".into(),
            },
        );
        profiles.insert(
            "linux".into(),
            PlatformProfile {
                ocr: "ocr_llama".into(),
                reasoning: "reasoning_llama".into(),
                generator: "generator_llama".into(),
            },
        );
        profiles.insert(
            "ios".into(),
            PlatformProfile {
                ocr: "ocr_llama_mobile".into(),
                reasoning: "reasoning_llama_mobile".into(),
                generator: "reasoning_llama_mobile".into(),
            },
        );
        profiles.insert(
            "android".into(),
            PlatformProfile {
                ocr: "ocr_llama_mobile".into(),
                reasoning: "reasoning_llama_mobile".into(),
                generator: "reasoning_llama_mobile".into(),
            },
        );

        Self {
            version: 1,
            platform_profiles: profiles,
            models,
        }
    }
}

/// Resolve a path from the manifest against the app root.
#[allow(dead_code)]
pub fn resolve_manifest_path(app_root: &Path, relative: &str) -> PathBuf {
    let p = Path::new(relative);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        app_root.join(relative)
    }
}
