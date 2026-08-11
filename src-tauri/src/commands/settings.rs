//! User settings (SRS §10.2).
//!
//! Persisted as key/value rows in `settings` so we don't rev a schema every
//! time we add a new field.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use super::CmdResult;
use crate::app_state::AppState;
use crate::db::queries;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub ocr_model_path: String,
    #[serde(default)]
    pub reasoning_model_path: String,
    #[serde(default)]
    pub generator_model_path: String,
    #[serde(default = "default_runtime")]
    pub runtime_backend: String,
    #[serde(default = "default_context")]
    pub context_length: u32,
    #[serde(default = "default_temp")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub thinking_mode: bool,
    #[serde(default = "default_aggressiveness")]
    pub interactive_aggressiveness: f32,
    #[serde(default)]
    pub offline_only: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ocr_model_path: String::new(),
            reasoning_model_path: String::new(),
            generator_model_path: String::new(),
            runtime_backend: default_runtime(),
            context_length: default_context(),
            temperature: default_temp(),
            max_tokens: default_max_tokens(),
            thinking_mode: false,
            interactive_aggressiveness: default_aggressiveness(),
            offline_only: true,
        }
    }
}

fn default_runtime() -> String {
    if cfg!(target_os = "macos") {
        "mlx-vlm"
    } else {
        "llama.cpp"
    }
    .into()
}
fn default_context() -> u32 {
    16384
}
fn default_temp() -> f32 {
    0.2
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_aggressiveness() -> f32 {
    0.6
}

const KEY: &str = "app_settings";

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> CmdResult<Settings> {
    let raw = queries::get_setting(&state.db, KEY).await?;
    let out = raw
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default();
    Ok(out)
}

#[tauri::command]
pub async fn update_settings(
    patch: serde_json::Value,
    state: State<'_, Arc<AppState>>,
) -> CmdResult<()> {
    let mut current: serde_json::Value = {
        let raw = queries::get_setting(&state.db, KEY).await?;
        raw.and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::to_value(Settings::default()).unwrap())
    };

    if let (Some(obj), Some(patch_obj)) = (current.as_object_mut(), patch.as_object()) {
        for (k, v) in patch_obj {
            obj.insert(k.clone(), v.clone());
        }
    }

    queries::set_setting(&state.db, KEY, &current.to_string()).await?;
    Ok(())
}
