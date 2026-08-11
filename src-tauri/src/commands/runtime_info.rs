//! Runtime status surface for the Settings UI.

use std::sync::Arc;

use tauri::State;

use super::{CmdError, CmdResult};
use crate::app_state::AppState;
use crate::runtime::selector::{resolved_models_dir, RuntimeStatusReport};

#[tauri::command]
pub async fn list_runtime_status(
    state: State<'_, Arc<AppState>>,
) -> CmdResult<RuntimeStatusReport> {
    Ok(state.runtime.status_report())
}

/// Re-evaluate runtime status without restarting the backend. Today this is
/// equivalent to `list_runtime_status`, but the dedicated command name lets
/// the Settings UI signal user intent ("re-check after I dropped weights")
/// and gives us a hook point to broadcast changes to other windows later.
#[tauri::command]
pub async fn recheck_runtime_status(
    state: State<'_, Arc<AppState>>,
) -> CmdResult<RuntimeStatusReport> {
    let report = state.runtime.status_report();
    Ok(report)
}

/// Open the OS file manager at the resolved `assets/models/` directory and
/// return the path. On mobile (no shell plugin available) this is a no-op
/// that returns the path string so the UI can display it for the user to
/// navigate manually via the Files app.
#[tauri::command]
pub async fn reveal_models_folder(state: State<'_, Arc<AppState>>) -> CmdResult<String> {
    let path = resolved_models_dir(&state.paths);
    // Make sure the dir exists before we hand a path to the OS file
    // manager — otherwise Finder/Explorer will pop a "not found" dialog
    // on a fresh checkout that hasn't run the backend yet.
    if let Err(e) = std::fs::create_dir_all(&path) {
        return Err(CmdError(format!(
            "could not create models dir at {}: {e}",
            path.display()
        )));
    }
    let path_str = path.to_string_lossy().to_string();

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        use tauri_plugin_shell::ShellExt;
        // `Shell::open` is marked deprecated in 2.3.x in favor of the
        // tauri-plugin-opener crate, but adding new dependencies is
        // out-of-scope for this change set. Swap to the opener plugin
        // when we next bump deps. See WORKAROUNDS.md.
        #[allow(deprecated)]
        state
            .handle
            .shell()
            .open(path_str.clone(), None)
            .map_err(|e| CmdError(format!("failed to open models folder: {e}")))?;
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        // Mobile: no-op — the UI shows the returned path so users can
        // navigate manually in the Files app.
        let _ = &state;
    }

    Ok(path_str)
}
