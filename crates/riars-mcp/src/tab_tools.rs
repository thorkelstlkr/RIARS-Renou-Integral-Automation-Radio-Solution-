//! Playlist tabs: lists open for editing next to the one on air, e.g. a programme's
//! inserts laid out ahead (see `riars_core::playlist::tabs`). The playlist tools keep
//! working on the list on air.

use riars_core::playlist::playlist::PlaylistItem;
use riars_core::playlist::tabs::{self, CopyTo};
use serde::Serialize;
use serde_json::{json, Value};

use crate::tools::{text_result, ToolDef};
use crate::McpState;

const TOOL_NAMES: &[&str] = &[
    "list_playlist_tabs",
    "get_playlist_tab",
    "open_playlist_tab",
    "update_playlist_tab",
    "close_playlist_tab",
    "add_to_playlist_tab",
    "remove_from_playlist_tab",
    "copy_playlist_items",
    "put_playlist_tab_on_air",
];

pub fn tool_defs() -> Vec<ToolDef> {
    let tab = json!({ "type": "string", "description": "Tab id, from list_playlist_tabs" });
    let position = json!({
        "type": "string",
        "enum": ["End", "Next"],
        "description": "End (default), or Next: right after the item on air (top of a list that is not on air)"
    });
    vec![
        def(
            "list_playlist_tabs",
            "List the playlist tabs: lists open for editing in the desktop app, each with its id, name, planned start_time, item count and length. on_air is the id of the one playing; only that list plays.",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "get_playlist_tab",
            "Get the items of a playlist tab (the live list for the tab on air).",
            json!({ "type": "object", "properties": { "id": tab }, "required": ["id"] }),
        ),
        def(
            "open_playlist_tab",
            "Open a new playlist tab, empty or with a copy of a saved playlist, to prepare a list (for example a programme's inserts) without touching the air.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "from_saved": { "type": "string", "description": "Saved playlist id or name to copy" }
                },
                "required": ["name"]
            }),
        ),
        def(
            "update_playlist_tab",
            "Rename a playlist tab and/or set when its list is planned to start (\"HH:MM\" or \"HH:MM:SS\"; empty string clears it).",
            json!({
                "type": "object",
                "properties": { "id": tab, "name": { "type": "string" }, "start_time": { "type": "string" } },
                "required": ["id"]
            }),
        ),
        def(
            "close_playlist_tab",
            "Close a playlist tab; its list is discarded (save it as a playlist first to keep it). The tab on air cannot be closed.",
            json!({ "type": "object", "properties": { "id": tab }, "required": ["id"] }),
        ),
        def(
            "add_to_playlist_tab",
            "Add audio files to a playlist tab.",
            json!({
                "type": "object",
                "properties": {
                    "id": tab,
                    "paths": { "type": "array", "items": { "type": "string" }, "description": "File paths, usually from search_library" },
                    "position": position
                },
                "required": ["id", "paths"]
            }),
        ),
        def(
            "remove_from_playlist_tab",
            "Remove items (by item id) from a playlist tab.",
            json!({
                "type": "object",
                "properties": { "id": tab, "item_ids": { "type": "array", "items": { "type": "string" } } },
                "required": ["id", "item_ids"]
            }),
        ),
        def(
            "copy_playlist_items",
            "Copy or move items (by item id) from one playlist tab to another. Leave out from or to for the list on air.",
            json!({
                "type": "object",
                "properties": {
                    "item_ids": { "type": "array", "items": { "type": "string" } },
                    "from": tab,
                    "to": tab,
                    "position": position,
                    "move": { "type": "boolean", "description": "Remove them from the source (default false)" }
                },
                "required": ["item_ids"]
            }),
        ),
        def(
            "put_playlist_tab_on_air",
            "Make a playlist tab the list on air. Nothing is cut: the track playing ends, then this list follows. The list that was on air keeps what it had left to play in its own tab. Ask the operator before doing this.",
            json!({ "type": "object", "properties": { "id": tab }, "required": ["id"] }),
        ),
    ]
}

fn def(name: &'static str, description: &'static str, input_schema: Value) -> ToolDef {
    ToolDef { name, description, input_schema }
}

pub fn handle(name: &str, args: &Value, state: &McpState) -> Option<Result<Value, String>> {
    if !TOOL_NAMES.contains(&name) {
        return None;
    }
    Some(dispatch(name, args, state))
}

fn strings(args: &Value, key: &str) -> Result<Vec<String>, String> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null)).map_err(|e| format!("Bad {key}: {e}"))
}

fn position(args: &Value) -> Result<CopyTo, String> {
    match args.get("position").and_then(Value::as_str) {
        None | Some("End") => Ok(CopyTo::End),
        Some("Next") => Ok(CopyTo::Next),
        Some(other) => Err(format!("Bad position: {other} (End or Next)")),
    }
}

fn dispatch(name: &str, args: &Value, state: &McpState) -> Result<Value, String> {
    let (engine, library) = (&*state.engine, &*state.library);
    let id = || args["id"].as_str().ok_or_else(|| "Missing id".to_string());
    match name {
        "list_playlist_tabs" => json_compact(&tabs::list(engine, library)?),
        "get_playlist_tab" => json_compact(&tabs::edit_list(engine, library, Some(id()?), |_| ())?.1),
        "open_playlist_tab" => {
            let name = args["name"].as_str().ok_or("Missing name")?;
            json_compact(&tabs::open(library, name, args["from_saved"].as_str())?)
        }
        "update_playlist_tab" => {
            let id = id()?;
            if let Some(name) = args["name"].as_str() {
                tabs::rename(engine, library, id, name)?;
            }
            if let Some(start) = args["start_time"].as_str() {
                tabs::set_start(engine, library, id, Some(start))?;
            }
            json_compact(&tabs::list(engine, library)?)
        }
        "close_playlist_tab" => json_compact(&tabs::close(engine, library, id()?)?),
        "add_to_playlist_tab" => {
            let paths = strings(args, "paths")?;
            if paths.is_empty() || paths.len() > 10_000 {
                return Err("Give between 1 and 10000 paths".to_string());
            }
            let items: Vec<PlaylistItem> = {
                let library = library.lock().map_err(|e| e.to_string())?;
                let guard = library.media_guard()?;
                for path in &paths {
                    guard.check(path)?;
                }
                paths
                    .into_iter()
                    .map(|path| match library.track_by_path(&path).ok().flatten() {
                        Some(t) => PlaylistItem::new(t.file_path, t.title, t.artist, t.duration_ms),
                        None => {
                            let title = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                            PlaylistItem::new(path, title, String::new(), 0)
                        }
                    })
                    .collect()
            };
            let at = position(args)?;
            let (_, list) = tabs::edit_list(engine, library, Some(id()?), |list| {
                let index = match at {
                    CopyTo::End => list.len(),
                    CopyTo::Next => list.next_position(),
                };
                list.insert_items(index, items);
            })?;
            json_compact(&list)
        }
        "remove_from_playlist_tab" => {
            let ids = strings(args, "item_ids")?;
            let (removed, _) = tabs::edit_list(engine, library, Some(id()?), |list| list.remove_ids(&ids))?;
            Ok(text_result(&format!("Removed {removed} items")))
        }
        "copy_playlist_items" => {
            let ids = strings(args, "item_ids")?;
            let remove = args["move"].as_bool().unwrap_or(false);
            let count = tabs::copy_items(engine, library, &ids, args["from"].as_str(), args["to"].as_str(), position(args)?, remove)?;
            Ok(text_result(&format!("{} {count} items", if remove { "Moved" } else { "Copied" })))
        }
        "put_playlist_tab_on_air" => json_compact(&tabs::put_on_air(engine, library, id()?)?),
        _ => Err(format!("Unknown playlist tab tool: {name}")),
    }
}

fn json_compact<T: Serialize>(value: &T) -> Result<Value, String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    Ok(text_result(&text))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    use riars_core::automation::Automation;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, MediaLibrary, Scheduler};

    use super::*;

    fn test_state() -> McpState {
        let base = std::env::temp_dir().join(format!("riars-mcp-tabs-{}", std::process::id()));
        McpState {
            engine: Arc::new(Mutex::new(AudioEngine::new_headless().unwrap())),
            library: Arc::new(Mutex::new(MediaLibrary::open(":memory:").unwrap())),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            stream: None,
            schedule_path: base.with_extension("schedule.json"),
            session: Mutex::new(SessionSaver::new(base.with_extension("session.json"))),
            automation: Arc::new(Mutex::new(Automation::new())),
        }
    }

    fn call(state: &McpState, name: &str, args: Value) -> Value {
        let text = match handle(name, &args, state).unwrap() {
            Ok(result) => result["content"][0]["text"].as_str().unwrap_or_default().to_string(),
            Err(e) => panic!("{name}: {e}"),
        };
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    }

    #[test]
    fn tools_are_listed_once() {
        let all = serde_json::to_value(crate::tools::list_tools()).unwrap();
        let mut names = HashSet::new();
        for tool in all["tools"].as_array().unwrap() {
            assert!(names.insert(tool["name"].as_str().unwrap().to_string()), "duplicate tool {}", tool["name"]);
        }
        for name in TOOL_NAMES {
            assert!(names.contains(*name), "{name} is missing from list_tools");
        }
    }

    #[test]
    fn prepare_a_programme_and_put_it_on_air() {
        let state = test_state();
        let tab = call(&state, "open_playlist_tab", json!({ "name": "Programa" }));
        let id = tab["id"].as_str().unwrap().to_string();
        let list = call(&state, "add_to_playlist_tab", json!({ "id": id, "paths": ["/s/a.mp3", "/s/b.mp3"] }));
        assert_eq!(list["items"].as_array().unwrap().len(), 2);
        assert_eq!(list["items"][0]["title"], "a");

        let tabs = call(&state, "update_playlist_tab", json!({ "id": id, "start_time": "10:30" }));
        let found = tabs["tabs"].as_array().unwrap().iter().find(|t| t["id"] == id.as_str()).unwrap().clone();
        assert_eq!(found["start_time"], "10:30:00");

        let first = list["items"][0]["id"].as_str().unwrap().to_string();
        let copied = call(&state, "copy_playlist_items", json!({ "item_ids": [first], "from": id }));
        assert_eq!(copied, "Copied 1 items");
        assert_eq!(state.engine.lock().unwrap().playlist().len(), 1, "copied to the list on air");

        let tabs = call(&state, "put_playlist_tab_on_air", json!({ "id": id }));
        assert_eq!(tabs["on_air"], id.as_str());
        assert!(handle("close_playlist_tab", &json!({ "id": id }), &state).unwrap().is_err());
        assert!(handle("update_playlist_tab", &json!({ "id": id, "start_time": "nope" }), &state).unwrap().is_err());
    }

    #[test]
    fn destructive_tools_need_confirm_and_paths_stay_in_roots() {
        let state = test_state();
        let err = crate::tools::handle_tool_call("close_playlist_tab", &json!({ "id": "x" }), &state).unwrap_err();
        assert!(err.contains("confirm"), "{err}");
        let tools = serde_json::to_value(crate::tools::list_tools_checked()).unwrap();
        let clear = tools["tools"].as_array().unwrap().iter().find(|t| t["name"] == "playlist_clear").unwrap();
        assert!(clear["inputSchema"]["required"].as_array().unwrap().contains(&json!("confirm")));

        let outside = std::env::temp_dir().to_string_lossy().into_owned();
        state.library.lock().unwrap().set_media_roots(&[]).unwrap();
        let err = crate::tools::handle_tool_call("scan_folder", &json!({ "path": outside }), &state).unwrap_err();
        assert!(err.contains("--media-root"), "{err}");
    }

    #[test]
    fn control_characters_are_stripped() {
        assert_eq!(crate::clean_text("Song\u{1b}[31m\nnext\tline\u{7}"), "Song[31m\nnext\tline");
    }
}
