use std::sync::Mutex;

use riars_core::{AudioEngine, EngineStateInfo, MediaLibrary, Playlist, Track};
use std::path::Path;

use riars_core::audio::player::TrackPlayer;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
fn set_locale(lang: String) -> Result<(), String> {
    riars_core::set_language(&lang);
    Ok(())
}

#[tauri::command]
fn get_supported_languages() -> Vec<&'static str> {
    riars_core::SUPPORTED_LANGUAGES.to_vec()
}

pub struct AppState {
    pub engine: Mutex<AudioEngine>,
    pub library: Mutex<MediaLibrary>,
}

#[tauri::command]
fn engine_play(state: State<AppState>) -> Result<(), String> {
    state.engine.lock().map_err(|e| e.to_string())?.play();
    Ok(())
}

#[tauri::command]
fn engine_pause(state: State<AppState>) -> Result<(), String> {
    state.engine.lock().map_err(|e| e.to_string())?.pause();
    Ok(())
}

#[tauri::command]
fn engine_stop(state: State<AppState>) -> Result<(), String> {
    state.engine.lock().map_err(|e| e.to_string())?.stop();
    Ok(())
}

#[tauri::command]
async fn engine_next(app: AppHandle) -> Result<(), String> {
    let next = app.state::<AppState>().engine.lock().map_err(|e| e.to_string())?.next_index();
    match next {
        Some(index) => decode_and_start(app, index, false).await,
        None => {
            app.state::<AppState>().engine.lock().map_err(|e| e.to_string())?.stop();
            Ok(())
        }
    }
}

/// Decode the playlist item at `index` without holding the engine lock, then start it.
/// With `skip_failures`, unplayable items are skipped (used for auto-advance).
async fn decode_and_start(app: AppHandle, index: usize, skip_failures: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let lock = || state.engine.lock().map_err(|e| e.to_string());
        let mut index = index;
        // Separate statement: a guard in the `for` expression would live for the whole
        // loop and deadlock on the next `lock()`.
        let attempts = lock()?.playlist().len().max(1);
        for _ in 0..attempts {
            let path = lock()?.item_path(index)?;
            match TrackPlayer::from_file(Path::new(&path), 44100) {
                Ok(player) => {
                    lock()?.start_index(index, player);
                    return Ok(());
                }
                Err(e) if skip_failures => {
                    log::warn!("Skipping unplayable playlist item {path}: {e}");
                    let mut engine = lock()?;
                    engine.playlist_mut().current_index = index;
                    match engine.next_index() {
                        Some(next) => index = next,
                        None => break,
                    }
                }
                Err(e) => return Err(e),
            }
        }
        lock()?.end_advance();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn engine_set_volume(volume: f32, state: State<AppState>) -> Result<(), String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .set_volume(volume.clamp(0.0, 1.0));
    Ok(())
}

#[tauri::command]
fn engine_get_state(app: AppHandle, state: State<AppState>) -> Result<EngineStateInfo, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    // The UI polls this every 250 ms, which also drives auto-advance in the background.
    if let Some(index) = engine.poll() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = decode_and_start(app, index, true).await {
                log::error!("Auto-advance failed: {e}");
            }
        });
    }
    Ok(engine.state_info())
}

#[tauri::command]
fn engine_load_track(path: String, state: State<AppState>) -> Result<(), String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .load_track(&path)
}

#[tauri::command]
fn library_scan_folder(path: String, state: State<AppState>) -> Result<usize, String> {
    state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .scan_folder(&path)
}

#[tauri::command]
fn library_get_tracks(state: State<AppState>) -> Result<Vec<Track>, String> {
    state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .get_all_tracks()
}

#[tauri::command]
fn library_search(query: String, state: State<AppState>) -> Result<Vec<Track>, String> {
    state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .search(&query)
}

#[tauri::command]
fn playlist_create(name: String) -> Result<Playlist, String> {
    Ok(Playlist::new(name))
}

#[tauri::command]
fn playlist_get(state: State<AppState>) -> Result<Playlist, String> {
    Ok(state.engine.lock().map_err(|e| e.to_string())?.playlist().clone())
}

#[tauri::command]
fn playlist_add(
    path: String,
    title: String,
    artist: String,
    duration_ms: u64,
    state: State<AppState>,
) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().add_item(path, title, artist, duration_ms);
    Ok(engine.playlist().clone())
}

#[tauri::command]
fn playlist_remove(index: usize, state: State<AppState>) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().remove_item(index);
    Ok(engine.playlist().clone())
}

#[tauri::command]
fn playlist_move(from: usize, to: usize, state: State<AppState>) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().move_item(from, to);
    Ok(engine.playlist().clone())
}

#[tauri::command]
fn playlist_clear(state: State<AppState>) -> Result<Playlist, String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    engine.playlist_mut().clear();
    Ok(engine.playlist().clone())
}

#[tauri::command]
async fn playlist_play(index: usize, app: AppHandle) -> Result<(), String> {
    decode_and_start(app, index, false).await
}

pub fn run() {
    env_logger::init();

    let engine = AudioEngine::new().expect("Failed to initialize audio engine");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            // Keep the library in the per-user app data folder, not the working directory.
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("riars_library.db");
            let library = MediaLibrary::open(&db_path.to_string_lossy())?;
            log::info!("Media library: {}", db_path.display());

            app.manage(AppState {
                engine: Mutex::new(engine),
                library: Mutex::new(library),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_play,
            engine_pause,
            engine_stop,
            engine_next,
            engine_set_volume,
            engine_get_state,
            engine_load_track,
            library_scan_folder,
            library_get_tracks,
            library_search,
            playlist_create,
            playlist_get,
            playlist_add,
            playlist_remove,
            playlist_move,
            playlist_clear,
            playlist_play,
            set_locale,
            get_supported_languages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RIARS");
}
