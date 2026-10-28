//! Library folders and the folder tree of the Library panel.
//!
//! Scans run on their own thread, one at a time, and hold the library lock only
//! to store what they found (see `riars_core::library::folders::scan_into`).
//! Progress goes to the UI as `library-scan` events.

use std::sync::Mutex;

use riars_core::library::folders::{list_dir, roots, scan_into, track_for_file};
use riars_core::{DirEntry, DirListing, LibraryFolder, Track};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;

/// Taken by the scan thread, so two scans never read the disk at once.
static SCAN_GATE: Mutex<()> = Mutex::new(());

/// Payload of the `library-scan` event.
#[derive(Clone, Serialize)]
struct ScanEvent {
    /// "scanning", "done" or "error" for one folder; "idle" when the batch ends.
    state: &'static str,
    folder: String,
    added: usize,
    error: Option<String>,
}

/// Scan one folder now, on the calling thread, waiting for any background scan first.
pub fn scan_now(app: &AppHandle, folder: &str) -> Result<usize, String> {
    let _gate = SCAN_GATE.lock().unwrap_or_else(|e| e.into_inner());
    scan_into(&app.state::<AppState>().library, folder)
}

/// Scan `folders` on a background thread, reporting each one as a `library-scan` event.
pub fn spawn_scan(app: AppHandle, folders: Vec<String>) {
    if folders.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new().name("riars-library-scan".into()).spawn(move || {
        let _gate = SCAN_GATE.lock().unwrap_or_else(|e| e.into_inner());
        let state = app.state::<AppState>();
        let mut total = 0;
        for folder in folders {
            let _ = app.emit("library-scan", ScanEvent { state: "scanning", folder: folder.clone(), added: 0, error: None });
            let event = match scan_into(&state.library, &folder) {
                Ok(added) => {
                    total += added;
                    ScanEvent { state: "done", folder, added, error: None }
                }
                Err(e) => {
                    log::warn!("Cannot scan {folder}: {e}");
                    ScanEvent { state: "error", folder, added: 0, error: Some(e) }
                }
            };
            let _ = app.emit("library-scan", event);
        }
        let _ = app.emit("library-scan", ScanEvent { state: "idle", folder: String::new(), added: total, error: None });
    });
    if let Err(e) = spawned {
        log::warn!("Cannot start the library scan: {e}");
    }
}

/// Rescan every library folder in the background (at startup and on request).
pub fn spawn_rescan(app: AppHandle) {
    let folders = app
        .state::<AppState>()
        .library
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|library| library.folders_to_scan());
    match folders {
        Ok(folders) => spawn_scan(app, folders),
        Err(e) => log::warn!("Cannot list the library folders: {e}"),
    }
}

/// Drives (Windows) or the root, home and mounted media (elsewhere).
#[tauri::command]
pub fn fs_roots() -> Vec<DirEntry> {
    roots()
}

/// One folder of the tree: its subfolders and audio files, the files marked with
/// their library track. Off the main thread: a network drive can be slow.
#[tauri::command]
pub async fn fs_list_dir(path: String, app: AppHandle) -> Result<DirListing, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut listing = list_dir(&path)?;
        app.state::<AppState>()
            .library
            .lock()
            .map_err(|e| e.to_string())?
            .fill_tracks(&mut listing)?;
        Ok(listing)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn library_folders(state: State<AppState>) -> Result<Vec<LibraryFolder>, String> {
    state.library.lock().map_err(|e| e.to_string())?.folders()
}

/// Remember `paths` as library folders and scan them in the background.
/// Returns the folders as stored; progress arrives as `library-scan` events.
/// The valid folders are added and scanned even when another path is wrong; the
/// error then names the wrong ones.
#[tauri::command]
pub fn library_add_folders(paths: Vec<String>, app: AppHandle) -> Result<Vec<String>, String> {
    let mut stored = Vec::new();
    let mut errors = Vec::new();
    {
        let state = app.state::<AppState>();
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        for path in &paths {
            match library.add_folder(path) {
                Ok(folder) => stored.push(folder),
                Err(e) => errors.push(e),
            }
        }
    }
    spawn_scan(app, stored.clone());
    if errors.is_empty() {
        Ok(stored)
    } else {
        Err(errors.join("; "))
    }
}

/// Forget a library folder; with `remove_tracks`, also remove its tracks that no
/// other library folder holds. Returns how many tracks were removed.
/// Off the main thread: removing a big folder deletes thousands of rows.
#[tauri::command]
pub async fn library_remove_folder(path: String, remove_tracks: bool, app: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .library
            .lock()
            .map_err(|e| e.to_string())?
            .remove_folder(&path, remove_tracks)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The library track of an audio file, adding the file when it is not in the
/// library yet (marks and analysis belong to library tracks).
#[tauri::command]
pub async fn library_track_for_file(path: String, app: AppHandle) -> Result<Track, String> {
    tauri::async_runtime::spawn_blocking(move || track_for_file(&app.state::<AppState>().library, &path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn library_rescan(app: AppHandle) {
    spawn_rescan(app);
}
