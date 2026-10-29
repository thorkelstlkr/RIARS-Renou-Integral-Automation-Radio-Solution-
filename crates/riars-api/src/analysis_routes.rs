use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use riars_core::analysis::{
    analyze_track_detailed, AnalysisStats, BatchScope, BatchStatus, CueAnalysis, CueDetector,
    CueSource, LibraryAccess, TrackCues,
};
use riars_core::audio::normalize::NormalizationConfig;
use riars_core::audio::{default_output_device, output_devices};
use riars_core::MediaLibrary;
use serde::{Deserialize, Serialize};

use crate::routes::ApiError;
use crate::AppState;

const DETECTOR_KEY: &str = "cue_detector";

pub fn analysis_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tracks/cues", post(tracks_cues))
        .route("/tracks/{id}/cues", get(get_cues).put(set_cues).delete(delete_cues))
        .route("/tracks/{id}/analyze", post(analyze_track))
        .route("/analysis/start", post(start_analysis))
        .route("/analysis/cancel", post(cancel_analysis))
        .route("/analysis/status", get(analysis_status))
        .route("/analysis/stats", get(analysis_stats))
        .route("/analysis/review", get(analysis_review))
        .route("/analysis/detector", get(get_detector).put(set_detector))
        .route("/player/normalization", get(get_normalization).post(set_normalization))
        .route("/audio/devices", get(audio_devices))
}

pub(crate) struct ApiLibrary(pub(crate) std::sync::Weak<AppState>);

impl LibraryAccess for ApiLibrary {
    fn with_library(
        &self,
        f: &mut dyn FnMut(&mut MediaLibrary) -> Result<(), String>,
    ) -> Result<(), String> {
        let state = self
            .0
            .upgrade()
            .ok_or_else(|| "The server state is gone".to_string())?;
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        f(&mut library)
    }
}

fn not_found(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, message.into())
}

fn detector_from_library(library: &MediaLibrary) -> Result<CueDetector, String> {
    let stored = library.kv_get(DETECTOR_KEY)?;
    Ok(stored
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

fn validate_detector(detector: &CueDetector) -> Result<(), String> {
    if let CueDetector::Command { program, args } = detector {
        if program.trim().is_empty() {
            return Err("The detector program is empty".to_string());
        }
        if args.len() > 20 {
            return Err("A detector command has at most 20 arguments".to_string());
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct CuesRequest {
    track_ids: Vec<i64>,
}

#[derive(Deserialize)]
struct AnalyzeQuery {
    #[serde(default)]
    save: bool,
    #[serde(default)]
    peaks: bool,
}

#[derive(Deserialize)]
struct StartRequest {
    scope: BatchScope,
    #[serde(default)]
    reanalyze: bool,
}

#[derive(Deserialize)]
struct ReviewQuery {
    min_confidence: Option<f32>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct CuesBody {
    cue_in_ms: u64,
    cue_out_ms: u64,
    #[serde(default)]
    intro_end_ms: Option<u64>,
    #[serde(default)]
    outro_start_ms: Option<u64>,
}

#[derive(Serialize)]
struct AudioDevices {
    devices: Vec<String>,
    default: Option<String>,
    in_use: Option<String>,
}

async fn get_cues(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<TrackCues>, ApiError> {
    let cues = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .track_cues(id)?;
    cues.map(Json)
        .ok_or_else(|| not_found(format!("No cues for track {id}")))
}

async fn set_cues(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<CuesBody>,
) -> Result<Json<TrackCues>, ApiError> {
    let worker = Arc::clone(&state);
    let stored = tokio::task::spawn_blocking(move || {
        let mut library = worker.library.lock().map_err(ApiError::internal)?;
        library
            .get_track(id)?
            .ok_or_else(|| not_found(format!("No track {id}")))?;
        let mut cues = library.track_cues(id)?.unwrap_or(TrackCues {
            track_id: id,
            duration_ms: 0,
            cue_in_ms: 0,
            cue_out_ms: 0,
            intro_end_ms: None,
            outro_start_ms: None,
            source: CueSource::Manual,
            confidence: 1.0,
            loudness_lufs: None,
            peak_dbfs: None,
            analyzed_at: None,
        });
        cues.track_id = id;
        cues.cue_in_ms = body.cue_in_ms;
        cues.cue_out_ms = body.cue_out_ms;
        cues.intro_end_ms = body.intro_end_ms;
        cues.outro_start_ms = body.outro_start_ms;
        cues.source = CueSource::Manual;
        cues.confidence = 1.0;
        library.set_track_cues(&cues)?;
        library
            .track_cues(id)?
            .ok_or_else(|| not_found(format!("No track {id}")))
    })
    .await
    .map_err(ApiError::internal)??;
    Ok(Json(stored))
}

async fn delete_cues(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .clear_track_cues(id)?;
    if !deleted {
        return Err(not_found(format!("No track {id}")));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn tracks_cues(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CuesRequest>,
) -> Result<Json<Vec<TrackCues>>, ApiError> {
    if req.track_ids.len() > 1000 {
        return Err(ApiError::from("At most 1000 track ids".to_string()));
    }
    let worker = Arc::clone(&state);
    let cues = tokio::task::spawn_blocking(move || {
        let library = worker.library.lock().map_err(|e| e.to_string())?;
        let mut cues = Vec::with_capacity(req.track_ids.len());
        for id in req.track_ids {
            if let Some(found) = library.track_cues(id)? {
                cues.push(found);
            }
        }
        Ok::<_, String>(cues)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    Ok(Json(cues))
}

async fn analyze_track(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Query(q): Query<AnalyzeQuery>,
) -> Result<Json<CueAnalysis>, ApiError> {
    let worker = Arc::clone(&state);
    let buckets = if q.peaks { 1200 } else { 0 };
    let save = q.save;
    let analysis = tokio::task::spawn_blocking(move || {
        let (path, detector) = {
            let library = worker.library.lock().map_err(ApiError::internal)?;
            let track = library
                .get_track(id)?
                .ok_or_else(|| not_found(format!("No track {id}")))?;
            (track.file_path, detector_from_library(&library)?)
        };
        let (analysis, cues) = analyze_track_detailed(
            id,
            std::path::Path::new(&path),
            buckets,
            &detector,
            &AtomicBool::new(false),
        )?;
        if save {
            let mut library = worker.library.lock().map_err(ApiError::internal)?;
            library.save_analysis(&cues)?;
        }
        Ok::<_, ApiError>(analysis)
    })
    .await
    .map_err(ApiError::internal)??;
    Ok(Json(analysis))
}

async fn start_analysis(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StartRequest>,
) -> Result<&'static str, ApiError> {
    let detector = {
        let library = state.library.lock().map_err(ApiError::internal)?;
        detector_from_library(&library)?
    };
    state.analysis().start(
        Arc::new(ApiLibrary(Arc::downgrade(&state))),
        req.scope,
        req.reanalyze,
        detector,
    )?;
    Ok("ok")
}

async fn cancel_analysis(State(state): State<Arc<AppState>>) -> &'static str {
    state.analysis().cancel();
    "ok"
}

async fn analysis_status(State(state): State<Arc<AppState>>) -> Json<BatchStatus> {
    Json(state.analysis().status())
}

async fn analysis_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AnalysisStats>, ApiError> {
    let worker = Arc::clone(&state);
    let stats = tokio::task::spawn_blocking(move || {
        let library = worker.library.lock().map_err(|e| e.to_string())?;
        let min_confidence = library.automation_config()?.talk_over.min_confidence;
        library.analysis_stats(min_confidence)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    Ok(Json(stats))
}

async fn analysis_review(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ReviewQuery>,
) -> Result<Json<Vec<TrackCues>>, ApiError> {
    let min_confidence = q.min_confidence.unwrap_or(0.35);
    if !(0.0..=1.0).contains(&min_confidence) {
        return Err(ApiError::from(
            "min_confidence must be between 0 and 1".to_string(),
        ));
    }
    let limit = q.limit.unwrap_or(50);
    if limit == 0 || limit > 1000 {
        return Err(ApiError::from(
            "limit must be between 1 and 1000".to_string(),
        ));
    }
    let worker = Arc::clone(&state);
    let cues = tokio::task::spawn_blocking(move || {
        let library = worker.library.lock().map_err(|e| e.to_string())?;
        library.tracks_needing_review(min_confidence, limit)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    Ok(Json(cues))
}

async fn get_detector(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CueDetector>, ApiError> {
    let library = state.library.lock().map_err(ApiError::internal)?;
    let detector = detector_from_library(&library)?;
    Ok(Json(detector))
}

async fn set_detector(
    State(state): State<Arc<AppState>>,
    Json(detector): Json<CueDetector>,
) -> Result<Json<CueDetector>, ApiError> {
    validate_detector(&detector)?;
    let json = serde_json::to_string(&detector).map_err(ApiError::internal)?;
    state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .kv_set(DETECTOR_KEY, &json)?;
    Ok(Json(detector))
}

async fn get_normalization(
    State(state): State<Arc<AppState>>,
) -> Result<Json<NormalizationConfig>, ApiError> {
    let config = state
        .engine
        .lock()
        .map_err(ApiError::internal)?
        .normalization_config();
    Ok(Json(config))
}

async fn set_normalization(
    State(state): State<Arc<AppState>>,
    Json(config): Json<NormalizationConfig>,
) -> Result<Json<NormalizationConfig>, ApiError> {
    config.validate()?;
    let mut engine = state.engine.lock().map_err(ApiError::internal)?;
    engine.set_normalization(config);
    Ok(Json(engine.normalization_config()))
}

async fn audio_devices(State(state): State<Arc<AppState>>) -> Result<Json<AudioDevices>, ApiError> {
    let in_use = state
        .engine
        .lock()
        .map_err(ApiError::internal)?
        .output_device()
        .map(str::to_string);
    let (devices, default) =
        tokio::task::spawn_blocking(|| (output_devices(), default_output_device()))
            .await
            .map_err(ApiError::internal)?;
    Ok(Json(AudioDevices {
        devices,
        default,
        in_use,
    }))
}

#[cfg(test)]
mod tests {
    use std::path::{Path as FsPath, PathBuf};
    use std::sync::Mutex;

    use riars_core::automation::Automation;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, Scheduler};

    use super::*;

    fn temp_path(name: &str, suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "riars-api-analysis-{}-{name}{suffix}",
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

    fn insert_track(state: &AppState, dir: &FsPath, name: &str) -> i64 {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), b"").unwrap();
        let mut library = state.library.lock().unwrap();
        library.scan_folder(&dir.to_string_lossy()).unwrap();
        library
            .get_all_tracks()
            .unwrap()
            .into_iter()
            .find(|track| track.file_path.ends_with(name))
            .unwrap()
            .id
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

    fn cues(track_id: i64) -> TrackCues {
        TrackCues {
            track_id,
            duration_ms: 200_000,
            cue_in_ms: 500,
            cue_out_ms: 195_000,
            intro_end_ms: Some(12_500),
            outro_start_ms: Some(180_000),
            source: CueSource::Manual,
            confidence: 1.0,
            loudness_lufs: None,
            peak_dbfs: None,
            analyzed_at: None,
        }
    }

    #[tokio::test]
    async fn cues_round_trip_and_delete() {
        let (state, path) = test_state("cues");
        let dir = temp_path("cues-audio", "");
        let track = insert_track(&state, &dir, "a.mp3");

        let Json(saved) = must(
            set_cues(
                State(Arc::clone(&state)),
                Path(track),
                Json(CuesBody {
                    cue_in_ms: 500,
                    cue_out_ms: 195_000,
                    intro_end_ms: Some(12_500),
                    outro_start_ms: Some(180_000),
                }),
            )
            .await,
        );
        assert_eq!(saved.track_id, track);
        assert_eq!(saved.source, CueSource::Manual);
        assert_eq!(saved.confidence, 1.0);

        let Json(loaded) = must(get_cues(State(Arc::clone(&state)), Path(track)).await);
        assert_eq!(loaded, saved);

        let Json(all) = must(
            tracks_cues(
                State(Arc::clone(&state)),
                Json(CuesRequest { track_ids: vec![track, 9_999] }),
            )
            .await,
        );
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].track_id, track);

        let _ = must(delete_cues(State(Arc::clone(&state)), Path(track)).await);
        let err = must_err(get_cues(State(Arc::clone(&state)), Path(track)).await);
        assert_eq!(err.0, StatusCode::NOT_FOUND);

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn missing_cues_and_tracks_are_not_found() {
        let (state, path) = test_state("missing");
        let err = must_err(get_cues(State(Arc::clone(&state)), Path(9_999)).await);
        assert_eq!(err.0, StatusCode::NOT_FOUND);

        let dir = temp_path("missing-audio", "");
        let track = insert_track(&state, &dir, "a.mp3");
        let err = must_err(get_cues(State(Arc::clone(&state)), Path(track)).await);
        assert_eq!(err.0, StatusCode::NOT_FOUND);

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn out_of_order_marks_are_rejected() {
        let (state, path) = test_state("marks");
        let dir = temp_path("marks-audio", "");
        let track = insert_track(&state, &dir, "a.mp3");
        let bad = CuesBody {
            cue_in_ms: 5_000,
            cue_out_ms: 195_000,
            intro_end_ms: Some(4_000),
            outro_start_ms: Some(180_000),
        };

        let err = must_err(set_cues(State(Arc::clone(&state)), Path(track), Json(bad)).await);
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn set_cues_keeps_loudness_and_checks_stored_duration() {
        let (state, path) = test_state("set-cues-body");
        let dir = temp_path("set-cues-body-audio", "");
        let track = insert_track(&state, &dir, "a.mp3");
        {
            let mut library = state.library.lock().unwrap();
            let mut stored = cues(track);
            stored.source = CueSource::Auto;
            stored.loudness_lufs = Some(-14.0);
            stored.peak_dbfs = Some(-1.0);
            stored.duration_ms = 200_000;
            library.save_analysis(&stored).unwrap();
        }

        let Json(saved) = must(
            set_cues(
                State(Arc::clone(&state)),
                Path(track),
                Json(CuesBody {
                    cue_in_ms: 1_000,
                    cue_out_ms: 190_000,
                    intro_end_ms: Some(10_000),
                    outro_start_ms: Some(180_000),
                }),
            )
            .await,
        );
        assert_eq!(saved.cue_in_ms, 1_000);
        assert_eq!(saved.cue_out_ms, 190_000);
        assert_eq!(saved.loudness_lufs, Some(-14.0));
        assert_eq!(saved.peak_dbfs, Some(-1.0));

        let err = must_err(
            set_cues(
                State(Arc::clone(&state)),
                Path(track),
                Json(CuesBody {
                    cue_in_ms: 1_000,
                    cue_out_ms: 200_001,
                    intro_end_ms: None,
                    outro_start_ms: None,
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn analysis_review_rejects_out_of_range_input() {
        let (state, path) = test_state("review");
        let err = must_err(
            analysis_review(
                State(Arc::clone(&state)),
                Query(ReviewQuery {
                    min_confidence: Some(2.0),
                    limit: None,
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        let err = must_err(
            analysis_review(
                State(Arc::clone(&state)),
                Query(ReviewQuery {
                    min_confidence: None,
                    limit: Some(0),
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn detector_setting_round_trips_and_validates() {
        let (state, path) = test_state("detector");
        let detector = CueDetector::Command {
            program: "detect".to_string(),
            args: vec!["--voice".to_string()],
        };
        let Json(saved) = must(
            set_detector(State(Arc::clone(&state)), Json(detector.clone())).await,
        );
        assert_eq!(saved, detector);
        let Json(loaded) = must(get_detector(State(Arc::clone(&state))).await);
        assert_eq!(loaded, detector);

        let empty = CueDetector::Command {
            program: "   ".to_string(),
            args: Vec::new(),
        };
        let err = must_err(set_detector(State(Arc::clone(&state)), Json(empty)).await);
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        let many = CueDetector::Command {
            program: "detect".to_string(),
            args: (0..21).map(|i| i.to_string()).collect(),
        };
        let err = must_err(set_detector(State(Arc::clone(&state)), Json(many)).await);
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
    }
}
