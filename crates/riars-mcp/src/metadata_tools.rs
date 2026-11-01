//! Completing track metadata from MusicBrainz, AcoustID and the Cover Art Archive
//! (see `riars-meta`). A lookup only proposes; `apply_track_metadata` stores what the
//! agent, with the operator's agreement, accepts.

use std::sync::Arc;

use riars_meta::ApplyItem;
use serde::Serialize;
use serde_json::{json, Value};

use crate::tools::{text_result, ToolDef};
use crate::McpState;

const TOOL_NAMES: &[&str] = &[
    "lookup_track_metadata",
    "metadata_lookup_status",
    "cancel_metadata_lookup",
    "apply_track_metadata",
    "get_track_metadata",
    "set_acoustid_key",
];

pub fn tool_defs() -> Vec<ToolDef> {
    let fields = json!({
        "type": "object",
        "description": "Only the fields to change; leave out the rest",
        "properties": {
            "title": { "type": "string" },
            "artist": { "type": "string" },
            "album": { "type": "string" },
            "genre": { "type": "string" },
            "year": { "type": "integer" }
        }
    });
    vec![
        def(
            "lookup_track_metadata",
            "Start looking up missing title, artist, album, year, genre and cover of library tracks in MusicBrainz (by tags or 'Artist - Title' file names) and, when an AcoustID key is set, by audio fingerprint. Runs in the background at about one track every 2-4 seconds; nothing is changed. Poll metadata_lookup_status for the proposals, show them to the operator, then apply_track_metadata what they accept.",
            json!({
                "type": "object",
                "properties": {
                    "track_ids": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "Tracks to look up. Leave out to take every track missing an artist, album, genre or year that is at least a minute long"
                    }
                }
            }),
        ),
        def(
            "metadata_lookup_status",
            "Progress of the metadata lookup and its proposals: for each track the current values, the values found (fields), which of them fill a gap (suggested), the cover_url, the method (tags, file_name, fingerprint), the confidence (0-1) and the source to record.",
            json!({
                "type": "object",
                "properties": {
                    "only_found": {
                        "type": "boolean",
                        "description": "Leave out tracks with no match (default false)"
                    }
                }
            }),
        ),
        def(
            "cancel_metadata_lookup",
            "Stop the metadata lookup after the current track. Proposals found so far are kept.",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "apply_track_metadata",
            "Store metadata the operator accepted. Only the given fields change; the source is recorded with the track. Optionally downloads the cover (Cover Art Archive addresses only) and writes the values into the audio file's own tags. Never overwrite values the operator has not approved.",
            json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "track_id": { "type": "integer" },
                                "fields": fields,
                                "cover_url": { "type": "string", "description": "cover_url from the proposal, to download and keep the cover" },
                                "source": { "type": "string", "description": "source from the proposal; 'manual' when left out" },
                                "write_file": { "type": "boolean", "description": "Also write the values into the file's tags (default false)" }
                            },
                            "required": ["track_id"]
                        }
                    }
                },
                "required": ["items"]
            }),
        ),
        def(
            "get_track_metadata",
            "A track's title, artist, album, genre, year, length, file, cover file and where its metadata came from.",
            json!({
                "type": "object",
                "properties": { "track_id": { "type": "integer" } },
                "required": ["track_id"]
            }),
        ),
        def(
            "set_acoustid_key",
            "Save the AcoustID application key used to identify tracks by their audio fingerprint (free for non-commercial use at acoustid.org; commercial stations need their own key). An empty key turns fingerprinting off.",
            json!({
                "type": "object",
                "properties": { "key": { "type": "string" } },
                "required": ["key"]
            }),
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

fn dispatch(name: &str, args: &Value, state: &McpState) -> Result<Value, String> {
    match name {
        "lookup_track_metadata" => {
            let ids: Vec<i64> = match args.get("track_ids") {
                Some(Value::Null) | None => Vec::new(),
                Some(ids) => serde_json::from_value(ids.clone()).map_err(|e| format!("Bad track_ids: {e}"))?,
            };
            let total = riars_meta::job().start(Arc::new(Arc::clone(&state.library)), ids)?;
            Ok(text_result(&format!("Looking up {total} tracks; poll metadata_lookup_status")))
        }
        "metadata_lookup_status" => {
            let mut status = riars_meta::job().status();
            if args.get("only_found").and_then(Value::as_bool).unwrap_or(false) {
                status.results.retain(|p| p.found.is_some());
            }
            json_compact(&status)
        }
        "cancel_metadata_lookup" => {
            riars_meta::job().cancel();
            Ok(text_result("The lookup stops after the current track"))
        }
        "apply_track_metadata" => {
            let items: Vec<ApplyItem> =
                serde_json::from_value(args.get("items").cloned().unwrap_or(Value::Null)).map_err(|e| format!("Bad items: {e}"))?;
            if items.is_empty() {
                return Err("Nothing to apply".to_string());
            }
            let covers = riars_meta::covers_dir(state.schedule_path.parent().unwrap_or(std::path::Path::new(".")));
            json_compact(&riars_meta::apply(&state.library, &covers, items))
        }
        "get_track_metadata" => {
            let id = args["track_id"].as_i64().ok_or("Missing track_id")?;
            let track = state.library.lock().map_err(|e| e.to_string())?.track_metadata(id)?;
            json_compact(&track.ok_or_else(|| format!("No track with id {id}"))?)
        }
        "set_acoustid_key" => {
            let key = args["key"].as_str().ok_or("Missing key")?;
            let mut library = state.library.lock().map_err(|e| e.to_string())?;
            riars_meta::set_acoustid_key(&mut library, key)?;
            let on = riars_meta::acoustid_key(&library)?.is_some();
            Ok(text_result(if on { "AcoustID key saved: fingerprint lookups are on" } else { "Fingerprint lookups are off" }))
        }
        _ => Err(format!("Unknown metadata tool: {name}")),
    }
}

fn json_compact<T: Serialize>(value: &T) -> Result<Value, String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    Ok(text_result(&text))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use riars_core::automation::Automation;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, MediaLibrary, Scheduler};

    use super::*;

    fn test_state(name: &str) -> (McpState, PathBuf) {
        let path = std::env::temp_dir().join(format!("riars-mcp-metadata-{}-{name}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
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

    fn call(state: &McpState, name: &str, args: Value) -> Result<Value, String> {
        handle(name, &args, state).unwrap_or_else(|| panic!("{name} is not a metadata tool"))
    }

    fn text_of(result: &Value) -> String {
        result["content"][0]["text"].as_str().unwrap_or_default().to_string()
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
    fn apply_get_and_key() {
        let (state, path) = test_state("apply");
        let id = {
            let mut library = state.library.lock().unwrap();
            library
                .insert_scanned(&[riars_core::library::folders::ScannedFile {
                    path: "/m/x.mp3".into(),
                    title: "x".into(),
                    artist: String::new(),
                    album: String::new(),
                    genre: String::new(),
                    duration_ms: 200_000,
                }])
                .unwrap();
            library.track_by_path("/m/x.mp3").unwrap().unwrap().id
        };
        let result = call(
            &state,
            "apply_track_metadata",
            json!({ "items": [{ "track_id": id, "fields": { "artist": "Amulet", "year": 2017 }, "source": "musicbrainz:r1" }] }),
        )
        .unwrap();
        let outcomes: Value = serde_json::from_str(&text_of(&result)).unwrap();
        assert_eq!(outcomes[0]["applied"], true);

        let track: Value = serde_json::from_str(&text_of(&call(&state, "get_track_metadata", json!({ "track_id": id })).unwrap())).unwrap();
        assert_eq!(track["artist"], "Amulet");
        assert_eq!(track["year"], 2017);
        assert_eq!(track["source"], "musicbrainz:r1");
        assert!(call(&state, "get_track_metadata", json!({ "track_id": 9999 })).is_err());
        assert!(call(&state, "apply_track_metadata", json!({ "items": [] })).is_err());

        assert!(text_of(&call(&state, "set_acoustid_key", json!({ "key": "abc123" })).unwrap()).contains("on"));
        assert!(call(&state, "set_acoustid_key", json!({ "key": "a b" })).is_err());
        assert!(text_of(&call(&state, "set_acoustid_key", json!({ "key": "" })).unwrap()).contains("off"));

        assert!(call(&state, "lookup_track_metadata", json!({ "track_ids": [424242] })).is_err());
        drop(state);
        let _ = std::fs::remove_file(&path);
    }
}
