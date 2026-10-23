use std::collections::HashSet;

use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use riars_core::automation::model::{
    Advertiser, AutomationConfig, Campaign, Category, CategoryRule, Clock, GridRule, GridTarget,
    SlotItem,
};
use riars_core::automation::runner::MAX_PREVIEW_HOURS;
use riars_core::automation::{parse_api_date, parse_api_datetime};
use riars_core::library::db::MediaLibrary;
use riars_core::playlist::playlist::PlaylistItem;

use crate::tools::{text_result, ToolDef};
use crate::McpState;

const TOOL_NAMES: &[&str] = &[
    "automation_status",
    "set_automation_enabled",
    "get_automation_config",
    "set_automation_config",
    "list_categories",
    "save_category",
    "delete_category",
    "assign_tracks_to_category",
    "set_track_codes",
    "set_track_enabled",
    "list_codes",
    "list_category_rules",
    "add_category_rule",
    "delete_category_rule",
    "apply_category_rules",
    "list_clocks",
    "save_clock",
    "delete_clock",
    "list_grid_rules",
    "save_grid_rule",
    "delete_grid_rule",
    "list_advertisers",
    "save_advertiser",
    "delete_advertiser",
    "list_campaigns",
    "save_campaign",
    "delete_campaign",
    "preview_schedule",
    "get_play_log",
    "delivery_report",
];

pub fn tool_defs() -> Vec<ToolDef> {
    vec![
        def(
            "automation_status",
            "Get the long-term automation state: whether it is enabled, the period on air and recent warnings",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "set_automation_enabled",
            "Turn the long-term automation (playlist generator) on or off",
            json!({
                "type": "object",
                "properties": { "enabled": { "type": "boolean" } },
                "required": ["enabled"]
            }),
        ),
        def(
            "get_automation_config",
            "Get the station automation settings (lookahead, separation, imaging, ad breaks...)",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "set_automation_config",
            "Replace the station automation settings. Omitted fields take their defaults. min_items_ahead 1-50, min_minutes_ahead 1-240, keep_played at most 500, ad_break_seconds 10-1800.",
            json!({
                "type": "object",
                "properties": { "config": automation_config_schema() },
                "required": ["config"]
            }),
        ),
        def(
            "list_categories",
            "List all categories (music styles, imaging, spots...) with their ids, kinds, parents and filters",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "save_category",
            "Create a category (id 0) or update an existing one. Returns the category id. Item kinds: Music, Jingle, Bumper, StationId, Sweeper, Promo, Spot, Program, Bed, Other.",
            json!({
                "type": "object",
                "properties": { "category": category_schema() },
                "required": ["category"]
            }),
        ),
        def(
            "delete_category",
            "Delete a category by id",
            json!({
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"]
            }),
        ),
        def(
            "assign_tracks_to_category",
            "Assign up to 10000 library tracks to a category (or remove them with remove=true), with an optional rotation weight",
            json!({
                "type": "object",
                "properties": {
                    "category_id": { "type": "integer" },
                    "track_ids": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "maxItems": 10000
                    },
                    "weight": { "type": "number", "minimum": 0, "description": "Rotation weight, default 1.0" },
                    "remove": { "type": "boolean", "description": "Remove the tracks from the category instead of adding them" }
                },
                "required": ["category_id", "track_ids"]
            }),
        ),
        def(
            "set_track_codes",
            "Replace the codes of a library track (lowercase tags used by category slots)",
            json!({
                "type": "object",
                "properties": {
                    "track_id": { "type": "integer" },
                    "codes": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["track_id", "codes"]
            }),
        ),
        def(
            "set_track_enabled",
            "Enable or disable a library track for automation",
            json!({
                "type": "object",
                "properties": {
                    "track_id": { "type": "integer" },
                    "enabled": { "type": "boolean" }
                },
                "required": ["track_id", "enabled"]
            }),
        ),
        def(
            "list_codes",
            "List every track code in use",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "list_category_rules",
            "List the rules that assign scanned tracks to categories automatically",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "add_category_rule",
            "Add a rule that assigns tracks to a category by genre, artist or folder (case and accents ignored). Returns the rule id.",
            json!({
                "type": "object",
                "properties": { "rule": category_rule_schema() },
                "required": ["rule"]
            }),
        ),
        def(
            "delete_category_rule",
            "Delete a category rule by id",
            json!({
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"]
            }),
        ),
        def(
            "apply_category_rules",
            "Apply every category rule to the tracks already in the library and return how many assignments were added",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "list_clocks",
            "List the format clocks (what each hour sounds like)",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "save_clock",
            "Create or update a format clock. duration_minutes 5-1440, anchors must fall inside the clock and referenced categories must exist. Slot items are externally tagged, e.g. {\"Category\": {\"category_id\": 1, \"count\": 1}}, {\"Imaging\": {\"category_id\": 3}}, {\"AdBreak\": {\"name\": \"Break :15\", \"max_seconds\": 180}}, {\"Fill\": {\"category_id\": 1}}, {\"Playlist\": {\"playlist_id\": \"Morning\"}} or {\"Track\": {\"path\": \"/music/theme.mp3\"}}.",
            json!({
                "type": "object",
                "properties": { "clock": clock_schema() },
                "required": ["clock"]
            }),
        ),
        def(
            "delete_clock",
            "Delete a format clock by id",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }),
        ),
        def(
            "list_grid_rules",
            "List the dayparts grid rules (which clock or playlist plays when)",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "save_grid_rule",
            "Create or update a dayparts grid rule. target is {\"Clock\": {\"clock_id\": \"...\"}} or {\"Playlist\": {\"playlist_id\": \"...\"}}. weekdays bit 0 = Monday ... bit 6 = Sunday; months bit 0 = January ... bit 11 = December; weeks_of_month bits 0-4 = first to fifth week, bit 5 = last week. The target clock must exist or the target playlist must load, and date_from must not be after date_to.",
            json!({
                "type": "object",
                "properties": { "rule": grid_rule_schema() },
                "required": ["rule"]
            }),
        ),
        def(
            "delete_grid_rule",
            "Delete a dayparts grid rule by id",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }),
        ),
        def(
            "list_advertisers",
            "List the advertisers (companies that buy airtime)",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "save_advertiser",
            "Create an advertiser (id 0) or update an existing one. Returns the advertiser id.",
            json!({
                "type": "object",
                "properties": { "advertiser": advertiser_schema() },
                "required": ["advertiser"]
            }),
        ),
        def(
            "delete_advertiser",
            "Delete an advertiser and their campaigns by id",
            json!({
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"]
            }),
        ),
        def(
            "list_campaigns",
            "List advertising campaigns with their spots, dayparts, dates and sponsorship",
            json!({ "type": "object", "properties": {} }),
        ),
        def(
            "save_campaign",
            "Create a campaign (id 0) or update an existing one. Returns the campaign id. Dates are YYYY-MM-DD; spots reuse library track ids.",
            json!({
                "type": "object",
                "properties": { "campaign": campaign_schema() },
                "required": ["campaign"]
            }),
        ),
        def(
            "delete_campaign",
            "Delete an advertising campaign by id",
            json!({
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"]
            }),
        ),
        def(
            "preview_schedule",
            "Preview what the automation would put on air. from is YYYY-MM-DDTHH:MM:SS (default now), hours defaults to 1 (maximum 48). Returns at most 300 compact items.",
            json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Start, YYYY-MM-DDTHH:MM:SS (default: now)" },
                    "hours": { "type": "integer", "minimum": 1, "maximum": MAX_PREVIEW_HOURS, "description": "Hours to preview, default 1" }
                }
            }),
        ),
        def(
            "get_play_log",
            "Get the proof-of-play log. from and to are YYYY-MM-DDTHH:MM:SS; defaults are the last 24 hours. limit defaults to 100 and is at most 1000.",
            json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Start, YYYY-MM-DDTHH:MM:SS (default: 24 hours before to)" },
                    "to": { "type": "string", "description": "End, YYYY-MM-DDTHH:MM:SS (default: now)" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Maximum entries, default 100" }
                }
            }),
        ),
        def(
            "delivery_report",
            "Get aired spots per campaign and day for billing. Dates are YYYY-MM-DD, default today, and the span may be at most 366 days.",
            json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "First day, YYYY-MM-DD (default: today)" },
                    "to": { "type": "string", "description": "Last day, YYYY-MM-DD (default: today)" }
                }
            }),
        ),
    ]
}

fn def(name: &'static str, description: &'static str, input_schema: Value) -> ToolDef {
    ToolDef { name, description, input_schema }
}

fn item_kinds() -> Value {
    json!(["Music", "Jingle", "Bumper", "StationId", "Sweeper", "Promo", "Spot", "Program", "Bed", "Other"])
}

fn separation_schema() -> Value {
    json!({
        "type": "object",
        "description": "Minimum minutes between repeats; zero disables a rule",
        "properties": {
            "song_minutes": { "type": "integer", "minimum": 0 },
            "artist_minutes": { "type": "integer", "minimum": 0 },
            "album_minutes": { "type": "integer", "minimum": 0 },
            "horizontal": { "type": "boolean", "description": "Avoid the hour a track played in the day before" },
            "imaging_others": { "type": "integer", "minimum": 0 }
        }
    })
}

fn automation_config_schema() -> Value {
    json!({
        "type": "object",
        "description": "Full station automation settings; omitted fields take their defaults",
        "properties": {
            "enabled": { "type": "boolean" },
            "default_clock": { "type": ["string", "null"], "description": "Clock id for hours no grid rule covers" },
            "fallback_category": { "type": ["integer", "null"], "description": "Category id used when another category has nothing playable" },
            "separation": separation_schema(),
            "imaging": {
                "type": "object",
                "properties": {
                    "jingle_category": { "type": ["integer", "null"] },
                    "jingle_every": { "type": "integer", "minimum": 0 },
                    "sweeper_on_change": { "type": ["integer", "null"] }
                }
            },
            "min_items_ahead": { "type": "integer", "minimum": 1, "maximum": 50 },
            "min_minutes_ahead": { "type": "integer", "minimum": 1, "maximum": 240 },
            "keep_played": { "type": "integer", "minimum": 0, "maximum": 500 },
            "ad_break_seconds": { "type": "integer", "minimum": 10, "maximum": 1800 },
            "no_overlap_kinds": { "type": "array", "items": { "type": "string", "enum": item_kinds() } }
        }
    })
}

fn category_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer", "description": "0 to create a new category; an existing id to update it" },
            "name": { "type": "string" },
            "kind": { "type": "string", "enum": item_kinds() },
            "color": { "type": "string", "description": "UI colour, e.g. #e91e63" },
            "parent_id": { "type": ["integer", "null"], "description": "Parent category id; picking from the parent also picks from this one" },
            "filter": {
                "type": ["object", "null"],
                "description": "Smart category: match tracks by these fields instead of assigned ones",
                "properties": {
                    "genre": { "type": "string" },
                    "artist": { "type": "string" },
                    "folder": { "type": "string", "description": "File path starts with this folder" },
                    "year_from": { "type": "integer" },
                    "year_to": { "type": "integer" },
                    "energy_min": { "type": "integer" },
                    "energy_max": { "type": "integer" },
                    "min_duration_ms": { "type": "integer" },
                    "max_duration_ms": { "type": "integer" }
                }
            },
            "separation": separation_schema()
        },
        "required": ["id", "name", "kind"]
    })
}

fn category_rule_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer", "description": "Ignored when adding; 0 is fine" },
            "category_id": { "type": "integer" },
            "field": { "type": "string", "enum": ["Genre", "Artist", "Folder"] },
            "pattern": { "type": "string", "description": "Compared ignoring case and accents: equal for Genre/Artist, prefix for Folder" }
        },
        "required": ["category_id", "field", "pattern"]
    })
}

fn anchor_schema() -> Value {
    json!({
        "type": "object",
        "description": "Where a slot should start inside the clock",
        "properties": {
            "minute": { "type": "integer", "minimum": 0, "description": "Minute inside the clock (0-based)" },
            "second": { "type": "integer", "minimum": 0, "maximum": 59 },
            "hard": { "type": "boolean", "description": "Cut what plays at that time; soft starts after the current item" },
            "max_wait_seconds": { "type": "integer", "minimum": 0, "description": "Soft anchors: how late they may start (default 120)" }
        },
        "required": ["minute"]
    })
}

fn slot_item_schema() -> Value {
    json!({
        "type": "object",
        "description": "One slot item, externally tagged, e.g. {\"Category\": {\"category_id\": 1, \"count\": 1}}, {\"Imaging\": {\"category_id\": 3}}, {\"AdBreak\": {\"name\": \"Break :15\", \"max_seconds\": 180}}, {\"Fill\": {\"category_id\": 1}}, {\"Playlist\": {\"playlist_id\": \"Morning\"}} or {\"Track\": {\"path\": \"/music/theme.mp3\"}}",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "Category": {
                        "type": "object",
                        "properties": {
                            "category_id": { "type": "integer" },
                            "count": { "type": "integer", "minimum": 1, "description": "How many tracks, default 1" },
                            "require_codes": { "type": "array", "items": { "type": "string" } },
                            "exclude_codes": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["category_id"]
                    }
                },
                "required": ["Category"]
            },
            {
                "type": "object",
                "properties": {
                    "Imaging": {
                        "type": "object",
                        "properties": { "category_id": { "type": "integer" } },
                        "required": ["category_id"]
                    }
                },
                "required": ["Imaging"]
            },
            {
                "type": "object",
                "properties": {
                    "AdBreak": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "max_seconds": { "type": "integer", "minimum": 0, "description": "0 uses the station default" },
                            "intro_category": { "type": ["integer", "null"] },
                            "outro_category": { "type": ["integer", "null"] }
                        },
                        "required": ["name"]
                    }
                },
                "required": ["AdBreak"]
            },
            {
                "type": "object",
                "properties": {
                    "Fill": {
                        "type": "object",
                        "properties": { "category_id": { "type": "integer" } },
                        "required": ["category_id"]
                    }
                },
                "required": ["Fill"]
            },
            {
                "type": "object",
                "properties": {
                    "Playlist": {
                        "type": "object",
                        "properties": {
                            "playlist_id": { "type": "string" },
                            "count": { "type": ["integer", "null"], "description": "Items to take; omitted means all" }
                        },
                        "required": ["playlist_id"]
                    }
                },
                "required": ["Playlist"]
            },
            {
                "type": "object",
                "properties": {
                    "Track": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"]
                    }
                },
                "required": ["Track"]
            }
        ]
    })
}

fn clock_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Unique clock id" },
            "name": { "type": "string" },
            "color": { "type": "string" },
            "duration_minutes": { "type": "integer", "minimum": 5, "maximum": 1440, "description": "Clock length in minutes, default 60" },
            "slots": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "at": anchor_schema(),
                        "item": slot_item_schema()
                    },
                    "required": ["item"]
                }
            },
            "fill_category": { "type": ["integer", "null"], "description": "Category id used when the slots run out" }
        },
        "required": ["id", "name", "slots"]
    })
}

fn grid_target_schema() -> Value {
    json!({
        "type": "object",
        "description": "Externally tagged, e.g. {\"Clock\": {\"clock_id\": \"morning\"}} or {\"Playlist\": {\"playlist_id\": \"Morning\"}}",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "Clock": {
                        "type": "object",
                        "properties": { "clock_id": { "type": "string" } },
                        "required": ["clock_id"]
                    }
                },
                "required": ["Clock"]
            },
            {
                "type": "object",
                "properties": {
                    "Playlist": {
                        "type": "object",
                        "properties": { "playlist_id": { "type": "string" } },
                        "required": ["playlist_id"]
                    }
                },
                "required": ["Playlist"]
            }
        ]
    })
}

fn grid_rule_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "name": { "type": "string" },
            "priority": { "type": "integer", "description": "Higher wins; ties go to the rule covering less time" },
            "target": grid_target_schema(),
            "weekdays": { "type": "integer", "minimum": 0, "maximum": 127, "description": "Bitmask: bit 0 = Monday ... bit 6 = Sunday; default 127 (all days)" },
            "months": { "type": "integer", "minimum": 0, "maximum": 4095, "description": "Bitmask: bit 0 = January ... bit 11 = December; default 4095 (all months)" },
            "weeks_of_month": { "type": "integer", "minimum": 0, "maximum": 63, "description": "Bitmask: bits 0-4 = first to fifth week of the month (days 1-7, 8-14...), bit 5 = last week; default 63 (all)" },
            "start": { "type": "string", "description": "HH:MM:SS; end <= start crosses midnight, end == start is all day" },
            "end": { "type": "string", "description": "HH:MM:SS" },
            "date_from": { "type": ["string", "null"], "description": "First day, YYYY-MM-DD" },
            "date_to": { "type": ["string", "null"], "description": "Last day, YYYY-MM-DD" },
            "enabled": { "type": "boolean" }
        },
        "required": ["id", "target", "start", "end"]
    })
}

fn advertiser_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer", "description": "0 to create; an existing id to update" },
            "name": { "type": "string" },
            "contact": { "type": "string" },
            "notes": { "type": "string" }
        },
        "required": ["name"]
    })
}

fn sponsor_target_schema() -> Value {
    json!({
        "type": "object",
        "description": "Externally tagged, e.g. {\"Clock\": {\"clock_id\": \"morning\"}} or {\"GridRule\": {\"rule_id\": \"g1\"}}",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "Clock": {
                        "type": "object",
                        "properties": { "clock_id": { "type": "string" } },
                        "required": ["clock_id"]
                    }
                },
                "required": ["Clock"]
            },
            {
                "type": "object",
                "properties": {
                    "GridRule": {
                        "type": "object",
                        "properties": { "rule_id": { "type": "string" } },
                        "required": ["rule_id"]
                    }
                },
                "required": ["GridRule"]
            }
        ]
    })
}

fn campaign_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer", "description": "0 to create; an existing id to update" },
            "advertiser_id": { "type": "integer" },
            "name": { "type": "string" },
            "date_from": { "type": "string", "description": "First day it airs, YYYY-MM-DD" },
            "date_to": { "type": "string", "description": "Last day it airs, YYYY-MM-DD" },
            "plays_per_day": { "type": "integer", "minimum": 0, "description": "Spots a day in ad breaks (0 for a pure sponsorship)" },
            "max_per_hour": { "type": "integer", "minimum": 0, "description": "At most this many in one clock hour (0 = no limit)" },
            "dayparts": {
                "type": "array",
                "description": "When it may air; empty means any time",
                "items": {
                    "type": "object",
                    "properties": {
                        "weekdays": { "type": "integer", "description": "Bitmask: bit 0 = Monday ... bit 6 = Sunday; default 127 (all days)" },
                        "start": { "type": "string", "description": "HH:MM:SS; end <= start crosses midnight, end == start is all day" },
                        "end": { "type": "string", "description": "HH:MM:SS" }
                    },
                    "required": ["start", "end"]
                }
            },
            "priority": { "type": "integer", "description": "Higher goes first when breaks are full" },
            "sponsorship": {
                "type": "object",
                "properties": {
                    "target": sponsor_target_schema(),
                    "opening": { "type": "boolean" },
                    "closing": { "type": "boolean" }
                },
                "required": ["target"]
            },
            "enabled": { "type": "boolean" },
            "spots": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "track_id": { "type": "integer" },
                        "weight": { "type": "number", "minimum": 0, "description": "Rotation weight, default 1.0" }
                    },
                    "required": ["track_id"]
                }
            }
        },
        "required": ["advertiser_id", "name", "date_from", "date_to"]
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
        "automation_status" => {
            let status = state.automation.lock().map_err(|e| e.to_string())?.status();
            json_pretty(&status)
        }
        "set_automation_enabled" => {
            let enabled = args["enabled"].as_bool().ok_or("Missing enabled")?;
            {
                let mut library = state.library.lock().map_err(|e| e.to_string())?;
                let mut config = library.automation_config()?;
                config.enabled = enabled;
                library.save_automation_config(&config)?;
            }
            reload_automation(state)?;
            Ok(text_result(if enabled { "Automation enabled" } else { "Automation disabled" }))
        }
        "get_automation_config" => {
            let config = state.library.lock().map_err(|e| e.to_string())?.automation_config()?;
            json_pretty(&config)
        }
        "set_automation_config" => {
            let config: AutomationConfig = parse_arg(args, "config")?;
            validate_config(&config)?;
            {
                let mut library = state.library.lock().map_err(|e| e.to_string())?;
                library.save_automation_config(&config)?;
            }
            reload_automation(state)?;
            Ok(text_result("Automation configuration saved"))
        }
        "list_categories" => {
            let categories = state.library.lock().map_err(|e| e.to_string())?.list_categories()?;
            json_compact(&categories)
        }
        "save_category" => {
            let category: Category = parse_id_arg(args, "category")?;
            if category.name.trim().is_empty() {
                return Err("Category name is required".to_string());
            }
            let id = {
                let mut library = state.library.lock().map_err(|e| e.to_string())?;
                if category.id == 0 {
                    library.create_category(&category)?
                } else {
                    library.update_category(&category)?;
                    category.id
                }
            };
            reload_automation(state)?;
            json_compact(&json!({ "id": id }))
        }
        "delete_category" => {
            let id = args["id"].as_i64().ok_or("Missing id")?;
            let deleted = state.library.lock().map_err(|e| e.to_string())?.delete_category(id)?;
            if !deleted {
                return Err(format!("No category {id}"));
            }
            reload_automation(state)?;
            Ok(text_result("Category deleted"))
        }
        "assign_tracks_to_category" => {
            let category_id = args["category_id"].as_i64().ok_or("Missing category_id")?;
            let track_ids: Vec<i64> = serde_json::from_value(
                args.get("track_ids").cloned().ok_or("Missing track_ids")?,
            )
            .map_err(|e| format!("Invalid track_ids: {e}"))?;
            if track_ids.len() > 10_000 {
                return Err("At most 10000 track ids are allowed".to_string());
            }
            let weight = args["weight"].as_f64().unwrap_or(1.0) as f32;
            let remove = args["remove"].as_bool().unwrap_or(false);
            if !remove && !(0.01..=100.0).contains(&weight) {
                return Err("weight must be between 0.01 and 100".to_string());
            }
            let changed = {
                let mut library = state.library.lock().map_err(|e| e.to_string())?;
                if remove {
                    library.remove_tracks_from_category(category_id, &track_ids)?
                } else {
                    library.add_tracks_to_category(category_id, &track_ids, weight)?
                }
            };
            reload_automation(state)?;
            Ok(text_result(&format!(
                "{} {changed} track assignments for category {category_id}",
                if remove { "Removed" } else { "Updated" }
            )))
        }
        "set_track_codes" => {
            let track_id = args["track_id"].as_i64().ok_or("Missing track_id")?;
            let codes: Vec<String> =
                serde_json::from_value(args.get("codes").cloned().ok_or("Missing codes")?)
                    .map_err(|e| format!("Invalid codes: {e}"))?;
            state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .set_track_codes(track_id, &codes)?;
            reload_automation(state)?;
            Ok(text_result(&format!("Codes saved for track {track_id}")))
        }
        "set_track_enabled" => {
            let track_id = args["track_id"].as_i64().ok_or("Missing track_id")?;
            let enabled = args["enabled"].as_bool().ok_or("Missing enabled")?;
            state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .set_track_enabled(track_id, enabled)?;
            reload_automation(state)?;
            Ok(text_result(&format!(
                "Track {track_id} {}",
                if enabled { "enabled" } else { "disabled" }
            )))
        }
        "list_codes" => {
            let codes = state.library.lock().map_err(|e| e.to_string())?.list_codes()?;
            json_compact(&codes)
        }
        "list_category_rules" => {
            let rules = state.library.lock().map_err(|e| e.to_string())?.list_category_rules()?;
            json_compact(&rules)
        }
        "add_category_rule" => {
            let rule: CategoryRule = parse_id_arg(args, "rule")?;
            let id = state.library.lock().map_err(|e| e.to_string())?.add_category_rule(&rule)?;
            reload_automation(state)?;
            json_compact(&json!({ "id": id }))
        }
        "delete_category_rule" => {
            let id = args["id"].as_i64().ok_or("Missing id")?;
            let deleted = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .delete_category_rule(id)?;
            if !deleted {
                return Err(format!("No category rule {id}"));
            }
            reload_automation(state)?;
            Ok(text_result("Category rule deleted"))
        }
        "apply_category_rules" => {
            let added = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .apply_category_rules()?;
            reload_automation(state)?;
            Ok(text_result(&format!("{added} track assignments added")))
        }
        "list_clocks" => {
            let clocks = state.library.lock().map_err(|e| e.to_string())?.list_clocks()?;
            json_compact(&clocks)
        }
        "save_clock" => {
            let clock: Clock = parse_arg(args, "clock")?;
            {
                let mut library = state.library.lock().map_err(|e| e.to_string())?;
                validate_clock(&clock, &library)?;
                library.save_clock(&clock)?;
            }
            reload_automation(state)?;
            json_compact(&json!({ "id": clock.id }))
        }
        "delete_clock" => {
            let id = args["id"].as_str().ok_or("Missing id")?;
            let deleted = state.library.lock().map_err(|e| e.to_string())?.delete_clock(id)?;
            if !deleted {
                return Err(format!("No clock {id}"));
            }
            reload_automation(state)?;
            Ok(text_result("Clock deleted"))
        }
        "list_grid_rules" => {
            let rules = state.library.lock().map_err(|e| e.to_string())?.list_grid_rules()?;
            json_compact(&rules)
        }
        "save_grid_rule" => {
            let rule: GridRule = parse_arg(args, "rule")?;
            if rule.id.trim().is_empty() {
                return Err("Grid rule id is required".to_string());
            }
            if let (Some(from), Some(to)) = (rule.date_from, rule.date_to) {
                if from > to {
                    return Err("Grid rule start date is after its end date".to_string());
                }
            }
            {
                let mut library = state.library.lock().map_err(|e| e.to_string())?;
                match &rule.target {
                    GridTarget::Clock { clock_id } => {
                        if !library.list_clocks()?.iter().any(|clock| &clock.id == clock_id) {
                            return Err(format!("No clock {clock_id}"));
                        }
                    }
                    GridTarget::Playlist { playlist_id } => {
                        library.load_playlist(playlist_id).map_err(|e| {
                            format!("No playlist {playlist_id}: {e}")
                        })?;
                    }
                }
                library.save_grid_rule(&rule)?;
            }
            reload_automation(state)?;
            json_compact(&json!({ "id": rule.id }))
        }
        "delete_grid_rule" => {
            let id = args["id"].as_str().ok_or("Missing id")?;
            let deleted = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .delete_grid_rule(id)?;
            if !deleted {
                return Err(format!("No grid rule {id}"));
            }
            reload_automation(state)?;
            Ok(text_result("Grid rule deleted"))
        }
        "list_advertisers" => {
            let advertisers = state.library.lock().map_err(|e| e.to_string())?.list_advertisers()?;
            json_compact(&advertisers)
        }
        "save_advertiser" => {
            let advertiser: Advertiser = parse_id_arg(args, "advertiser")?;
            let id = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .save_advertiser(&advertiser)?;
            reload_automation(state)?;
            json_compact(&json!({ "id": id }))
        }
        "delete_advertiser" => {
            let id = args["id"].as_i64().ok_or("Missing id")?;
            let deleted = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .delete_advertiser(id)?;
            if !deleted {
                return Err(format!("No advertiser {id}"));
            }
            reload_automation(state)?;
            Ok(text_result("Advertiser deleted"))
        }
        "list_campaigns" => {
            let campaigns = state.library.lock().map_err(|e| e.to_string())?.list_campaigns()?;
            json_compact(&campaigns)
        }
        "save_campaign" => {
            let campaign: Campaign = parse_id_arg(args, "campaign")?;
            let id = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .save_campaign(&campaign)?;
            reload_automation(state)?;
            json_compact(&json!({ "id": id }))
        }
        "delete_campaign" => {
            let id = args["id"].as_i64().ok_or("Missing id")?;
            let deleted = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .delete_campaign(id)?;
            if !deleted {
                return Err(format!("No campaign {id}"));
            }
            reload_automation(state)?;
            Ok(text_result("Campaign deleted"))
        }
        "preview_schedule" => {
            let from = match args.get("from").and_then(Value::as_str) {
                Some(text) => parse_api_datetime(text)?,
                None => Local::now().naive_local(),
            };
            let hours = args.get("hours").and_then(Value::as_u64).unwrap_or(1);
            if !(1..=MAX_PREVIEW_HOURS as u64).contains(&hours) {
                return Err(format!("hours must be between 1 and {MAX_PREVIEW_HOURS}"));
            }
            let hours = hours as u32;
            let overlap = {
                let engine = state.engine.lock().map_err(|e| e.to_string())?;
                let fade = engine.crossfade_config();
                if fade.enabled { fade.duration.as_millis() as u64 } else { 0 }
            };
            let items = riars_core::automation::preview(state.library.as_ref(), from, hours, overlap)?;
            json_compact(&preview_items(items))
        }
        "get_play_log" => {
            let now = Local::now().naive_local();
            let to = opt_datetime(args, "to")?.unwrap_or(now);
            let from = opt_datetime(args, "from")?.unwrap_or(to - Duration::hours(24));
            if to < from {
                return Err("get_play_log end is before its start".to_string());
            }
            if (to - from).num_days() > 366 {
                return Err("get_play_log span is at most 366 days".to_string());
            }
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(100)
                .min(1000) as usize;
            let entries = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .play_log(from, to, limit)?;
            let items: Vec<Value> = entries
                .into_iter()
                .map(|entry| {
                    json!({
                        "aired_at": entry.aired_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                        "kind": serde_json::to_value(entry.kind).unwrap_or(Value::Null),
                        "title": entry.title,
                        "artist": entry.artist,
                        "played_s": entry.played_ms / 1000,
                        "complete": entry.complete,
                        "campaign_id": entry.campaign_id,
                    })
                })
                .collect();
            json_compact(&items)
        }
        "delivery_report" => {
            let today = Local::now().date_naive();
            let from = opt_date(args, "from")?.unwrap_or(today);
            let to = opt_date(args, "to")?.unwrap_or(today);
            if to < from {
                return Err("delivery_report end date is before its start date".to_string());
            }
            if (to - from).num_days() > 365 {
                return Err("delivery_report span is at most 366 days".to_string());
            }
            let rows = state
                .library
                .lock()
                .map_err(|e| e.to_string())?
                .delivery_report(from, to)?;
            json_compact(&rows)
        }
        _ => Err(format!("Unknown automation tool: {name}")),
    }
}

fn reload_automation(state: &McpState) -> Result<(), String> {
    state.automation.lock().map_err(|e| e.to_string())?.invalidate();
    Ok(())
}

fn validate_config(config: &AutomationConfig) -> Result<(), String> {
    if !(1..=50).contains(&config.min_items_ahead) {
        return Err("min_items_ahead must be between 1 and 50".to_string());
    }
    if !(1..=240).contains(&config.min_minutes_ahead) {
        return Err("min_minutes_ahead must be between 1 and 240".to_string());
    }
    if config.keep_played > 500 {
        return Err("keep_played must be at most 500".to_string());
    }
    if !(10..=1800).contains(&config.ad_break_seconds) {
        return Err("ad_break_seconds must be between 10 and 1800".to_string());
    }
    Ok(())
}

fn validate_clock(clock: &Clock, library: &MediaLibrary) -> Result<(), String> {
    if clock.id.trim().is_empty() {
        return Err("Clock id is required".to_string());
    }
    if clock.name.trim().is_empty() {
        return Err("Clock name is required".to_string());
    }
    if !(5..=1440).contains(&clock.duration_minutes) {
        return Err("Clock duration_minutes must be between 5 and 1440".to_string());
    }
    let clock_seconds = clock.duration_minutes as i64 * 60;
    let categories: HashSet<i64> = library
        .list_categories()?
        .into_iter()
        .map(|category| category.id)
        .collect();
    let mut referenced = Vec::new();
    if let Some(id) = clock.fill_category {
        referenced.push(id);
    }
    for slot in &clock.slots {
        if let Some(anchor) = slot.at {
            if anchor.second >= 60 {
                return Err("Clock anchor second must be below 60".to_string());
            }
            if anchor.offset_seconds() >= clock_seconds {
                return Err(format!(
                    "Clock anchor {}:{} falls outside the clock",
                    anchor.minute, anchor.second
                ));
            }
        }
        match &slot.item {
            SlotItem::Category { category_id, .. }
            | SlotItem::Imaging { category_id }
            | SlotItem::Fill { category_id } => referenced.push(*category_id),
            SlotItem::AdBreak { intro_category, outro_category, .. } => {
                referenced.extend(intro_category.iter().chain(outro_category.iter()).copied());
            }
            SlotItem::Playlist { playlist_id, .. } => {
                let playlist = library
                    .load_playlist(playlist_id)
                    .map_err(|e| format!("No playlist {playlist_id}: {e}"))?;
                if playlist.items.is_empty() {
                    return Err(format!("Playlist {playlist_id} is empty"));
                }
            }
            SlotItem::Track { path } => {
                if !std::path::Path::new(path).is_file() {
                    return Err(format!("No track file {path}"));
                }
            }
        }
    }
    for id in referenced {
        if !categories.contains(&id) {
            return Err(format!("No category {id}"));
        }
    }
    Ok(())
}

fn preview_items(items: Vec<PlaylistItem>) -> Vec<Value> {
    let first_date = items
        .first()
        .and_then(|item| item.planned_at.as_ref().map(|at| at.date()));
    items
        .into_iter()
        .take(300)
        .map(|item| {
            let planned_at = match item.planned_at {
                Some(at) if Some(at.date()) == first_date => at.format("%H:%M:%S").to_string(),
                Some(at) => at.format("%Y-%m-%d %H:%M:%S").to_string(),
                None => String::new(),
            };
            json!({
                "planned_at": planned_at,
                "kind": serde_json::to_value(item.kind).unwrap_or(Value::Null),
                "title": item.title,
                "artist": item.artist,
                "duration_s": item.duration_ms / 1000,
                "category_id": item.category_id,
                "campaign_id": item.campaign_id,
            })
        })
        .collect()
}

fn json_pretty<T: Serialize>(value: &T) -> Result<Value, String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    Ok(text_result(&text))
}

fn json_compact<T: Serialize>(value: &T) -> Result<Value, String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    Ok(text_result(&text))
}

fn arg_object(args: &Value, key: &str) -> Result<Value, String> {
    args.get(key)
        .cloned()
        .filter(Value::is_object)
        .ok_or_else(|| format!("Missing {key} object"))
}

fn parse_arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(arg_object(args, key)?).map_err(|e| format!("Invalid {key}: {e}"))
}

fn parse_id_arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    let mut value = arg_object(args, key)?;
    if value.get("id").is_none() {
        value["id"] = json!(0);
    }
    serde_json::from_value(value).map_err(|e| format!("Invalid {key}: {e}"))
}

fn opt_datetime(args: &Value, key: &str) -> Result<Option<NaiveDateTime>, String> {
    match args.get(key).and_then(Value::as_str) {
        Some(text) => Ok(Some(parse_api_datetime(text)?)),
        None => Ok(None),
    }
}

fn opt_date(args: &Value, key: &str) -> Result<Option<NaiveDate>, String> {
    match args.get(key).and_then(Value::as_str) {
        Some(text) => parse_api_date(text).map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use super::*;
    use riars_core::automation::Automation;
    use riars_core::session::SessionSaver;
    use riars_core::{AudioEngine, Scheduler};

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
            "riars-mcp-automation-{}-{name}.db",
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
        handle(name, &args, state).unwrap_or_else(|| panic!("{name} is not an automation tool"))
    }

    fn text_of(result: &Value) -> String {
        result["content"][0]["text"].as_str().unwrap_or_default().to_string()
    }

    fn parse_text(result: &Value) -> Value {
        serde_json::from_str(&text_of(result)).unwrap()
    }

    fn insert_tracks(state: &McpState, dir: &Path, count: usize) {
        std::fs::create_dir_all(dir).unwrap();
        for i in 0..count {
            std::fs::write(dir.join(format!("track{i}.mp3")), b"not really audio").unwrap();
        }
        let added = state
            .library
            .lock()
            .unwrap()
            .scan_folder(&dir.to_string_lossy())
            .unwrap();
        assert_eq!(added, count);
    }

    fn track_ids(state: &McpState) -> Vec<i64> {
        state
            .library
            .lock()
            .unwrap()
            .get_all_tracks()
            .unwrap()
            .into_iter()
            .map(|track| track.id)
            .collect()
    }

    #[test]
    fn tool_names_are_unique_and_do_not_clash() {
        let defs = tool_defs();
        let mut own = HashSet::new();
        for tool in &defs {
            assert!(own.insert(tool.name), "duplicate automation tool {}", tool.name);
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
    fn save_and_list_category() {
        let (state, path) = test_state("category");

        let saved = call(
            &state,
            "save_category",
            json!({ "category": { "id": 0, "name": "Pop", "kind": "Music" } }),
        )
        .unwrap();
        let id = parse_text(&saved)["id"].as_i64().unwrap();

        let listed = call(&state, "list_categories", json!({})).unwrap();
        let categories = parse_text(&listed);
        let categories = categories.as_array().unwrap();
        assert!(categories.iter().any(|category| {
            category["id"].as_i64() == Some(id) && category["name"] == "Pop"
        }));

        cleanup(state, &path);
    }

    #[test]
    fn save_clock_rejects_missing_category() {
        let (state, path) = test_state("clock-category");

        let result = call(
            &state,
            "save_clock",
            json!({
                "clock": {
                    "id": "c1",
                    "name": "Broken",
                    "duration_minutes": 60,
                    "slots": [{ "item": { "Fill": { "category_id": 9999 } } }]
                }
            }),
        );
        assert!(result.is_err());

        cleanup(state, &path);
    }

    #[test]
    fn save_clock_rejects_missing_playlist_track_and_bad_anchor() {
        let (state, path) = test_state("clock-files");

        let missing_playlist = call(
            &state,
            "save_clock",
            json!({
                "clock": {
                    "id": "c1",
                    "name": "Broken",
                    "duration_minutes": 60,
                    "slots": [{
                        "item": { "Playlist": { "playlist_id": "missing" } }
                    }]
                }
            }),
        );
        assert!(missing_playlist.is_err());

        let missing_file = std::env::temp_dir()
            .join(format!("riars-mcp-automation-{}-missing.mp3", std::process::id()));
        let _ = std::fs::remove_file(&missing_file);
        let missing_track = call(
            &state,
            "save_clock",
            json!({
                "clock": {
                    "id": "c1",
                    "name": "Broken",
                    "duration_minutes": 60,
                    "slots": [{
                        "item": { "Track": { "path": missing_file.to_string_lossy().into_owned() } }
                    }]
                }
            }),
        );
        assert!(missing_track.is_err());

        let bad_anchor = call(
            &state,
            "save_clock",
            json!({
                "clock": {
                    "id": "c1",
                    "name": "Broken",
                    "duration_minutes": 60,
                    "slots": [{
                        "at": { "minute": 1, "second": 60 },
                        "item": { "Fill": { "category_id": 1 } }
                    }]
                }
            }),
        );
        assert!(bad_anchor.is_err());

        cleanup(state, &path);
    }

    #[test]
    fn api_date_helpers_reject_out_of_range_values() {
        let parsed =
            opt_datetime(&json!({ "from": "2026-01-01 00:00:00" }), "from").unwrap().unwrap();
        assert_eq!(parsed, parse_api_datetime("2026-01-01T00:00:00").unwrap());
        assert!(opt_datetime(&json!({ "to": "-262143-01-01T00:00:00" }), "to").is_err());
        assert!(opt_date(&json!({ "from": "1900-01-01" }), "from").is_err());
    }

    #[test]
    fn ranges_reject_backwards_and_long_spans() {
        let (state, path) = test_state("spans");

        let backwards = call(
            &state,
            "get_play_log",
            json!({ "from": "2024-01-02T00:00:00", "to": "2024-01-01T00:00:00" }),
        )
        .unwrap_err();
        assert!(backwards.contains("before its start"));

        let long_log = call(
            &state,
            "get_play_log",
            json!({ "from": "2024-01-01T00:00:00", "to": "2026-01-01T00:00:00" }),
        )
        .unwrap_err();
        assert!(long_log.contains("366"));

        let long_delivery = call(
            &state,
            "delivery_report",
            json!({ "from": "2024-01-01", "to": "2026-01-01" }),
        )
        .unwrap_err();
        assert!(long_delivery.contains("366"));

        cleanup(state, &path);
    }

    #[test]
    fn preview_rejects_hours_outside_the_range() {
        let (state, path) = test_state("preview-hours");
        assert!(call(&state, "preview_schedule", json!({ "hours": 0 })).is_err());
        assert!(
            call(&state, "preview_schedule", json!({ "hours": MAX_PREVIEW_HOURS + 1 }))
                .is_err()
        );
        assert!(
            call(
                &state,
                "preview_schedule",
                json!({ "from": "1900-01-01T00:00:00", "hours": 1 })
            )
            .is_err()
        );
        cleanup(state, &path);
    }

    #[test]
    fn category_weight_is_checked_only_when_adding() {
        let (state, path) = test_state("weight");

        let err = call(
            &state,
            "assign_tracks_to_category",
            json!({ "category_id": 1, "track_ids": [1], "weight": 0 }),
        )
        .unwrap_err();
        assert!(err.contains("weight"));

        call(
            &state,
            "assign_tracks_to_category",
            json!({ "category_id": 1, "track_ids": [1], "weight": 0, "remove": true }),
        )
        .unwrap();

        cleanup(state, &path);
    }

    #[test]
    fn set_automation_enabled_is_saved() {
        let (state, path) = test_state("enabled");

        call(&state, "set_automation_enabled", json!({ "enabled": true })).unwrap();

        let config = parse_text(&call(&state, "get_automation_config", json!({})).unwrap());
        assert_eq!(config["enabled"].as_bool(), Some(true));

        let status = parse_text(&call(&state, "automation_status", json!({})).unwrap());
        assert!(status.get("enabled").is_some());

        cleanup(state, &path);
    }

    #[test]
    fn preview_schedule_returns_compact_items() {
        let (state, path) = test_state("preview");
        let dir = std::env::temp_dir().join(format!(
            "riars-mcp-automation-media-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        insert_tracks(&state, &dir, 3);

        let category = parse_text(
            &call(
                &state,
                "save_category",
                json!({ "category": { "id": 0, "name": "Music", "kind": "Music" } }),
            )
            .unwrap(),
        );
        let category_id = category["id"].as_i64().unwrap();

        call(
            &state,
            "assign_tracks_to_category",
            json!({ "category_id": category_id, "track_ids": track_ids(&state) }),
        )
        .unwrap();

        call(
            &state,
            "save_clock",
            json!({
                "clock": {
                    "id": "c1",
                    "name": "All day",
                    "duration_minutes": 60,
                    "slots": [{ "item": { "Fill": { "category_id": category_id } } }]
                }
            }),
        )
        .unwrap();

        call(
            &state,
            "set_automation_config",
            json!({ "config": { "enabled": true, "default_clock": "c1" } }),
        )
        .unwrap();

        let items = parse_text(
            &call(
                &state,
                "preview_schedule",
                json!({ "from": "2024-01-01T00:00:00", "hours": 1 }),
            )
            .unwrap(),
        );
        let items = items.as_array().unwrap();
        assert!(!items.is_empty());
        assert!(items.len() <= 300);
        for item in items {
            for key in [
                "planned_at",
                "kind",
                "title",
                "artist",
                "duration_s",
                "category_id",
                "campaign_id",
            ] {
                assert!(item.get(key).is_some(), "preview item is missing {key}");
            }
            assert!(item["planned_at"].as_str().unwrap().len() >= 8);
            assert!(item["kind"].is_string());
            assert!(item["duration_s"].is_number());
            assert!(item.get("track_path").is_none());
        }

        let _ = std::fs::remove_dir_all(&dir);
        cleanup(state, &path);
    }

    #[test]
    fn advertiser_and_campaign_round_trip() {
        let (state, path) = test_state("traffic");

        let advertiser = parse_text(
            &call(
                &state,
                "save_advertiser",
                json!({ "advertiser": { "id": 0, "name": "Acme", "contact": "Ana" } }),
            )
            .unwrap(),
        );
        let advertiser_id = advertiser["id"].as_i64().unwrap();

        let campaign = parse_text(
            &call(
                &state,
                "save_campaign",
                json!({
                    "campaign": {
                        "id": 0,
                        "advertiser_id": advertiser_id,
                        "name": "Summer",
                        "date_from": "2024-06-01",
                        "date_to": "2024-06-30",
                        "plays_per_day": 2
                    }
                }),
            )
            .unwrap(),
        );
        let campaign_id = campaign["id"].as_i64().unwrap();

        let advertisers = parse_text(&call(&state, "list_advertisers", json!({})).unwrap());
        assert!(advertisers.as_array().unwrap().iter().any(|advertiser| {
            advertiser["id"].as_i64() == Some(advertiser_id) && advertiser["name"] == "Acme"
        }));

        let campaigns = parse_text(&call(&state, "list_campaigns", json!({})).unwrap());
        assert!(campaigns.as_array().unwrap().iter().any(|campaign| {
            campaign["id"].as_i64() == Some(campaign_id)
                && campaign["name"] == "Summer"
                && campaign["advertiser_id"].as_i64() == Some(advertiser_id)
        }));

        cleanup(state, &path);
    }
}
