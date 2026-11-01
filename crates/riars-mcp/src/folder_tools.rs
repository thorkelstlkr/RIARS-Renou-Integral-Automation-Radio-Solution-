use serde::Serialize;
use serde_json::{json, Value};

use riars_core::library::folders::{rescan_all, scan_into};

use crate::tools::{text_result, ToolDef};
use crate::McpState;

const TOOL_NAMES: &[&str] = &[
    "list_library_folders",
    "add_library_folder",
    "remove_library_folder",
    "rescan_library",
];

pub fn tool_defs() -> Vec<ToolDef> {
    vec![
        def(
            "list_library_folders",
            "List the library folders and how many tracks each holds. Library folders are rescanned at startup.",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "add_library_folder",
            "Add a folder to the library and scan it for audio files. Library folders are rescanned at startup.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the folder to add and scan"
                    }
                },
                "required": ["path"]
            }),
        ),
        def(
            "remove_library_folder",
            "Remove a folder from the library, optionally removing its tracks. Library folders are rescanned at startup.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path of the library folder to remove"
                    },
                    "remove_tracks": {
                        "type": "boolean",
                        "description": "Also remove the folder's tracks from the library (default false)"
                    }
                },
                "required": ["path"]
            }),
        ),
        def(
            "rescan_library",
            "Rescan every library folder for new audio files. Library folders are also rescanned at startup.",
            json!({ "type": "object", "properties": {} }),
        ),
    ]
}

fn def(name: &'static str, description: &'static str, input_schema: Value) -> ToolDef {
    ToolDef {
        name,
        description,
        input_schema,
    }
}

pub fn handle(name: &str, args: &Value, state: &McpState) -> Option<Result<Value, String>> {
    if !TOOL_NAMES.contains(&name) {
        return None;
    }
    Some(dispatch(name, args, state))
}

fn dispatch(name: &str, args: &Value, state: &McpState) -> Result<Value, String> {
    match name {
        "list_library_folders" => {
            let folders = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .folders()?;
            json_compact(&folders)
        }
        "add_library_folder" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let stored = {
                let mut library = state.library.lock().map_err(|e| e.to_string())?;
                library.check_media_path(path)?;
                library.add_folder(path)?
            };
            let added = scan_into(&state.library, &stored)?;
            Ok(text_result(&format!("{stored}: {added} tracks added")))
        }
        "remove_library_folder" => {
            let path = args["path"].as_str().ok_or("Missing path")?;
            let remove_tracks = args
                .get("remove_tracks")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let removed = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .remove_folder(path, remove_tracks)?;
            Ok(text_result(&format!("{removed} tracks removed")))
        }
        "rescan_library" => {
            let added = rescan_all(&state.library)?;
            Ok(text_result(&format!("{added} tracks added")))
        }
        _ => Err(format!("Unknown folder tool: {name}")),
    }
}

fn json_compact<T: Serialize>(value: &T) -> Result<Value, String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    Ok(text_result(&text))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use riars_core::automation::Automation;
    use riars_core::library::db::MediaLibrary;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, Scheduler};

    use super::*;

    fn remove_db_files(path: &Path) {
        let mut wal = path.as_os_str().to_os_string();
        wal.push("-wal");
        let mut shm = path.as_os_str().to_os_string();
        shm.push("-shm");
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(PathBuf::from(wal));
        let _ = std::fs::remove_file(PathBuf::from(shm));
    }

    fn test_state(name: &str) -> (McpState, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "riars-mcp-folders-{}-{name}.db",
            std::process::id()
        ));
        remove_db_files(&path);
        let library = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        let state = McpState {
            engine: Arc::new(Mutex::new(AudioEngine::new_headless().unwrap())),
            library: Arc::new(Mutex::new(library)),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            stream: None,
            rds: riars_rds::RdsService::new(None),
            schedule_path: path.with_extension("schedule.json"),
            session: Mutex::new(SessionSaver::new(path.with_extension("session.json"))),
            automation: Arc::new(Mutex::new(Automation::new())),
        };
        (state, path)
    }

    fn cleanup(state: McpState, path: &Path) {
        drop(state);
        remove_db_files(path);
        let _ = std::fs::remove_file(path.with_extension("schedule.json"));
        let _ = std::fs::remove_file(path.with_extension("session.json"));
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"not really audio").unwrap();
    }

    fn music_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "riars-mcp-folders-{}-{name}-audio",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        touch(&dir.join("music").join("one.mp3"));
        touch(&dir.join("music").join("sub").join("two.mp3"));
        dir
    }

    fn call(state: &McpState, name: &str, args: Value) -> Result<Value, String> {
        handle(name, &args, state).unwrap_or_else(|| panic!("{name} is not a folder tool"))
    }

    fn text_of(result: &Value) -> String {
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn parse_text(result: &Value) -> Value {
        serde_json::from_str(&text_of(result)).unwrap()
    }

    #[test]
    fn tool_names_are_unique_and_do_not_clash() {
        let defs = tool_defs();
        let mut own = HashSet::new();
        for tool in &defs {
            assert!(own.insert(tool.name), "duplicate folder tool {}", tool.name);
        }

        let all = serde_json::to_value(crate::tools::list_tools()).unwrap();
        let tools = all["tools"].as_array().unwrap();
        let mut names = HashSet::new();
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            assert!(names.insert(name.to_string()), "duplicate tool {name}");
        }
        for name in own {
            assert!(names.contains(name), "{name} is missing from list_tools");
        }
    }

    #[test]
    fn folders_are_added_listed_and_rescanned_once() {
        let (state, path) = test_state("folders");
        let dir = music_dir("folders");
        let music = dir.join("music").to_string_lossy().into_owned();

        let result = call(&state, "add_library_folder", json!({ "path": music })).unwrap();
        assert!(text_of(&result).contains("2 tracks added"), "{}", text_of(&result));

        let result = call(&state, "list_library_folders", json!({})).unwrap();
        let folders = parse_text(&result);
        assert_eq!(folders.as_array().unwrap().len(), 1);
        assert_eq!(folders[0]["track_count"], 2);

        let result = call(&state, "add_library_folder", json!({ "path": music })).unwrap();
        assert!(text_of(&result).contains("0 tracks added"), "{}", text_of(&result));
        let result = call(&state, "list_library_folders", json!({})).unwrap();
        assert_eq!(parse_text(&result).as_array().unwrap().len(), 1);

        let result = call(&state, "rescan_library", json!({})).unwrap();
        assert!(text_of(&result).contains("0 tracks added"), "{}", text_of(&result));

        let result = call(
            &state,
            "remove_library_folder",
            json!({ "path": music, "remove_tracks": true }),
        )
        .unwrap();
        assert!(text_of(&result).contains("2 tracks removed"), "{}", text_of(&result));

        let err = call(
            &state,
            "remove_library_folder",
            json!({ "path": music }),
        )
        .unwrap_err();
        assert!(err.contains("Not a library folder"), "{err}");

        cleanup(state, &path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn adding_a_missing_folder_errors() {
        let (state, path) = test_state("missing");
        let missing = std::env::temp_dir()
            .join(format!("riars-mcp-folders-{}-nope", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let err = call(&state, "add_library_folder", json!({ "path": missing })).unwrap_err();
        assert!(err.contains("Not a directory"), "{err}");

        cleanup(state, &path);
    }
}
