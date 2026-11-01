//! RDS for an external encoder (see `riars_rds`): the settings, what is being sent,
//! and a RadioText typed by hand that replaces the song's for a while.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use riars_core::rds::RdsConfig;
use riars_rds::RdsStatus;
use serde::{Deserialize, Serialize};

use crate::routes::ApiError;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/rds", get(get_rds).put(set_config))
        .route("/rds/text", post(set_text).delete(clear_text))
}

#[derive(Serialize)]
struct RdsResponse {
    config: RdsConfig,
    status: RdsStatus,
}

#[derive(Deserialize)]
struct TextRequest {
    text: String,
    /// How long the text stays; missing keeps it until cleared.
    #[serde(default)]
    minutes: Option<u32>,
}

fn response(state: &AppState) -> Json<RdsResponse> {
    Json(RdsResponse { config: state.rds.config(), status: state.rds.status() })
}

async fn get_rds(State(state): State<Arc<AppState>>) -> Json<RdsResponse> {
    response(&state)
}

async fn set_config(State(state): State<Arc<AppState>>, Json(config): Json<RdsConfig>) -> Result<Json<RdsResponse>, ApiError> {
    let worker = Arc::clone(&state);
    // Saving writes a file: off the async workers.
    tokio::task::spawn_blocking(move || worker.rds.set_config(config)).await.map_err(ApiError::internal)??;
    Ok(response(&state))
}

async fn set_text(State(state): State<Arc<AppState>>, Json(req): Json<TextRequest>) -> Result<Json<RdsResponse>, ApiError> {
    state.rds.set_text(Some(&req.text), req.minutes)?;
    Ok(response(&state))
}

async fn clear_text(State(state): State<Arc<AppState>>) -> Result<Json<RdsResponse>, ApiError> {
    state.rds.set_text(None, None)?;
    Ok(response(&state))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use riars_core::automation::Automation;
    use riars_core::rds::{RdsCharset, RdsOutput, RdsProtocol, RdsTransport};
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, MediaLibrary, Scheduler};

    use super::*;

    fn temp_path(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("riars-api-rds-{}{suffix}", std::process::id()))
    }

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState {
            engine: Mutex::new(AudioEngine::new_headless().unwrap()),
            library: Mutex::new(MediaLibrary::open(":memory:").unwrap()),
            scheduler: Mutex::new(Scheduler::new()),
            schedule_path: temp_path(".schedule.json"),
            api_token: "t".into(),
            stream: None,
            rds: riars_rds::RdsService::new(Some(temp_path(".rds.json"))),
            session: Mutex::new(SessionSaver::new(temp_path(".session.json"))),
            automation: Mutex::new(Automation::new()),
            automation_events: tokio::sync::broadcast::channel(4).0,
        })
    }

    fn must<T>(result: Result<T, ApiError>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("{}", error.1),
        }
    }

    #[tokio::test]
    async fn settings_are_checked_saved_and_texts_set_by_hand() {
        let state = test_state();
        let file = temp_path(".rds.txt");
        let config = RdsConfig {
            enabled: true,
            pi: "E2A1".into(),
            ps: "RENOU".into(),
            station_text: "Radio Renou".into(),
            outputs: vec![RdsOutput {
                name: "Archivo".into(),
                enabled: true,
                transport: RdsTransport::File { path: file.to_string_lossy().into_owned() },
                protocol: RdsProtocol::Ascii { template: "{rt}".into(), charset: RdsCharset::Utf8 },
            }],
            ..RdsConfig::default()
        };
        let Json(saved) = must(set_config(State(state.clone()), Json(config.clone())).await);
        assert_eq!(saved.config, config);
        assert_eq!(saved.status.texts.rt, "Radio Renou");
        assert_eq!(riars_core::rds::load(&temp_path(".rds.json")).unwrap(), config);

        let bad = RdsConfig { pty: 40, ..config.clone() };
        assert!(set_config(State(state.clone()), Json(bad)).await.is_err());

        let Json(manual) = must(set_text(State(state.clone()), Json(TextRequest { text: "Llama al 900".into(), minutes: Some(10) })).await);
        assert_eq!(manual.status.texts.rt, "Llama al 900");
        assert!(set_text(State(state.clone()), Json(TextRequest { text: "x".repeat(65), minutes: None })).await.is_err());
        let Json(cleared) = must(clear_text(State(state.clone())).await);
        assert_eq!(cleared.status.texts.rt, "Radio Renou");
        assert_eq!(cleared.status.manual, None);

        let _ = std::fs::remove_file(temp_path(".rds.json"));
        let _ = std::fs::remove_file(file);
    }
}
