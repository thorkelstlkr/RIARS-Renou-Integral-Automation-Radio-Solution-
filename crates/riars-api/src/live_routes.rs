use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use riars_core::audio::live::{default_input_device, input_devices, MicConfig};
use riars_core::live::{CartBank, CartBankSummary, CartButton};
use riars_core::{AudioEngine, MicStatus, PlayMode};
use serde::Deserialize;

use crate::routes::ApiError;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/live/status", get(get_status))
        .route("/live/mode", post(set_mode))
        .route("/live/mic", post(set_mic))
        .route("/live/mic/config", get(get_mic_config).put(set_mic_config))
        .route("/live/mic/devices", get(mic_devices))
        .route("/live/banks", get(list_banks).post(save_bank))
        .route("/live/banks/{id}", get(get_bank).delete(delete_bank))
        .route("/live/banks/{id}/active", post(set_active_bank))
        .route("/live/banks/{id}/buttons", put(put_button))
        .route("/live/banks/{id}/buttons/{page}/{slot}", delete(delete_button))
        .route("/live/fire", post(fire))
        .route("/live/stop", post(stop))
        .route("/playlist/items/{id}/stop_after", put(set_stop_after))
}

#[derive(Deserialize)]
struct ModeRequest {
    mode: PlayMode,
}

#[derive(Deserialize)]
struct MicRequest {
    on: bool,
}

#[derive(Deserialize)]
struct FireRequest {
    #[serde(default)]
    bank_id: Option<i64>,
    page: u8,
    slot: u8,
}

#[derive(Deserialize)]
struct StopRequest {
    #[serde(default)]
    button: Option<u32>,
}

#[derive(Deserialize)]
struct StopAfterRequest {
    value: bool,
}

fn not_found(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, message.into())
}

fn status_json(engine: &AudioEngine) -> serde_json::Value {
    serde_json::json!({
        "mode": engine.play_mode(),
        "mic": engine.mic_status(),
        "carts": engine.carts_playing(),
    })
}

async fn get_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let engine = state.engine.lock().map_err(ApiError::internal)?;
    Ok(Json(status_json(&engine)))
}

async fn set_mode(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ModeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    engine.set_play_mode(req.mode);
    Ok(Json(status_json(&engine)))
}

async fn set_mic(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MicRequest>,
) -> Result<Json<MicStatus>, ApiError> {
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    if req.on && !engine.mic_config().allow_remote {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Opening the microphone remotely is disabled in the microphone settings".to_string(),
        ));
    }
    engine.set_mic_on(req.on, true).map_err(ApiError::from)?;
    Ok(Json(engine.mic_status()))
}

async fn get_mic_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MicConfig>, ApiError> {
    let config = state
        .engine
        .lock()
        .map_err(ApiError::internal)?
        .mic_config()
        .clone();
    Ok(Json(config))
}

async fn set_mic_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<MicConfig>,
) -> Result<Json<MicConfig>, ApiError> {
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    engine.set_mic_config(config).map_err(ApiError::from)?;
    Ok(Json(engine.mic_config().clone()))
}

async fn mic_devices(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (devices, default) =
        tokio::task::spawn_blocking(|| (input_devices(), default_input_device()))
            .await
            .map_err(ApiError::internal)?;
    Ok(Json(serde_json::json!({
        "devices": devices,
        "default": default,
    })))
}

async fn list_banks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CartBankSummary>>, ApiError> {
    let banks = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .cart_banks()?;
    Ok(Json(banks))
}

async fn save_bank(
    State(state): State<Arc<AppState>>,
    Json(bank): Json<CartBank>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        let guard = library.media_guard()?;
        for button in bank.buttons.iter().filter(|b| !b.file_path.is_empty()) {
            guard.check(&button.file_path)?;
        }
        library.save_cart_bank(&bank)?
    };
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn get_bank(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<CartBank>, ApiError> {
    let bank = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .cart_bank(id)?;
    bank.map(Json)
        .ok_or_else(|| not_found(format!("No cart bank {id}")))
}

async fn delete_bank(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .delete_cart_bank(id)?;
    if !deleted {
        return Err(not_found(format!("No cart bank {id}")));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn set_active_bank(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut library = state.library.lock().map_err(ApiError::internal)?;
    if library.cart_bank(id)?.is_none() {
        return Err(not_found(format!("No cart bank {id}")));
    }
    library.set_active_cart_bank(Some(id))?;
    Ok(Json(serde_json::json!({ "active": id })))
}

async fn put_button(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(button): Json<CartButton>,
) -> Result<&'static str, ApiError> {
    let mut library = state.library.lock().map_err(ApiError::internal)?;
    if !button.file_path.is_empty() {
        library.check_media_path(&button.file_path)?;
    }
    library
        .set_cart_button(id, &button)
        .map_err(|error| {
            if error.starts_with("No cart bank") {
                not_found(error)
            } else {
                ApiError::from(error)
            }
        })?;
    Ok("ok")
}

async fn delete_button(
    State(state): State<Arc<AppState>>,
    Path((id, page, slot)): Path<(i64, u8, u8)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .clear_cart_button(id, page, slot)?;
    if !deleted {
        return Err(not_found(format!(
            "No button at page {page} slot {slot} in bank {id}"
        )));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn fire(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FireRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let button = {
        let library = state.library.lock().map_err(ApiError::internal)?;
        let bank_id = match req.bank_id {
            Some(id) => id,
            None => library
                .active_cart_bank()?
                .ok_or_else(|| not_found("No active cart bank"))?,
        };
        let bank = library
            .cart_bank(bank_id)?
            .ok_or_else(|| not_found(format!("No cart bank {bank_id}")))?;
        bank.buttons
            .into_iter()
            .find(|button| button.page == req.page && button.slot == req.slot)
            .ok_or_else(|| {
                not_found(format!(
                    "No button at page {} slot {} in bank {bank_id}",
                    req.page, req.slot
                ))
            })?
    };
    let worker = Arc::clone(&state);
    let started = tokio::task::spawn_blocking(move || {
        riars_core::fire_cart_unlocked(&worker.engine, &button)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "started": started })))
}

async fn stop(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StopRequest>,
) -> Result<&'static str, ApiError> {
    state
        .engine
        .lock()
        .map_err(ApiError::internal)?
        .stop_carts(req.button);
    Ok("ok")
}

async fn set_stop_after(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<StopAfterRequest>,
) -> Result<&'static str, ApiError> {
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    let index = engine
        .playlist()
        .index_of(&id)
        .ok_or_else(|| not_found(format!("No playlist item {id}")))?;
    engine.playlist_mut().items[index].stop_after = req.value;
    Ok("ok")
}

#[cfg(test)]
mod tests {
    use std::path::{Path as FsPath, PathBuf};
    use std::sync::Mutex;

    use riars_core::automation::Automation;
    use riars_core::live::CartMode;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, MediaLibrary, Scheduler};

    use super::*;

    fn temp_path(name: &str, suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "riars-api-live-{}-{name}{suffix}",
            std::process::id()
        ))
    }

    fn remove_db_files(path: &FsPath) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    fn test_state(name: &str) -> (Arc<AppState>, PathBuf) {
        let path = temp_path(name, ".db");
        remove_db_files(&path);
        let library = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        let engine = AudioEngine::new_headless().unwrap();
        let state = Arc::new(AppState {
            engine: Mutex::new(engine),
            library: Mutex::new(library),
            scheduler: Mutex::new(Scheduler::new()),
            schedule_path: temp_path(name, ".schedule.json"),
            api_token: "t".into(),
            stream: None,
            rds: riars_rds::RdsService::new(None),
            session: Mutex::new(SessionSaver::new(temp_path(name, ".session.json"))),
            automation: Mutex::new(Automation::new()),
            automation_events: tokio::sync::broadcast::channel(4).0,
        });
        (state, path)
    }

    fn cleanup(state: Arc<AppState>, path: &FsPath) {
        drop(state);
        remove_db_files(path);
    }

    fn must<T>(result: Result<T, ApiError>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("{}", error.1),
        }
    }

    fn must_err<T>(result: Result<T, ApiError>) -> ApiError {
        match result {
            Ok(_) => panic!("expected an error"),
            Err(error) => error,
        }
    }

    fn button(page: u8, slot: u8, label: &str) -> CartButton {
        CartButton {
            page,
            slot,
            track_id: None,
            file_path: "/sounds/clap.wav".to_string(),
            label: label.to_string(),
            color: String::new(),
            gain_db: 0.0,
            mode: CartMode::Fire,
            duck: false,
        }
    }

    fn bank(name: &str, buttons: Vec<CartButton>) -> CartBank {
        CartBank {
            id: 0,
            name: name.to_string(),
            buttons,
        }
    }

    #[tokio::test]
    async fn play_mode_round_trips() {
        let (state, path) = test_state("mode");

        let Json(status) = must(
            set_mode(
                State(Arc::clone(&state)),
                Json(ModeRequest {
                    mode: PlayMode::Manual,
                }),
            )
            .await,
        );
        assert_eq!(status["mode"], "Manual");

        let Json(status) = must(get_status(State(Arc::clone(&state))).await);
        assert_eq!(status["mode"], "Manual");
        assert!(status["mic"].is_object());
        assert!(status["carts"].is_array());

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn microphone_opening_is_refused_by_default() {
        let (state, path) = test_state("mic");
        let err = must_err(
            set_mic(
                State(Arc::clone(&state)),
                Json(MicRequest { on: true }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("remotely"));

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn microphone_config_validation_rejects_bad_values() {
        let (state, path) = test_state("mic-config");
        let mut config = MicConfig::default();
        config.gain_db = 100.0;
        let err = must_err(
            set_mic_config(State(Arc::clone(&state)), Json(config)).await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        let config = MicConfig {
            allow_remote: true,
            ..MicConfig::default()
        };
        let Json(saved) = must(
            set_mic_config(State(Arc::clone(&state)), Json(config)).await,
        );
        assert!(saved.allow_remote);
        let Json(loaded) = must(get_mic_config(State(Arc::clone(&state))).await);
        assert!(loaded.allow_remote);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn banks_buttons_and_active_bank_round_trip() {
        let (state, path) = test_state("banks");

        let Json(saved) = must(
            save_bank(
                State(Arc::clone(&state)),
                Json(bank("Bed", vec![button(0, 0, "One"), button(1, 2, "Two")])),
            )
            .await,
        );
        let id = saved["id"].as_i64().unwrap();
        assert!(id > 0);

        let Json(list) = must(list_banks(State(Arc::clone(&state))).await);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].buttons, 2);

        let Json(loaded) = must(get_bank(State(Arc::clone(&state)), Path(id)).await);
        assert_eq!(loaded.name, "Bed");
        assert_eq!(loaded.buttons.len(), 2);

        must(
            put_button(
                State(Arc::clone(&state)),
                Path(id),
                Json(button(2, 3, "Three")),
            )
            .await,
        );
        let Json(loaded) = must(get_bank(State(Arc::clone(&state)), Path(id)).await);
        assert_eq!(loaded.buttons.len(), 3);

        let Json(deleted) = must(
            delete_button(State(Arc::clone(&state)), Path((id, 2, 3))).await,
        );
        assert_eq!(deleted["deleted"], true);
        let Json(loaded) = must(get_bank(State(Arc::clone(&state)), Path(id)).await);
        assert_eq!(loaded.buttons.len(), 2);

        let Json(active) = must(
            set_active_bank(State(Arc::clone(&state)), Path(id)).await,
        );
        assert_eq!(active["active"], id);

        let Json(deleted) = must(delete_bank(State(Arc::clone(&state)), Path(id)).await);
        assert_eq!(deleted["deleted"], true);
        let err = must_err(get_bank(State(Arc::clone(&state)), Path(id)).await);
        assert_eq!(err.0, StatusCode::NOT_FOUND);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn firing_an_empty_button_is_not_found() {
        let (state, path) = test_state("fire");
        let Json(saved) = must(
            save_bank(State(Arc::clone(&state)), Json(bank("Empty", vec![]))).await,
        );
        let id = saved["id"].as_i64().unwrap();

        let err = must_err(
            fire(
                State(Arc::clone(&state)),
                Json(FireRequest {
                    bank_id: Some(id),
                    page: 0,
                    slot: 0,
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::NOT_FOUND);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn stop_after_updates_a_known_item_and_rejects_unknown_ids() {
        let (state, path) = test_state("stop-after");

        let err = must_err(
            set_stop_after(
                State(Arc::clone(&state)),
                Path("missing".to_string()),
                Json(StopAfterRequest { value: true }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::NOT_FOUND);

        let item_id = {
            let mut engine = state.engine.lock().unwrap();
            engine.playlist_mut().add_item(
                "/tmp/a.mp3".to_string(),
                "A".to_string(),
                String::new(),
                0,
            );
            engine.playlist().items[0].id.clone()
        };

        must(
            set_stop_after(
                State(Arc::clone(&state)),
                Path(item_id.clone()),
                Json(StopAfterRequest { value: true }),
            )
            .await,
        );
        let engine = state.engine.lock().unwrap();
        assert!(engine.playlist().items[0].stop_after);
        drop(engine);

        cleanup(state, &path);
    }
}
