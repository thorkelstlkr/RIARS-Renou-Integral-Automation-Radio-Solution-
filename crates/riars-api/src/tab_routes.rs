//! Playlist tabs: lists open for editing next to the one on air, e.g. a programme's
//! inserts laid out ahead (see `riars_core::playlist::tabs`). `/playlist` stays the list
//! on air; a tab's id is accepted wherever a tab is named, the on-air tab's included.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use riars_core::playlist::playlist::{Playlist, PlaylistItem};
use riars_core::playlist::tabs::{self, CopyTo, Tabs};
use serde::{Deserialize, Serialize};

use crate::routes::ApiError;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/playlist/tabs", get(list).post(open))
        .route("/playlist/tabs/order", put(order))
        .route("/playlist/tabs/copy", post(copy))
        .route("/playlist/tabs/{id}", get(get_tab).patch(update).delete(close))
        .route("/playlist/tabs/{id}/items", post(add_items))
        .route("/playlist/tabs/{id}/remove", post(remove_items))
        .route("/playlist/tabs/{id}/move", post(move_item))
        .route("/playlist/tabs/{id}/on-air", post(on_air))
}

#[derive(Deserialize)]
struct OpenRequest {
    #[serde(default)]
    name: String,
    /// Open a copy of this saved playlist (id or name) instead of an empty list.
    #[serde(default)]
    from_saved: Option<String>,
}

#[derive(Deserialize)]
struct UpdateRequest {
    #[serde(default)]
    name: Option<String>,
    /// "HH:MM" or "HH:MM:SS"; an empty string clears it. Left alone when missing.
    #[serde(default)]
    start_time: Option<String>,
}

#[derive(Deserialize)]
struct NewItem {
    path: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    duration_ms: u64,
}

#[derive(Deserialize)]
struct AddRequest {
    items: Vec<NewItem>,
    #[serde(default)]
    position: CopyTo,
}

#[derive(Deserialize)]
struct IdsRequest {
    ids: Vec<String>,
}

#[derive(Deserialize)]
struct MoveRequest {
    from: usize,
    to: usize,
}

#[derive(Deserialize)]
struct CopyRequest {
    ids: Vec<String>,
    /// Tab ids; missing means the list on air.
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    position: CopyTo,
    /// Move instead of copy.
    #[serde(default)]
    remove: bool,
}

#[derive(Serialize)]
struct Copied {
    count: usize,
}

async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Tabs>, ApiError> {
    Ok(Json(tabs::list(&state.engine, &state.library)?))
}

async fn open(State(state): State<Arc<AppState>>, Json(req): Json<OpenRequest>) -> Result<Json<Playlist>, ApiError> {
    Ok(Json(tabs::open(&state.library, &req.name, req.from_saved.as_deref())?))
}

async fn get_tab(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Playlist>, ApiError> {
    Ok(Json(tabs::edit_list(&state.engine, &state.library, Some(&id), |_| ())?.1))
}

async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateRequest>,
) -> Result<Json<Tabs>, ApiError> {
    if let Some(name) = &req.name {
        tabs::rename(&state.engine, &state.library, &id, name)?;
    }
    if let Some(start) = &req.start_time {
        tabs::set_start(&state.engine, &state.library, &id, Some(start))?;
    }
    Ok(Json(tabs::list(&state.engine, &state.library)?))
}

async fn close(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Tabs>, ApiError> {
    Ok(Json(tabs::close(&state.engine, &state.library, &id)?))
}

async fn order(State(state): State<Arc<AppState>>, Json(req): Json<IdsRequest>) -> Result<Json<Tabs>, ApiError> {
    Ok(Json(tabs::order(&state.engine, &state.library, &req.ids)?))
}

async fn add_items(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AddRequest>,
) -> Result<Json<Playlist>, ApiError> {
    if req.items.len() > 10_000 {
        return Err("At most 10000 items at once".to_string().into());
    }
    let items: Vec<PlaylistItem> = req
        .items
        .into_iter()
        .map(|i| {
            let title = if i.title.is_empty() {
                std::path::Path::new(&i.path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
            } else {
                i.title
            };
            PlaylistItem::new(i.path, title, i.artist, i.duration_ms)
        })
        .collect();
    let (_, list) = tabs::edit_list(&state.engine, &state.library, Some(&id), |list| {
        let at = match req.position {
            CopyTo::End => list.len(),
            CopyTo::Next => list.next_position(),
        };
        list.insert_items(at, items);
    })?;
    Ok(Json(list))
}

async fn remove_items(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<IdsRequest>,
) -> Result<Json<Playlist>, ApiError> {
    Ok(Json(tabs::edit_list(&state.engine, &state.library, Some(&id), |list| { list.remove_ids(&req.ids); })?.1))
}

async fn move_item(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<Playlist>, ApiError> {
    Ok(Json(tabs::edit_list(&state.engine, &state.library, Some(&id), |list| list.move_item(req.from, req.to))?.1))
}

async fn on_air(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Tabs>, ApiError> {
    Ok(Json(tabs::put_on_air(&state.engine, &state.library, &id)?))
}

async fn copy(State(state): State<Arc<AppState>>, Json(req): Json<CopyRequest>) -> Result<Json<Copied>, ApiError> {
    let count = tabs::copy_items(
        &state.engine,
        &state.library,
        &req.ids,
        req.from.as_deref(),
        req.to.as_deref(),
        req.position,
        req.remove,
    )?;
    Ok(Json(Copied { count }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use riars_core::automation::Automation;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, MediaLibrary, Scheduler};

    use super::*;

    fn temp_path(name: &str, suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("riars-api-tabs-{}-{name}{suffix}", std::process::id()))
    }

    fn test_state(name: &str) -> Arc<AppState> {
        Arc::new(AppState {
            engine: Mutex::new(AudioEngine::new_headless().unwrap()),
            library: Mutex::new(MediaLibrary::open(":memory:").unwrap()),
            scheduler: Mutex::new(Scheduler::new()),
            schedule_path: temp_path(name, ".schedule.json"),
            api_token: "t".into(),
            stream: None,
            rds: riars_rds::RdsService::new(None),
            session: Mutex::new(SessionSaver::new(temp_path(name, ".session.json"))),
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
    async fn a_programme_tab_is_filled_planned_and_put_on_air() {
        let state = test_state("programme");
        let Json(tabs) = must(list(State(state.clone())).await);
        let main = tabs.on_air.clone();

        let Json(tab) = must(open(State(state.clone()), Json(OpenRequest { name: "Programa".into(), from_saved: None })).await);
        let add = AddRequest {
            items: vec![
                NewItem { path: "/spots/a.mp3".into(), title: String::new(), artist: String::new(), duration_ms: 30_000 },
                NewItem { path: "/spots/b.mp3".into(), title: "B".into(), artist: String::new(), duration_ms: 20_000 },
            ],
            position: CopyTo::End,
        };
        let Json(filled) = must(add_items(State(state.clone()), Path(tab.id.clone()), Json(add)).await);
        assert_eq!(filled.items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["a", "B"]);

        let req = UpdateRequest { name: Some("Tarde".into()), start_time: Some("10:00".into()) };
        let Json(tabs) = must(update(State(state.clone()), Path(tab.id.clone()), Json(req)).await);
        let stored = tabs.tabs.iter().find(|t| t.id == tab.id).unwrap();
        assert_eq!((stored.name.as_str(), stored.start_time.as_deref(), stored.items), ("Tarde", Some("10:00:00"), 2));

        let bad = UpdateRequest { name: None, start_time: Some("later".into()) };
        assert!(update(State(state.clone()), Path(tab.id.clone()), Json(bad)).await.is_err());

        let Json(tabs) = must(on_air(State(state.clone()), Path(tab.id.clone())).await);
        assert_eq!(tabs.on_air, tab.id);
        assert_eq!(state.engine.lock().unwrap().playlist().len(), 2);
        assert!(close(State(state.clone()), Path(tab.id.clone())).await.is_err());
        let Json(tabs) = must(close(State(state.clone()), Path(main)).await);
        assert_eq!(tabs.tabs.len(), 1);
    }
}
