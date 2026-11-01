//! RDS for an external encoder: Broadcast ▸ RDS. Everything is in `riars_rds`.

use riars_core::rds::RdsConfig;
use riars_rds::RdsStatus;
use serde::Serialize;
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
pub struct RdsInfo {
    config: RdsConfig,
    status: RdsStatus,
}

fn info(state: &AppState) -> RdsInfo {
    RdsInfo { config: state.rds.config(), status: state.rds.status() }
}

#[tauri::command]
pub fn rds_get(state: State<AppState>) -> RdsInfo {
    info(&state)
}

#[tauri::command]
pub fn rds_set_config(config: RdsConfig, state: State<AppState>) -> Result<RdsInfo, String> {
    state.rds.set_config(config)?;
    Ok(info(&state))
}

/// A RadioText typed by hand; `None` (or empty) goes back to the automatic texts.
#[tauri::command]
pub fn rds_set_text(text: Option<String>, minutes: Option<u32>, state: State<AppState>) -> Result<RdsInfo, String> {
    state.rds.set_text(text.as_deref(), minutes)?;
    Ok(info(&state))
}
