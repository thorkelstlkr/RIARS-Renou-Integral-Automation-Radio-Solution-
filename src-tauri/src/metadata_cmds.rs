//! Completing track metadata from MusicBrainz, AcoustID and the Cover Art Archive.
//! The lookup and its proposals live in `riars-meta`; these commands start it, report
//! it and apply what the operator ticks in the review dialog.

use std::sync::Arc;

use riars_meta::{ApplyItem, ApplyOutcome, LookupStatus};
use tauri::{AppHandle, Manager, State};

use crate::analysis_cmds::TauriLibrary;
use crate::AppState;

/// Start looking up `track_ids`, or every track with gaps when empty; returns how many.
#[tauri::command]
pub async fn metadata_lookup_start(app: AppHandle, track_ids: Vec<i64>) -> Result<usize, String> {
    let library = Arc::new(TauriLibrary(app));
    tauri::async_runtime::spawn_blocking(move || riars_meta::job().start(library, track_ids))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn metadata_lookup_status() -> LookupStatus {
    riars_meta::job().status()
}

#[tauri::command]
pub fn metadata_lookup_cancel() {
    riars_meta::job().cancel();
}

/// Apply the ticked changes: covers are downloaded and files written off the UI thread.
#[tauri::command]
pub async fn metadata_apply(app: AppHandle, items: Vec<ApplyItem>) -> Result<Vec<ApplyOutcome>, String> {
    let covers = {
        let state = app.state::<AppState>();
        riars_meta::covers_dir(state.schedule_path.parent().unwrap_or(std::path::Path::new(".")))
    };
    let library = TauriLibrary(app);
    tauri::async_runtime::spawn_blocking(move || riars_meta::apply(&library, &covers, items))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn metadata_settings_get(state: State<AppState>) -> Result<Option<String>, String> {
    let library = state.library.lock().map_err(|e| e.to_string())?;
    riars_meta::acoustid_key(&library)
}

/// Save the AcoustID key (blank turns fingerprinting off); returns what is stored.
#[tauri::command]
pub fn metadata_settings_set(acoustid_key: String, state: State<AppState>) -> Result<Option<String>, String> {
    let mut library = state.library.lock().map_err(|e| e.to_string())?;
    riars_meta::set_acoustid_key(&mut library, &acoustid_key)?;
    riars_meta::acoustid_key(&library)
}

/// Tracks with gaps, for the whole-library lookup.
#[derive(serde::Serialize)]
pub struct MissingCount {
    /// Tracks the lookup would take.
    missing: usize,
    /// Tracks with gaps it leaves out for being shorter than a minute.
    short: usize,
}

/// How many tracks a whole-library lookup would take, and how many short ones it skips.
#[tauri::command]
pub fn metadata_missing_count(state: State<AppState>) -> Result<MissingCount, String> {
    let library = state.library.lock().map_err(|e| e.to_string())?;
    let missing = library.tracks_missing_metadata(riars_meta::MIN_SONG_MS, riars_meta::MAX_TRACKS)?.len();
    let all = library.tracks_missing_metadata(0, usize::MAX)?.len();
    Ok(MissingCount { missing, short: all.saturating_sub(missing) })
}

/// Album, year and genre (and the rest) of the library tracks at these paths, for the
/// playlist's columns. Paths not in the library are left out.
#[tauri::command]
pub fn metadata_for_paths(state: State<AppState>, paths: Vec<String>) -> Result<Vec<riars_core::TrackMetadata>, String> {
    let library = state.library.lock().map_err(|e| e.to_string())?;
    library.metadata_for_paths(&paths)
}
