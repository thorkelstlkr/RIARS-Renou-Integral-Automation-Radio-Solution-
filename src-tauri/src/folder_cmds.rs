//! Library folders and the folder tree of the Library panel.
//!
//! Scans run on their own thread, one at a time, and hold the library lock only
//! to store what they found (see `riars_core::library::folders::scan_into`).
//! Progress goes to the UI as `library-scan` events.

use std::sync::Mutex;

use riars_core::library::folders::{expand_dropped, list_dir, roots, scan_into, track_for_file, tracks_for_files};
use riars_core::playlist::playlist::PlaylistItem;
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

/// Most files one drop adds, as `playlist_insert` allows.
const DROP_LIMIT: usize = 10_000;

/// Payload of the `playlist-drop` event, for the drop `job` the UI numbered.
#[derive(Clone, Serialize)]
struct DropProgress {
    job: u32,
    /// "listing" while folders are walked, then "reading" with `done` of `total`
    /// files not yet in the library read.
    state: &'static str,
    done: usize,
    total: usize,
}

/// What a drop added to the playlist.
#[derive(Serialize)]
pub struct DropResult {
    added: usize,
    /// Dropped paths that were not audio or a folder.
    skipped: usize,
    /// Audio files that could not be read.
    failed: usize,
    /// More than `DROP_LIMIT` files: the rest were left out.
    truncated: bool,
}

/// Add files and folders dropped from the system's file manager (or a track dragged
/// from the library) to the playlist, before the item `before_id`, or at the end
/// when it is missing or gone. Files not in the library join it first, so they get
/// analysis, normalisation and intro/outro marks like the rest; the new ones are
/// analysed in the background. Progress arrives as `playlist-drop` events.
#[tauri::command]
pub async fn playlist_drop_files(
    paths: Vec<String>,
    before_id: Option<String>,
    job: u32,
    app: AppHandle,
) -> Result<DropResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = app.emit("playlist-drop", DropProgress { job, state: "listing", done: 0, total: 0 });
        let dropped = expand_dropped(&paths, DROP_LIMIT);
        let state = app.state::<AppState>();
        let mut last = std::time::Instant::now();
        let (tracks, failed) = tracks_for_files(&state.library, &dropped.files, &mut |done, total| {
            // A few events a second are enough for a progress line.
            if done == 0 || done == total || last.elapsed().as_millis() >= 100 {
                last = std::time::Instant::now();
                let _ = app.emit("playlist-drop", DropProgress { job, state: "reading", done, total });
            }
        })?;
        let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
        let added = tracks.len();
        let items = tracks
            .into_iter()
            .map(|t| PlaylistItem::new(t.file_path, t.title, t.artist, t.duration_ms))
            .collect();
        {
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            let playlist = engine.playlist_mut();
            let at = before_id.and_then(|id| playlist.index_of(&id)).unwrap_or(playlist.len());
            playlist.insert_items(at, items);
        }
        crate::analysis_cmds::analyze_new_tracks(&app, ids);
        Ok(DropResult { added, skipped: dropped.skipped, failed, truncated: dropped.truncated })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn library_rescan(app: AppHandle) {
    spawn_rescan(app);
}
