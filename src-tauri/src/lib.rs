use std::sync::Mutex;

use riars_core::{AudioEngine, EngineStateInfo, MediaLibrary, Playlist, Track};
use tauri::{Manager, State};

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
fn engine_next(state: State<AppState>) -> Result<(), String> {
    state.engine.lock().map_err(|e| e.to_string())?.next_track();
    Ok(())
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
fn engine_get_state(state: State<AppState>) -> Result<EngineStateInfo, String> {
    Ok(state.engine.lock().map_err(|e| e.to_string())?.state_info())
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
            set_locale,
            get_supported_languages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RIARS");
}
