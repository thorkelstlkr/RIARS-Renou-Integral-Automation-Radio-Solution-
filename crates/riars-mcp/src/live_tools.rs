use serde::Serialize;
use serde_json::{json, Value};

use riars_core::live::CartButton;
use riars_core::{MicStatus, PlayMode};

use crate::tools::{text_result, ToolDef};
use crate::McpState;

const TOOL_NAMES: &[&str] = &[
    "get_live_status",
    "set_play_mode",
    "set_microphone",
    "list_cart_banks",
    "get_cart_bank",
    "fire_cart",
    "stop_carts",
    "set_stop_after",
];

pub fn tool_defs() -> Vec<ToolDef> {
    vec![
        def(
            "get_live_status",
            "Get the live broadcast state: play mode, whether the microphone is on air and speaking (with its level and device) and which sound-bank buttons are playing",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "set_play_mode",
            "How the playlist moves on: Automatic (the list advances and the automation fills it), Assisted (the list advances but the automation adds nothing) or Manual (every item stops at its end and waits for Play or Next)",
            json!({
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["Automatic", "Assisted", "Manual"] }
                },
                "required": ["mode"]
            }),
        ),
        def(
            "set_microphone",
            "Put the microphone on or off air. Opening it remotely is refused unless the microphone settings allow remote control; closing it is always allowed.",
            json!({
                "type": "object",
                "properties": { "on": { "type": "boolean" } },
                "required": ["on"]
            }),
        ),
        def(
            "list_cart_banks",
            "List the sound banks (cartwall): id, name and number of buttons",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "get_cart_bank",
            "Get one sound bank with all its buttons (page, slot, label, file and how a press behaves)",
            json!({
                "type": "object",
                "properties": { "bank_id": { "type": "integer" } },
                "required": ["bank_id"]
            }),
        ),
        def(
            "fire_cart",
            "Play a sound-bank button, by page and slot or by label (case-insensitive). Without bank_id the active bank is used. Fails if the label is ambiguous or the button does not exist.",
            json!({
                "type": "object",
                "properties": {
                    "bank_id": { "type": "integer", "description": "Defaults to the active bank" },
                    "page": { "type": "integer", "minimum": 0 },
                    "slot": { "type": "integer", "minimum": 0 },
                    "label": { "type": "string", "description": "Button label, case-insensitive; use instead of page and slot" }
                }
            }),
        ),
        def(
            "stop_carts",
            "Stop the playing sound-bank buttons: one by its audio button id, or all of them",
            json!({
                "type": "object",
                "properties": { "button": { "type": "integer", "description": "Button id (page * 24 + slot); omit to stop all" } }
            }),
        ),
        def(
            "set_stop_after",
            "Make the playlist stop when this item ends (value true), or let it go on (false)",
            json!({
                "type": "object",
                "properties": {
                    "item_id": { "type": "string" },
                    "value": { "type": "boolean" }
                },
                "required": ["item_id", "value"]
            }),
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

fn mic_json(status: &MicStatus) -> Value {
    json!({
        "on": status.on,
        "speaking": status.speaking,
        "level": status.level,
        "device": status.device,
        "error": status.error,
    })
}

pub fn handle(name: &str, args: &Value, state: &McpState) -> Option<Result<Value, String>> {
    if !TOOL_NAMES.contains(&name) {
        return None;
    }
    Some(dispatch(name, args, state))
}

fn dispatch(name: &str, args: &Value, state: &McpState) -> Result<Value, String> {
    match name {
        "get_live_status" => {
            let engine = state.engine.lock().map_err(|e| e.to_string())?;
            json_compact(&json!({
                "mode": engine.play_mode(),
                "microphone": mic_json(&engine.mic_status()),
                "carts": engine.carts_playing(),
            }))
        }
        "set_play_mode" => {
            let mode: PlayMode = serde_json::from_value(
                args.get("mode").cloned().ok_or("Missing mode")?,
            )
            .map_err(|e| format!("Invalid mode: {e}"))?;
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            engine.set_play_mode(mode);
            json_compact(&json!({ "mode": engine.play_mode() }))
        }
        "set_microphone" => {
            let on = args["on"].as_bool().ok_or("Missing on")?;
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            if on && !engine.mic_config().allow_remote {
                return Err(
                    "Opening the microphone remotely is disabled in the microphone settings"
                        .to_string(),
                );
            }
            engine.set_mic_on(on, true)?;
            json_compact(&mic_json(&engine.mic_status()))
        }
        "list_cart_banks" => {
            let banks = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .cart_banks()?;
            json_compact(&banks)
        }
        "get_cart_bank" => {
            let bank_id = args["bank_id"].as_i64().ok_or("Missing bank_id")?;
            let bank = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .cart_bank(bank_id)?;
            match bank {
                Some(bank) => json_compact(&bank),
                None => Err(format!("No cart bank {bank_id}")),
            }
        }
        "fire_cart" => {
            let requested = args.get("bank_id").and_then(Value::as_i64);
            let label = args
                .get("label")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|label| !label.is_empty());
            let page = args.get("page").and_then(Value::as_u64).map(|v| v as u8);
            let slot = args.get("slot").and_then(Value::as_u64).map(|v| v as u8);
            let button = {
                let library = state.library.lock().map_err(|e| e.to_string())?;
                let bank_id = match requested {
                    Some(id) => id,
                    None => library
                        .active_cart_bank()?
                        .ok_or("No active cart bank")?,
                };
                let bank = library
                    .cart_bank(bank_id)?
                    .ok_or_else(|| format!("No cart bank {bank_id}"))?;
                if let Some(label) = label {
                    let mut matches: Vec<CartButton> = bank
                        .buttons
                        .into_iter()
                        .filter(|button| button.label.eq_ignore_ascii_case(label))
                        .collect();
                    match matches.len() {
                        0 => {
                            return Err(format!(
                                "No button labelled {label} in bank {bank_id}"
                            ))
                        }
                        1 => matches.remove(0),
                        _ => {
                            return Err(format!(
                                "More than one button is labelled {label} in bank {bank_id}"
                            ))
                        }
                    }
                } else {
                    let (page, slot) = match (page, slot) {
                        (Some(page), Some(slot)) => (page, slot),
                        _ => return Err("Give a label, or a page and a slot".to_string()),
                    };
                    bank.buttons
                        .into_iter()
                        .find(|button| button.page == page && button.slot == slot)
                        .ok_or_else(|| {
                            format!(
                                "No button at page {page} slot {slot} in bank {bank_id}"
                            )
                        })?
                }
            };
            let started = riars_core::fire_cart_unlocked(&state.engine, &button)?;
            json_compact(&json!({ "started": started }))
        }
        "stop_carts" => {
            let button = args
                .get("button")
                .and_then(Value::as_u64)
                .map(|button| button as u32);
            state
                .engine
                .lock()
                .map_err(|e| e.to_string())?
                .stop_carts(button);
            Ok(text_result("Cart sounds stopped"))
        }
        "set_stop_after" => {
            let item_id = args["item_id"].as_str().ok_or("Missing item_id")?;
            let value = args["value"].as_bool().ok_or("Missing value")?;
            let mut engine = state.engine.lock().map_err(|e| e.to_string())?;
            let index = engine
                .playlist()
                .index_of(item_id)
                .ok_or_else(|| format!("No playlist item {item_id}"))?;
            engine.playlist_mut().items[index].stop_after = value;
            Ok(text_result(if value {
                "The playlist will stop after this item"
            } else {
                "The playlist will go on after this item"
            }))
        }
        _ => Err(format!("Unknown live tool: {name}")),
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
    use riars_core::live::{CartBank, CartMode};
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
            "riars-mcp-live-{}-{name}.db",
            std::process::id()
        ));
        remove_db_files(&path);
        let library = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        let state = McpState {
            engine: Arc::new(Mutex::new(AudioEngine::new_headless().unwrap())),
            library: Arc::new(Mutex::new(library)),
            scheduler: Arc::new(Mutex::new(Scheduler::new())),
            stream: None,
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

    fn call(state: &McpState, name: &str, args: Value) -> Result<Value, String> {
        handle(name, &args, state).unwrap_or_else(|| panic!("{name} is not a live tool"))
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

    #[test]
    fn tool_names_are_unique_and_do_not_clash() {
        let defs = tool_defs();
        let mut own = HashSet::new();
        for tool in &defs {
            assert!(own.insert(tool.name), "duplicate live tool {}", tool.name);
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
    fn set_microphone_is_refused_by_default() {
        let (state, path) = test_state("mic");
        let err = call(&state, "set_microphone", json!({ "on": true })).unwrap_err();
        assert!(err.contains("remotely"), "{err}");

        cleanup(state, &path);
    }

    #[test]
    fn fire_cart_unknown_label_errors() {
        let (state, path) = test_state("fire");
        let bank_id = {
            let mut library = state.library.lock().unwrap();
            library
                .save_cart_bank(&CartBank {
                    id: 0,
                    name: "Bed".to_string(),
                    buttons: vec![button(0, 0, "Clap")],
                })
                .unwrap()
        };

        let err = call(
            &state,
            "fire_cart",
            json!({ "bank_id": bank_id, "label": "Nope" }),
        )
        .unwrap_err();
        assert!(err.contains("Nope"), "{err}");

        cleanup(state, &path);
    }

    #[test]
    fn play_mode_round_trips() {
        let (state, path) = test_state("mode");
        let result = call(&state, "set_play_mode", json!({ "mode": "Manual" })).unwrap();
        assert_eq!(parse_text(&result)["mode"], "Manual");
        assert_eq!(state.engine.lock().unwrap().play_mode(), PlayMode::Manual);

        let result = call(&state, "set_play_mode", json!({ "mode": "Automatic" })).unwrap();
        assert_eq!(parse_text(&result)["mode"], "Automatic");
        assert_eq!(
            state.engine.lock().unwrap().play_mode(),
            PlayMode::Automatic
        );

        cleanup(state, &path);
    }

    #[test]
    fn set_stop_after_unknown_item_errors() {
        let (state, path) = test_state("stop-after");
        let err = call(
            &state,
            "set_stop_after",
            json!({ "item_id": "missing", "value": true }),
        )
        .unwrap_err();
        assert!(err.contains("missing"), "{err}");

        cleanup(state, &path);
    }
}
