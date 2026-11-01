//! Completing track metadata from MusicBrainz, AcoustID and the Cover Art Archive
//! (see `riars-meta`). A lookup only proposes; `apply` stores what the client accepts.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use riars_core::TrackMetadata;
use riars_meta::{ApplyItem, ApplyOutcome, LookupStatus};
use serde::{Deserialize, Serialize};

use crate::analysis_routes::ApiLibrary;
use crate::routes::ApiError;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/library/metadata/lookup", get(lookup_status).post(start_lookup))
        .route("/library/metadata/cancel", post(cancel_lookup))
        .route("/library/metadata/apply", post(apply))
        .route("/library/metadata/settings", get(get_settings).put(set_settings))
        .route("/tracks/{id}/metadata", get(track_metadata))
}

#[derive(Deserialize)]
struct LookupRequest {
    /// Tracks to look up; empty or missing: every track with gaps (one minute or longer).
    #[serde(default)]
    track_ids: Vec<i64>,
}

#[derive(Serialize)]
struct LookupStarted {
    total: usize,
}

#[derive(Deserialize)]
struct ApplyRequest {
    items: Vec<ApplyItem>,
}

#[derive(Serialize, Deserialize)]
struct MetadataSettings {
    #[serde(default)]
    acoustid_key: Option<String>,
}

/// Covers are kept next to the library database, as the schedule is.
fn covers(state: &AppState) -> PathBuf {
    riars_meta::covers_dir(state.schedule_path.parent().unwrap_or(std::path::Path::new(".")))
}

async fn start_lookup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LookupRequest>,
) -> Result<Json<LookupStarted>, ApiError> {
    let library = Arc::new(ApiLibrary(Arc::downgrade(&state)));
    let total = tokio::task::spawn_blocking(move || riars_meta::job().start(library, req.track_ids))
        .await
        .map_err(ApiError::internal)??;
    Ok(Json(LookupStarted { total }))
}

async fn lookup_status() -> Json<LookupStatus> {
    Json(riars_meta::job().status())
}

async fn cancel_lookup() -> &'static str {
    riars_meta::job().cancel();
    "ok"
}

async fn apply(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApplyRequest>,
) -> Result<Json<Vec<ApplyOutcome>>, ApiError> {
    let library = ApiLibrary(Arc::downgrade(&state));
    let dir = covers(&state);
    let outcomes = tokio::task::spawn_blocking(move || riars_meta::apply(&library, &dir, req.items))
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(outcomes))
}

async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Json<MetadataSettings>, ApiError> {
    let library = state.library.lock().map_err(ApiError::internal)?;
    Ok(Json(MetadataSettings { acoustid_key: riars_meta::acoustid_key(&library)? }))
}

async fn set_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MetadataSettings>,
) -> Result<Json<MetadataSettings>, ApiError> {
    let mut library = state.library.lock().map_err(ApiError::internal)?;
    riars_meta::set_acoustid_key(&mut library, req.acoustid_key.as_deref().unwrap_or(""))?;
    Ok(Json(MetadataSettings { acoustid_key: riars_meta::acoustid_key(&library)? }))
}

async fn track_metadata(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<TrackMetadata>, ApiError> {
    let library = state.library.lock().map_err(ApiError::internal)?;
    library
        .track_metadata(id)?
        .map(Json)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("No track with id {id}")))
}

#[cfg(test)]
mod tests {
    use std::path::{Path as FsPath, PathBuf};
    use std::sync::Mutex;

    use riars_core::automation::Automation;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, MediaLibrary, MetadataFields, Scheduler};

    use super::*;

    fn temp_path(name: &str, suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("riars-api-metadata-{}-{name}{suffix}", std::process::id()))
    }

    fn test_state(name: &str) -> (Arc<AppState>, PathBuf) {
        let path = temp_path(name, ".db");
        for p in [path.display().to_string(), format!("{}-wal", path.display()), format!("{}-shm", path.display())] {
            let _ = std::fs::remove_file(p);
        }
        let library = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        let state = Arc::new(AppState {
            engine: Mutex::new(AudioEngine::new_headless().unwrap()),
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

    fn must<T>(result: Result<T, ApiError>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("{}", error.1),
        }
    }

    fn add_track(state: &AppState, path: &str) -> i64 {
        let mut library = state.library.lock().unwrap();
        library
            .insert_scanned(&[riars_core::library::folders::ScannedFile {
                path: path.into(),
                title: "Roma".into(),
                artist: "Amulet".into(),
                album: String::new(),
                genre: String::new(),
                duration_ms: 230_000,
            }])
            .unwrap();
        library.track_by_path(path).unwrap().unwrap().id
    }

    #[tokio::test]
    async fn settings_apply_and_track_metadata() {
        let (state, path) = test_state("apply");
        let id = add_track(&state, "/m/07 Roma.mp3");

        let Json(settings) = must(get_settings(State(Arc::clone(&state))).await);
        assert_eq!(settings.acoustid_key, None);
        let Json(settings) = must(
            set_settings(State(Arc::clone(&state)), Json(MetadataSettings { acoustid_key: Some("Key123".into()) })).await,
        );
        assert_eq!(settings.acoustid_key.as_deref(), Some("Key123"));
        let bad = set_settings(State(Arc::clone(&state)), Json(MetadataSettings { acoustid_key: Some("no spaces".into()) })).await;
        assert!(bad.is_err_and(|e| e.0 == StatusCode::BAD_REQUEST));

        let Json(outcomes) = must(
            apply(
                State(Arc::clone(&state)),
                Json(ApplyRequest {
                    items: vec![ApplyItem {
                        track_id: id,
                        fields: MetadataFields { album: Some("Petit Viking".into()), year: Some(2017), ..Default::default() },
                        source: "musicbrainz:r1".into(),
                        ..Default::default()
                    }],
                }),
            )
            .await,
        );
        assert!(outcomes[0].applied, "{:?}", outcomes[0].error);

        let Json(track) = must(track_metadata(State(Arc::clone(&state)), Path(id)).await);
        assert_eq!(track.album, "Petit Viking");
        assert_eq!(track.year, Some(2017));
        assert_eq!(track.source, "musicbrainz:r1");
        let missing = track_metadata(State(Arc::clone(&state)), Path(9999)).await;
        assert!(missing.is_err_and(|e| e.0 == StatusCode::NOT_FOUND));

        drop(state);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn a_lookup_of_unknown_tracks_is_refused() {
        let (state, path) = test_state("lookup");
        let refused = start_lookup(State(Arc::clone(&state)), Json(LookupRequest { track_ids: vec![424242] })).await;
        assert!(refused.is_err_and(|e| e.0 == StatusCode::BAD_REQUEST && e.1.contains("No tracks")));
        drop(state);
        let _ = std::fs::remove_file(FsPath::new(&path));
    }
}
