//! Job-lifecycle commands.

use std::sync::Arc;

use tauri::State;

use super::CmdResult;
use crate::app_state::AppState;

#[tauri::command]
pub async fn pause_job(id: String, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    state.jobs.pause_job(&id).await?;
    Ok(())
}

#[tauri::command]
pub async fn resume_job(id: String, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    state.jobs.resume_job(&id).await?;
    Ok(())
}

#[tauri::command]
pub async fn cancel_job(id: String, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    state.jobs.cancel_job(&id).await?;
    Ok(())
}
