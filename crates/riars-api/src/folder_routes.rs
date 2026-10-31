use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use riars_core::library::folders::{rescan_all, scan_into};
use riars_core::LibraryFolder;
use serde::{Deserialize, Serialize};

use crate::routes::ApiError;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/library/folders",
            get(list_folders).post(add_folder).delete(remove_folder),
        )
        .route("/library/rescan", post(rescan))
}

#[derive(Deserialize)]
struct FolderRequest {
    path: String,
}

#[derive(Deserialize)]
struct RemoveFolderRequest {
    path: String,
    #[serde(default)]
    remove_tracks: bool,
}

#[derive(Serialize)]
struct AddFolderResponse {
    path: String,
    added: usize,
}

#[derive(Serialize)]
struct AddedResponse {
    added: usize,
}

#[derive(Serialize)]
struct RemovedResponse {
    removed: usize,
}

fn not_found(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, message.into())
}

async fn list_folders(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<LibraryFolder>>, ApiError> {
    let folders = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .folders()?;
    Ok(Json(folders))
}

async fn add_folder(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FolderRequest>,
) -> Result<Json<AddFolderResponse>, ApiError> {
    let stored = {
        let mut library = state.library.lock().map_err(ApiError::internal)?;
        library.check_media_path(&req.path)?;
        library.add_folder(&req.path)?
    };
    let worker = Arc::clone(&state);
    let folder = stored.clone();
    let added = tokio::task::spawn_blocking(move || scan_into(&worker.library, &folder))
        .await
        .map_err(ApiError::internal)??;
    Ok(Json(AddFolderResponse {
        path: stored,
        added,
    }))
}

async fn remove_folder(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RemoveFolderRequest>,
) -> Result<Json<RemovedResponse>, ApiError> {
    let result = state
        .library
        .lock()
        .map_err(ApiError::internal)?
        .remove_folder(&req.path, req.remove_tracks);
    match result {
        Ok(removed) => Ok(Json(RemovedResponse { removed })),
        Err(error) if error.starts_with("Not a library folder") => Err(not_found(error)),
        Err(error) => Err(ApiError::from(error)),
    }
}

async fn rescan(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AddedResponse>, ApiError> {
    let worker = Arc::clone(&state);
    let added = tokio::task::spawn_blocking(move || rescan_all(&worker.library))
        .await
        .map_err(ApiError::internal)??;
    Ok(Json(AddedResponse { added }))
}

#[cfg(test)]
mod tests {
    use std::path::{Path as FsPath, PathBuf};
    use std::sync::Mutex;

    use riars_core::automation::Automation;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, MediaLibrary, Scheduler};

    use super::*;

    fn temp_path(name: &str, suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "riars-api-folders-{}-{name}{suffix}",
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

    fn touch(path: &FsPath) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"not really audio").unwrap();
    }

    fn music_dir(name: &str) -> PathBuf {
        let dir = temp_path(name, "-audio");
        let _ = std::fs::remove_dir_all(&dir);
        touch(&dir.join("music").join("one.mp3"));
        touch(&dir.join("music").join("sub").join("two.mp3"));
        dir
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
    async fn folders_are_added_listed_and_rescanned_once() {
        let (state, path) = test_state("folders");
        let dir = music_dir("folders");
        let music = dir.join("music").to_string_lossy().into_owned();

        let Json(added) = must(
            add_folder(
                State(Arc::clone(&state)),
                Json(FolderRequest {
                    path: music.clone(),
                }),
            )
            .await,
        );
        assert_eq!(added.added, 2);
        assert_eq!(added.path, music);

        let Json(folders) = must(list_folders(State(Arc::clone(&state))).await);
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].track_count, 2);

        let Json(added) = must(
            add_folder(
                State(Arc::clone(&state)),
                Json(FolderRequest {
                    path: music.clone(),
                }),
            )
            .await,
        );
        assert_eq!(added.added, 0);
        let Json(folders) = must(list_folders(State(Arc::clone(&state))).await);
        assert_eq!(folders.len(), 1);

        let Json(added) = must(rescan(State(Arc::clone(&state))).await);
        assert_eq!(added.added, 0);

        let Json(removed) = must(
            remove_folder(
                State(Arc::clone(&state)),
                Json(RemoveFolderRequest {
                    path: music.clone(),
                    remove_tracks: true,
                }),
            )
            .await,
        );
        assert_eq!(removed.removed, 2);

        let err = must_err(
            remove_folder(
                State(Arc::clone(&state)),
                Json(RemoveFolderRequest {
                    path: music,
                    remove_tracks: false,
                }),
            )
            .await,
        );
        assert_eq!(err.0, StatusCode::NOT_FOUND);

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn adding_a_missing_folder_is_a_client_error() {
        let (state, path) = test_state("missing");
        let err = must_err(
            add_folder(
                State(Arc::clone(&state)),
                Json(FolderRequest {
                    path: temp_path("does-not-exist", "-audio")
                        .to_string_lossy()
                        .into_owned(),
                }),
            )
            .await,
        );
        assert!(err.0.is_client_error());

        cleanup(state, &path);
    }
}
