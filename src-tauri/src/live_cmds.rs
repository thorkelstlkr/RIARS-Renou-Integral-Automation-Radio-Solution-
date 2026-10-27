//! Tauri commands for live broadcasting: microphone, sound bank (cartwall) and play
//! modes. They mirror the `live` object in `src/live/api.ts`.

use riars_core::audio::live::{default_input_device, input_devices, MicConfig};
use riars_core::live::{CartBank, CartBankSummary, CartButton};
use riars_core::{CartPlaying, MicStatus, PlayMode};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::AppState;

/// What the On air panel polls: one engine lock, no library.
#[derive(Debug, Clone, Serialize)]
pub struct LiveStatus {
    mode: PlayMode,
    mic: MicStatus,
    carts: Vec<CartPlaying>,
}

/// Cards that can record, and the system default.
#[derive(Debug, Clone, Serialize)]
pub struct InputDevices {
    devices: Vec<String>,
    default: Option<String>,
}

/// Run `f` on a worker thread: Tauri runs plain commands on the UI thread, and opening
/// a sound card, decoding a bank sound or touching the library would freeze the window.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| e.to_string())?
}

// --- Status and play modes ---------------------------------------------------------

#[tauri::command]
pub fn live_status(state: State<AppState>) -> Result<LiveStatus, String> {
    let engine = state.engine.lock().map_err(|e| e.to_string())?;
    Ok(LiveStatus {
        mode: engine.play_mode(),
        mic: engine.mic_status(),
        carts: engine.carts_playing(),
    })
}

#[tauri::command]
pub fn live_set_mode(mode: PlayMode, state: State<AppState>) -> Result<(), String> {
    state.engine.lock().map_err(|e| e.to_string())?.set_play_mode(mode);
    Ok(())
}

// --- Microphone --------------------------------------------------------------------

#[tauri::command]
pub async fn live_mic_open(app: AppHandle) -> Result<String, String> {
    blocking(app, |state| state.engine.lock().map_err(|e| e.to_string())?.open_mic()).await
}

#[tauri::command]
pub fn live_mic_close(state: State<AppState>) -> Result<(), String> {
    state.engine.lock().map_err(|e| e.to_string())?.close_mic();
    Ok(())
}

#[tauri::command]
pub fn live_set_mic(on: bool, state: State<AppState>) -> Result<(), String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .set_mic_on(on, false)
}

#[tauri::command]
pub fn live_mic_config(state: State<AppState>) -> Result<MicConfig, String> {
    Ok(state.engine.lock().map_err(|e| e.to_string())?.mic_config().clone())
}

#[tauri::command]
pub async fn live_set_mic_config(app: AppHandle, config: MicConfig) -> Result<(), String> {
    blocking(app, move |state| {
        state
            .engine
            .lock()
            .map_err(|e| e.to_string())?
            .set_mic_config(config)
    })
    .await
}

#[tauri::command]
pub async fn live_input_devices(app: AppHandle) -> Result<InputDevices, String> {
    blocking(app, |_state| {
        Ok(InputDevices { devices: input_devices(), default: default_input_device() })
    })
    .await
}

// --- Sound bank --------------------------------------------------------------------

#[tauri::command]
pub async fn live_banks(app: AppHandle) -> Result<Vec<CartBankSummary>, String> {
    blocking(app, |state| state.library.lock().map_err(|e| e.to_string())?.cart_banks()).await
}

#[tauri::command]
pub async fn live_bank(app: AppHandle, id: i64) -> Result<Option<CartBank>, String> {
    blocking(app, move |state| state.library.lock().map_err(|e| e.to_string())?.cart_bank(id))
        .await
}

#[tauri::command]
pub async fn live_save_bank(app: AppHandle, bank: CartBank) -> Result<i64, String> {
    blocking(app, move |state| {
        state.library.lock().map_err(|e| e.to_string())?.save_cart_bank(&bank)
    })
    .await
}

#[tauri::command]
pub async fn live_delete_bank(app: AppHandle, id: i64) -> Result<(), String> {
    blocking(app, move |state| {
        state.library.lock().map_err(|e| e.to_string())?.delete_cart_bank(id)?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn live_active_bank(app: AppHandle) -> Result<Option<i64>, String> {
    blocking(app, |state| state.library.lock().map_err(|e| e.to_string())?.active_cart_bank()).await
}

#[tauri::command]
pub async fn live_set_active_bank(app: AppHandle, id: Option<i64>) -> Result<(), String> {
    blocking(app, move |state| {
        state.library.lock().map_err(|e| e.to_string())?.set_active_cart_bank(id)
    })
    .await
}

#[tauri::command]
pub async fn live_set_button(
    app: AppHandle,
    bank_id: i64,
    button: CartButton,
) -> Result<(), String> {
    blocking(app, move |state| {
        state
            .library
            .lock()
            .map_err(|e| e.to_string())?
            .set_cart_button(bank_id, &button)
    })
    .await
}

#[tauri::command]
pub async fn live_clear_button(
    app: AppHandle,
    bank_id: i64,
    page: u8,
    slot: u8,
) -> Result<(), String> {
    blocking(app, move |state| {
        state
            .library
            .lock()
            .map_err(|e| e.to_string())?
            .clear_cart_button(bank_id, page, slot)?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn live_preload(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    blocking(app, move |state| {
        riars_core::preload_carts_unlocked(&state.engine, &paths);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn live_fire(app: AppHandle, button: CartButton) -> Result<bool, String> {
    blocking(app, move |state| {
        riars_core::fire_cart_unlocked(&state.engine, &button)
    })
    .await
}

#[tauri::command]
pub fn live_stop(button: Option<u32>, state: State<AppState>) -> Result<(), String> {
    state
        .engine
        .lock()
        .map_err(|e| e.to_string())?
        .stop_carts(button);
    Ok(())
}

// --- Playlist ----------------------------------------------------------------------

#[tauri::command]
pub fn playlist_set_stop_after(
    item_id: String,
    value: bool,
    state: State<AppState>,
) -> Result<(), String> {
    let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
    let playlist = engine.playlist_mut();
    match playlist.items.iter_mut().find(|item| item.id == item_id) {
        Some(item) => {
            item.stop_after = value;
            Ok(())
        }
        None => Err(format!("No playlist item {item_id}")),
    }
}
