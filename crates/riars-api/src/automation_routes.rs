use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Local, NaiveDateTime};
use riars_core::automation::model::{
    Advertiser, AutomationConfig, Campaign, Category, CategoryRule, Clock, DeliveryRow, GridRule,
    PlayLogEntry,
};
use riars_core::automation::preview;
use riars_core::automation::runner::AutomationStatus;
use riars_core::automation::{parse_api_datetime, validate};
use riars_core::playlist::playlist::PlaylistItem;
use serde::{Deserialize, Serialize};

use crate::routes::ApiError;
use crate::AppState;

pub fn automation_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/automation/status", get(status))
        .route("/automation/config", get(get_config).post(save_config))
        .route("/automation/enable", post(set_enabled))
        .route("/automation/categories", get(list_categories).post(save_category))
        .route("/automation/categories/delete", post(delete_category))
        .route("/automation/categories/tracks", post(add_category_tracks))
        .route("/automation/tracks/{id}/categories", get(track_categories))
        .route("/automation/tracks/codes", post(set_codes))
        .route("/automation/tracks/enabled", post(set_track_enabled))
        .route("/automation/codes", get(list_codes))
        .route("/automation/rules", get(list_rules).post(save_rule))
        .route("/automation/rules/delete", post(delete_rule))
        .route("/automation/rules/apply", post(apply_rules))
        .route("/automation/clocks", get(list_clocks).post(save_clock))
        .route("/automation/clocks/delete", post(delete_clock))
        .route("/automation/grid", get(list_grid).post(save_grid_rule))
        .route("/automation/grid/delete", post(delete_grid_rule))
        .route("/automation/advertisers", get(list_advertisers).post(save_advertiser))
        .route("/automation/advertisers/delete", post(delete_advertiser))
        .route("/automation/campaigns", get(list_campaigns).post(save_campaign))
        .route("/automation/campaigns/{id}", get(get_campaign))
        .route("/automation/campaigns/delete", post(delete_campaign))
        .route("/automation/preview", get(preview_get))
        .route("/automation/log", get(log_get))
        .route("/automation/log.csv", get(log_csv))
        .route("/automation/delivery", get(delivery_get))
        .route("/automation/delivery.csv", get(delivery_csv))
        .route("/automation/campaign-log.csv", get(campaign_log_csv))
}

/// Drop the library guard, then make the generator read the new data on its next tick.
fn invalidate(state: &AppState) -> Result<(), ApiError> {
    state.automation.lock().map_err(ApiError::internal)?.invalidate();
    Ok(())
}

/// The crossfade between items now, so a preview plans the same times as the air.
fn crossfade_overlap(state: &AppState) -> Result<u64, ApiError> {
    let engine = state.engine.lock().map_err(ApiError::internal)?;
    let fade = engine.crossfade_config();
    Ok(if fade.enabled { fade.duration.as_millis() as u64 } else { 0 })
}

fn not_found(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, message.into())
}

#[derive(Deserialize)]
struct EnableRequest {
    enabled: bool,
}

#[derive(Deserialize)]
struct IdRequest {
    id: i64,
}

#[derive(Deserialize)]
struct StringIdRequest {
    id: String,
}

#[derive(Serialize)]
struct Deleted {
    deleted: bool,
}

#[derive(Serialize)]
struct CountResponse {
    count: usize,
}

#[derive(Serialize)]
struct IdResponse {
    id: i64,
}

#[derive(Deserialize)]
struct CategoryTracksRequest {
    category_id: i64,
    track_ids: Vec<i64>,
    #[serde(default = "default_weight")]
    weight: f32,
    #[serde(default)]
    remove: bool,
}

fn default_weight() -> f32 {
    1.0
}

#[derive(Serialize)]
struct TrackCategoryRow {
    category_id: i64,
    weight: f32,
}

#[derive(Deserialize)]
struct CodesRequest {
    track_id: i64,
    codes: Vec<String>,
}

#[derive(Serialize)]
struct CodesResponse {
    codes: Vec<String>,
}

#[derive(Deserialize)]
struct TrackEnabledRequest {
    track_id: i64,
    enabled: bool,
}

#[derive(Deserialize)]
struct PreviewQuery {
    from: Option<String>,
    hours: Option<u32>,
}

#[derive(Deserialize)]
struct LogQuery {
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct CsvRangeQuery {
    from: Option<String>,
    to: Option<String>,
}

#[derive(Deserialize)]
struct DateRangeQuery {
    from: Option<String>,
    to: Option<String>,
}

#[derive(Deserialize)]
struct CampaignLogQuery {
    from: Option<String>,
    to: Option<String>,
    advertiser_id: Option<i64>,
    campaign_id: Option<i64>,
}

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

fn date_stamp(value: NaiveDateTime) -> String {
    value.date().format("%Y-%m-%d").to_string()
}

fn csv_response(body: String, filename: String) -> Response {
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"));
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8")),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        body,
    )
        .into_response()
}

async fn status(State(state): State<Arc<AppState>>) -> Result<Json<AutomationStatus>, ApiError> {
    let status = state.automation.lock().map_err(ApiError::internal)?.status();
    Ok(Json(status))
}

async fn get_config(State(state): State<Arc<AppState>>) -> Result<Json<AutomationConfig>, ApiError> {
    let config = state.library.lock().map_err(ApiError::internal)?.automation_config()?;
    Ok(Json(config))
}

async fn save_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<AutomationConfig>,
) -> Result<Json<AutomationConfig>, ApiError> {
    validate::validate_config(&config)?;
    {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.save_automation_config(&config)?;
    }
    invalidate(&state)?;
    Ok(Json(config))
}

async fn set_enabled(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EnableRequest>,
) -> Result<Json<AutomationConfig>, ApiError> {
    let config = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        let mut config = library.automation_config()?;
        config.enabled = req.enabled;
        library.save_automation_config(&config)?;
        config
    };
    invalidate(&state)?;
    Ok(Json(config))
}

async fn list_categories(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Category>>, ApiError> {
    let categories = state.library.lock().map_err(ApiError::internal)?.list_categories()?;
    Ok(Json(categories))
}

async fn save_category(
    State(state): State<Arc<AppState>>,
    Json(mut category): Json<Category>,
) -> Result<Json<Category>, ApiError> {
    if category.name.trim().is_empty() {
        return Err(ApiError::from("category name is required".to_string()));
    }
    let saved = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        if category.id == 0 {
            category.id = library.create_category(&category)?;
        } else {
            library.update_category(&category)?;
        }
        category
    };
    invalidate(&state)?;
    Ok(Json(saved))
}

async fn delete_category(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IdRequest>,
) -> Result<Json<Deleted>, ApiError> {
    let deleted = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.delete_category(req.id)?
    };
    if !deleted {
        return Err(not_found(format!("No category {}", req.id)));
    }
    invalidate(&state)?;
    Ok(Json(Deleted { deleted: true }))
}

async fn add_category_tracks(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CategoryTracksRequest>,
) -> Result<Json<CountResponse>, ApiError> {
    if req.track_ids.len() > 10_000 {
        return Err(ApiError::from("at most 10000 track ids per call".to_string()));
    }
    if !req.remove && !(0.01..=100.0).contains(&req.weight) {
        return Err(ApiError::from("weight must be between 0.01 and 100".to_string()));
    }
    let count = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        if req.remove {
            library.remove_tracks_from_category(req.category_id, &req.track_ids)?
        } else {
            library.add_tracks_to_category(req.category_id, &req.track_ids, req.weight)?
        }
    };
    invalidate(&state)?;
    Ok(Json(CountResponse { count }))
}

async fn track_categories(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<TrackCategoryRow>>, ApiError> {
    let rows = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .track_categories(id)?
        .into_iter()
        .map(|(category_id, weight)| TrackCategoryRow { category_id, weight })
        .collect();
    Ok(Json(rows))
}

async fn set_codes(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CodesRequest>,
) -> Result<Json<CodesResponse>, ApiError> {
    let codes: Vec<String> = req
        .codes
        .iter()
        .map(|code| code.trim().to_lowercase())
        .filter(|code| !code.is_empty())
        .collect();
    {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.set_track_codes(req.track_id, &codes)?;
    }
    invalidate(&state)?;
    Ok(Json(CodesResponse { codes }))
}

async fn set_track_enabled(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TrackEnabledRequest>,
) -> Result<&'static str, ApiError> {
    {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.set_track_enabled(req.track_id, req.enabled)?;
    }
    invalidate(&state)?;
    Ok("ok")
}

async fn list_codes(State(state): State<Arc<AppState>>) -> Result<Json<Vec<String>>, ApiError> {
    let codes = state.library.lock().map_err(ApiError::internal)?.list_codes()?;
    Ok(Json(codes))
}

async fn list_rules(State(state): State<Arc<AppState>>) -> Result<Json<Vec<CategoryRule>>, ApiError> {
    let rules = state.library.lock().map_err(ApiError::internal)?.list_category_rules()?;
    Ok(Json(rules))
}

async fn save_rule(
    State(state): State<Arc<AppState>>,
    Json(mut rule): Json<CategoryRule>,
) -> Result<Json<IdResponse>, ApiError> {
    rule.id = 0;
    let id = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.add_category_rule(&rule)?
    };
    invalidate(&state)?;
    Ok(Json(IdResponse { id }))
}

async fn delete_rule(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IdRequest>,
) -> Result<Json<Deleted>, ApiError> {
    let deleted = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.delete_category_rule(req.id)?
    };
    if !deleted {
        return Err(not_found(format!("No category rule {}", req.id)));
    }
    invalidate(&state)?;
    Ok(Json(Deleted { deleted: true }))
}

async fn apply_rules(State(state): State<Arc<AppState>>) -> Result<Json<CountResponse>, ApiError> {
    let worker = Arc::clone(&state);
    let count = tokio::task::spawn_blocking(move || {
        worker.library.lock().map_err(|e| e.to_string())?.apply_category_rules()
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    invalidate(&state)?;
    Ok(Json(CountResponse { count }))
}

async fn list_clocks(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Clock>>, ApiError> {
    let clocks = state.library.lock().map_err(ApiError::internal)?.list_clocks()?;
    Ok(Json(clocks))
}

async fn save_clock(
    State(state): State<Arc<AppState>>,
    Json(clock): Json<Clock>,
) -> Result<Json<Clock>, ApiError> {
    let saved = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        validate::validate_clock(&clock, &library)?;
        library.save_clock(&clock)?;
        clock
    };
    invalidate(&state)?;
    Ok(Json(saved))
}

async fn delete_clock(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StringIdRequest>,
) -> Result<Json<Deleted>, ApiError> {
    let deleted = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.delete_clock(&req.id)?
    };
    if !deleted {
        return Err(not_found(format!("No clock {}", req.id)));
    }
    invalidate(&state)?;
    Ok(Json(Deleted { deleted: true }))
}

async fn list_grid(State(state): State<Arc<AppState>>) -> Result<Json<Vec<GridRule>>, ApiError> {
    let rules = state.library.lock().map_err(ApiError::internal)?.list_grid_rules()?;
    Ok(Json(rules))
}

async fn save_grid_rule(
    State(state): State<Arc<AppState>>,
    Json(rule): Json<GridRule>,
) -> Result<Json<GridRule>, ApiError> {
    let saved = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        validate::validate_grid_rule(&rule, &library)?;
        library.save_grid_rule(&rule)?;
        rule
    };
    invalidate(&state)?;
    Ok(Json(saved))
}

async fn delete_grid_rule(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StringIdRequest>,
) -> Result<Json<Deleted>, ApiError> {
    let deleted = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.delete_grid_rule(&req.id)?
    };
    if !deleted {
        return Err(not_found(format!("No grid rule {}", req.id)));
    }
    invalidate(&state)?;
    Ok(Json(Deleted { deleted: true }))
}

async fn list_advertisers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Advertiser>>, ApiError> {
    let advertisers = state.library.lock().map_err(ApiError::internal)?.list_advertisers()?;
    Ok(Json(advertisers))
}

async fn save_advertiser(
    State(state): State<Arc<AppState>>,
    Json(advertiser): Json<Advertiser>,
) -> Result<Json<Advertiser>, ApiError> {
    let id = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.save_advertiser(&advertiser)?
    };
    invalidate(&state)?;
    let saved = {
        let library = state.library.lock().map_err(ApiError::internal)?;
        library
            .list_advertisers()?
            .into_iter()
            .find(|saved| saved.id == id)
            .unwrap_or(Advertiser { id, ..advertiser })
    };
    Ok(Json(saved))
}

async fn delete_advertiser(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IdRequest>,
) -> Result<Json<Deleted>, ApiError> {
    let deleted = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.delete_advertiser(req.id)?
    };
    if !deleted {
        return Err(not_found(format!("No advertiser {}", req.id)));
    }
    invalidate(&state)?;
    Ok(Json(Deleted { deleted: true }))
}

async fn list_campaigns(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Campaign>>, ApiError> {
    let campaigns = state.library.lock().map_err(ApiError::internal)?.list_campaigns()?;
    Ok(Json(campaigns))
}

async fn get_campaign(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Campaign>, ApiError> {
    let campaign = state.library.lock().map_err(ApiError::internal)?.get_campaign(id)?;
    campaign.map(Json).ok_or_else(|| not_found(format!("No campaign {id}")))
}

async fn save_campaign(
    State(state): State<Arc<AppState>>,
    Json(campaign): Json<Campaign>,
) -> Result<Json<Campaign>, ApiError> {
    let id = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.save_campaign(&campaign)?
    };
    invalidate(&state)?;
    let saved = state.library.lock().map_err(ApiError::internal)?.get_campaign(id)?;
    saved
        .map(Json)
        .ok_or_else(|| ApiError::internal(format!("No campaign {id}")))
}

async fn delete_campaign(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IdRequest>,
) -> Result<Json<Deleted>, ApiError> {
    let deleted = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.delete_campaign(req.id)?
    };
    if !deleted {
        return Err(not_found(format!("No campaign {}", req.id)));
    }
    invalidate(&state)?;
    Ok(Json(Deleted { deleted: true }))
}

async fn preview_get(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PreviewQuery>,
) -> Result<Json<Vec<PlaylistItem>>, ApiError> {
    let from = match q.from {
        Some(text) => parse_api_datetime(&text).map_err(ApiError::from)?,
        None => now(),
    };
    let hours = validate::preview_hours(q.hours)?;
    let worker = Arc::clone(&state);
    let overlap = crossfade_overlap(&state)?;
    let items = tokio::task::spawn_blocking(move || preview(&worker.library, from, hours, overlap))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::from)?;
    Ok(Json(items))
}

async fn log_get(
    State(state): State<Arc<AppState>>,
    Query(q): Query<LogQuery>,
) -> Result<Json<Vec<PlayLogEntry>>, ApiError> {
    let (from, to) = validate::datetime_range(q.from.as_deref(), q.to.as_deref())?;
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);
    let worker = Arc::clone(&state);
    let entries = tokio::task::spawn_blocking(move || {
        worker.library.lock().map_err(|e| e.to_string())?.play_log(from, to, limit)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    Ok(Json(entries))
}

async fn log_csv(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CsvRangeQuery>,
) -> Result<Response, ApiError> {
    let (from, to) = validate::datetime_range(q.from.as_deref(), q.to.as_deref())?;
    let worker = Arc::clone(&state);
    let csv = tokio::task::spawn_blocking(move || {
        worker.library.lock().map_err(|e| e.to_string())?.play_log_csv(from, to)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    Ok(csv_response(csv, format!("riars-log-{}-{}.csv", date_stamp(from), date_stamp(to))))
}

async fn delivery_get(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DateRangeQuery>,
) -> Result<Json<Vec<DeliveryRow>>, ApiError> {
    let (from, to) = validate::date_range(q.from.as_deref(), q.to.as_deref())?;
    let worker = Arc::clone(&state);
    let rows = tokio::task::spawn_blocking(move || {
        worker.library.lock().map_err(|e| e.to_string())?.delivery_report(from, to)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    Ok(Json(rows))
}

async fn delivery_csv(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DateRangeQuery>,
) -> Result<Response, ApiError> {
    let (from, to) = validate::date_range(q.from.as_deref(), q.to.as_deref())?;
    let worker = Arc::clone(&state);
    let csv = tokio::task::spawn_blocking(move || {
        worker.library.lock().map_err(|e| e.to_string())?.delivery_csv(from, to)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    Ok(csv_response(
        csv,
        format!("riars-delivery-{from}-{to}.csv"),
    ))
}

async fn campaign_log_csv(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CampaignLogQuery>,
) -> Result<Response, ApiError> {
    let (from, to) = validate::datetime_range(q.from.as_deref(), q.to.as_deref())?;
    let worker = Arc::clone(&state);
    let csv = tokio::task::spawn_blocking(move || {
        worker
            .library
            .lock()
            .map_err(|e| e.to_string())?
            .campaign_log_csv(from, to, q.advertiser_id, q.campaign_id)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::from)?;
    Ok(csv_response(
        csv,
        format!("riars-campaign-log-{}-{}.csv", date_stamp(from), date_stamp(to)),
    ))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use chrono::{NaiveDate, NaiveTime};
    use riars_core::automation::model::{ClockSlot, GridTarget, SlotItem};
    use riars_core::automation::runner::MAX_PREVIEW_HOURS;
    use riars_core::library::db::MediaLibrary;
    use riars_core::playlist::playlist::ItemKind;
    use riars_core::{AudioEngine, Scheduler};
    use riars_core::automation::Automation;
    use riars_core::session::SessionSaver;

    use super::*;

    fn temp_path(name: &str, suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("riars-api-auto-{}-{name}{suffix}", std::process::id()))
    }

    fn remove_db_files(path: &Path) {
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
            engine: std::sync::Mutex::new(engine),
            library: std::sync::Mutex::new(library),
            scheduler: std::sync::Mutex::new(Scheduler::new()),
            schedule_path: temp_path(name, ".schedule.json"),
            api_token: "t".into(),
            stream: None,
            session: std::sync::Mutex::new(SessionSaver::new(temp_path(name, ".session.json"))),
            automation: std::sync::Mutex::new(Automation::new()),
            automation_events: tokio::sync::broadcast::channel(4).0,
        });
        (state, path)
    }

    fn cleanup(state: Arc<AppState>, path: &Path) {
        drop(state);
        remove_db_files(path);
    }

    fn category(name: &str) -> Category {
        Category {
            id: 0,
            name: name.to_string(),
            kind: ItemKind::Music,
            color: String::new(),
            parent_id: None,
            filter: None,
            separation: None,
        }
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn time(hour: u32, minute: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hour, minute, 0).unwrap()
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

    #[tokio::test]
    async fn category_create_and_list() {
        let (state, path) = test_state("category");
        let Json(saved) =
            must(save_category(State(Arc::clone(&state)), Json(category("Pop"))).await);
        assert!(saved.id > 0);

        let Json(listed) = must(list_categories(State(Arc::clone(&state))).await);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Pop");
        assert_eq!(listed[0].id, saved.id);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn clock_with_missing_category_is_rejected() {
        let (state, path) = test_state("clock-validation");
        let clock = Clock {
            id: "c".to_string(),
            name: "Clock".to_string(),
            color: String::new(),
            duration_minutes: 60,
            slots: vec![ClockSlot {
                at: None,
                item: SlotItem::Fill { category_id: 999 },
            }],
            fill_category: None,
        };
        let err = must_err(save_clock(State(Arc::clone(&state)), Json(clock)).await);
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn clock_with_missing_playlist_or_track_is_rejected() {
        let (state, path) = test_state("clock-files");
        let missing_playlist = Clock {
            id: "c".to_string(),
            name: "Clock".to_string(),
            color: String::new(),
            duration_minutes: 60,
            slots: vec![ClockSlot {
                at: None,
                item: SlotItem::Playlist {
                    playlist_id: "missing".to_string(),
                    count: None,
                },
            }],
            fill_category: None,
        };
        let err = must_err(save_clock(State(Arc::clone(&state)), Json(missing_playlist)).await);
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        let missing_file = temp_path("missing-track", ".mp3");
        let _ = std::fs::remove_file(&missing_file);
        let missing_track = Clock {
            id: "c".to_string(),
            name: "Clock".to_string(),
            color: String::new(),
            duration_minutes: 60,
            slots: vec![ClockSlot {
                at: None,
                item: SlotItem::Track {
                    path: missing_file.to_string_lossy().into_owned(),
                },
            }],
            fill_category: None,
        };
        let err = must_err(save_clock(State(Arc::clone(&state)), Json(missing_track)).await);
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn empty_category_name_is_rejected() {
        let (state, path) = test_state("category-name");
        let err = must_err(save_category(State(Arc::clone(&state)), Json(category("   "))).await);
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn deleting_a_missing_category_is_not_found() {
        let (state, path) = test_state("delete-missing");
        let err = must_err(
            delete_category(State(Arc::clone(&state)), Json(IdRequest { id: 9999 })).await,
        );
        assert_eq!(err.0, StatusCode::NOT_FOUND);
        assert_eq!(err.1, "No category 9999");

        let Json(saved) =
            must(save_category(State(Arc::clone(&state)), Json(category("Pop"))).await);
        let Json(deleted) = must(
            delete_category(State(Arc::clone(&state)), Json(IdRequest { id: saved.id })).await,
        );
        assert!(deleted.deleted);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn datetime_queries_accept_space_and_reject_out_of_range() {
        let (state, path) = test_state("dates");
        let Json(entries) = must(
            log_get(
                State(Arc::clone(&state)),
                Query(LogQuery {
                    from: Some("2026-01-01 00:00:00".to_string()),
                    to: Some("2026-01-01T01:00:00".to_string()),
                    limit: None,
                }),
            )
            .await,
        );
        assert!(entries.is_empty());

        let err = must_err(
            log_get(
                State(Arc::clone(&state)),
                Query(LogQuery {
                    from: None,
                    to: Some("-262143-01-01T00:00:00".to_string()),
                    limit: None,
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        let err = must_err(
            delivery_get(
                State(Arc::clone(&state)),
                Query(DateRangeQuery {
                    from: Some("1900-01-01".to_string()),
                    to: None,
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn range_span_longer_than_366_days_is_rejected() {
        let (state, path) = test_state("spans");
        let err = must_err(
            log_csv(
                State(Arc::clone(&state)),
                Query(CsvRangeQuery {
                    from: Some("2024-01-01T00:00:00".to_string()),
                    to: Some("2026-01-01T00:00:00".to_string()),
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        let err = must_err(
            delivery_csv(
                State(Arc::clone(&state)),
                Query(DateRangeQuery {
                    from: Some("2024-01-01".to_string()),
                    to: Some("2026-01-01".to_string()),
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        let err = must_err(
            log_get(
                State(Arc::clone(&state)),
                Query(LogQuery {
                    from: Some("2026-01-02T00:00:00".to_string()),
                    to: Some("2026-01-01T00:00:00".to_string()),
                    limit: None,
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn preview_rejects_invalid_hours_and_dates() {
        let (state, path) = test_state("preview-validation");
        for hours in [0, MAX_PREVIEW_HOURS + 1] {
            let err = must_err(
                preview_get(
                    State(Arc::clone(&state)),
                    Query(PreviewQuery { from: None, hours: Some(hours) }),
                )
                .await,
            );
            assert_eq!(err.0, StatusCode::BAD_REQUEST);
        }

        let err = must_err(
            preview_get(
                State(Arc::clone(&state)),
                Query(PreviewQuery {
                    from: Some("1900-01-01T00:00:00".to_string()),
                    hours: Some(1),
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn grid_rule_with_missing_clock_is_rejected() {
        let (state, path) = test_state("grid-validation");
        let rule = GridRule {
            id: "g".to_string(),
            name: "Morning".to_string(),
            priority: 0,
            target: GridTarget::Clock { clock_id: "missing".to_string() },
            weekdays: 0x7f,
            months: 0xfff,
            weeks_of_month: 0x3f,
            start: time(6, 0),
            end: time(9, 0),
            date_from: None,
            date_to: None,
            enabled: true,
        };
        let err = must_err(save_grid_rule(State(Arc::clone(&state)), Json(rule)).await);
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn enable_flips_status_after_invalidate() {
        let (state, path) = test_state("enable");
        assert!(!state.automation.lock().unwrap().status().enabled);

        let Json(config) = must(
            set_enabled(State(Arc::clone(&state)), Json(EnableRequest { enabled: true })).await,
        );
        assert!(config.enabled);

        {
            let mut automation = state.automation.lock().unwrap();
            let library = state.library.lock().unwrap();
            automation.generator_mut().load(&*library).unwrap();
        }
        assert!(state.automation.lock().unwrap().status().enabled);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn preview_returns_items_for_a_fill_clock() {
        let (state, path) = test_state("preview");
        let audio_dir = std::env::temp_dir()
            .join(format!("riars-api-auto-{}-preview-audio", std::process::id()));
        let _ = std::fs::remove_dir_all(&audio_dir);
        std::fs::create_dir_all(&audio_dir).unwrap();
        for index in 0..30 {
            std::fs::write(audio_dir.join(format!("{index}.mp3")), b"").unwrap();
        }

        let track_ids = {
            let mut library = state.library.lock().unwrap();
            library.scan_folder(&audio_dir.to_string_lossy()).unwrap();
            library.get_all_tracks().unwrap().into_iter().map(|track| track.id).collect::<Vec<_>>()
        };
        assert_eq!(track_ids.len(), 30);

        let Json(pop) =
            must(save_category(State(Arc::clone(&state)), Json(category("Pop"))).await);
        let _ = must(
            add_category_tracks(
                State(Arc::clone(&state)),
                Json(CategoryTracksRequest {
                    category_id: pop.id,
                    track_ids,
                    weight: 1.0,
                    remove: false,
                }),
            )
            .await,
        );

        let clock = Clock {
            id: "c".to_string(),
            name: "Clock".to_string(),
            color: String::new(),
            duration_minutes: 60,
            slots: vec![ClockSlot {
                at: None,
                item: SlotItem::Fill { category_id: pop.id },
            }],
            fill_category: None,
        };
        let _ = must(save_clock(State(Arc::clone(&state)), Json(clock)).await);

        let config = AutomationConfig {
            enabled: true,
            default_clock: Some("c".to_string()),
            ..AutomationConfig::default()
        };
        let _ = must(save_config(State(Arc::clone(&state)), Json(config)).await);

        let from = Local::now().naive_local().format("%Y-%m-%dT%H:%M:%S").to_string();
        let Json(items) = must(
            preview_get(
                State(Arc::clone(&state)),
                Query(PreviewQuery { from: Some(from), hours: Some(1) }),
            )
            .await,
        );
        assert!(!items.is_empty());

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&audio_dir);
    }

    #[tokio::test]
    async fn advertiser_and_campaign_round_trip() {
        let (state, path) = test_state("campaign");
        let advertiser = Advertiser {
            id: 0,
            name: "Acme".to_string(),
            contact: "sales".to_string(),
            notes: String::new(),
        };
        let Json(saved_advertiser) =
            must(save_advertiser(State(Arc::clone(&state)), Json(advertiser)).await);
        assert!(saved_advertiser.id > 0);
        assert_eq!(saved_advertiser.name, "Acme");

        let campaign = Campaign {
            id: 0,
            advertiser_id: saved_advertiser.id,
            name: "Summer".to_string(),
            date_from: date(2024, 6, 1),
            date_to: date(2024, 6, 30),
            plays_per_day: 2,
            max_per_hour: 1,
            dayparts: Vec::new(),
            priority: 0,
            sponsorship: None,
            enabled: true,
            spots: Vec::new(),
        };
        let Json(saved_campaign) =
            must(save_campaign(State(Arc::clone(&state)), Json(campaign)).await);
        assert!(saved_campaign.id > 0);
        assert_eq!(saved_campaign.name, "Summer");

        let Json(loaded) =
            must(get_campaign(State(Arc::clone(&state)), Path(saved_campaign.id)).await);
        assert_eq!(loaded.id, saved_campaign.id);
        assert_eq!(loaded.advertiser_id, saved_advertiser.id);

        cleanup(state, &path);
    }

    #[tokio::test]
    async fn delivery_csv_has_csv_content_type() {
        let (state, path) = test_state("delivery-csv");
        let response = must(
            delivery_csv(
                State(Arc::clone(&state)),
                Query(DateRangeQuery { from: None, to: None }),
            )
            .await,
        );
        let content_type = response.headers().get(header::CONTENT_TYPE).unwrap();
        assert_eq!(content_type.to_str().unwrap(), "text/csv; charset=utf-8");
        assert!(response.headers().get(header::CONTENT_DISPOSITION).is_some());

        cleanup(state, &path);
    }
}
