//! Tauri commands for long-term automation. They mirror the `automation` object in
//! `src/automation/api.ts`.

use riars_core::automation::model::{
    Advertiser, AutomationConfig, Campaign, Category, CategoryRule, Clock, DeliveryRow, GridRule,
    PlayLogEntry,
};
use riars_core::automation::runner::AutomationStatus;
use riars_core::automation::validate;
use riars_core::playlist::playlist::PlaylistItem;
use tauri::{AppHandle, Manager, State};

use crate::AppState;

/// Drop the library guard first, then make the generator read the new data on its
/// next tick. Never hold the automation lock together with the library or engine.
fn invalidate(state: &AppState) -> Result<(), String> {
    state.automation.lock().map_err(|e| e.to_string())?.invalidate();
    Ok(())
}

#[tauri::command]
pub fn automation_status(state: State<AppState>) -> Result<AutomationStatus, String> {
    Ok(state.automation.lock().map_err(|e| e.to_string())?.status())
}

#[tauri::command]
pub fn automation_get_config(state: State<AppState>) -> Result<AutomationConfig, String> {
    state.library.lock().map_err(|e| e.to_string())?.automation_config()
}

#[tauri::command]
pub fn automation_save_config(
    config: AutomationConfig,
    state: State<AppState>,
) -> Result<AutomationConfig, String> {
    validate::validate_config(&config)?;
    {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.save_automation_config(&config)?;
    }
    invalidate(&state)?;
    state.library.lock().map_err(|e| e.to_string())?.automation_config()
}

#[tauri::command]
pub fn automation_set_enabled(
    enabled: bool,
    state: State<AppState>,
) -> Result<AutomationConfig, String> {
    let config = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        let mut config = library.automation_config()?;
        config.enabled = enabled;
        library.save_automation_config(&config)?;
        config
    };
    invalidate(&state)?;
    Ok(config)
}

#[tauri::command]
pub fn automation_categories(state: State<AppState>) -> Result<Vec<Category>, String> {
    state.library.lock().map_err(|e| e.to_string())?.list_categories()
}

#[tauri::command]
pub fn automation_save_category(
    category: Category,
    state: State<AppState>,
) -> Result<Category, String> {
    validate::validate_category(&category)?;
    let id = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        if category.id == 0 {
            library.create_category(&category)?
        } else {
            library.update_category(&category)?;
            category.id
        }
    };
    invalidate(&state)?;
    let saved = state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .list_categories()?
        .into_iter()
        .find(|saved| saved.id == id)
        .unwrap_or(Category { id, ..category });
    Ok(saved)
}

#[tauri::command]
pub fn automation_delete_category(id: i64, state: State<AppState>) -> Result<(), String> {
    let deleted = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.delete_category(id)?
    };
    if !deleted {
        return Err(format!("No category {id}"));
    }
    invalidate(&state)?;
    Ok(())
}

#[tauri::command]
pub fn automation_category_tracks(
    category_id: i64,
    track_ids: Vec<i64>,
    weight: f32,
    remove: bool,
    state: State<AppState>,
) -> Result<usize, String> {
    if track_ids.len() > 10_000 {
        return Err("at most 10000 track ids per call".to_string());
    }
    validate::validate_weight(weight, remove)?;
    let count = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        if remove {
            library.remove_tracks_from_category(category_id, &track_ids)?
        } else {
            library.add_tracks_to_category(category_id, &track_ids, weight)?
        }
    };
    invalidate(&state)?;
    Ok(count)
}

#[tauri::command]
pub fn automation_track_categories(
    track_id: i64,
    state: State<AppState>,
) -> Result<Vec<(i64, f32)>, String> {
    state.library.lock().map_err(|e| e.to_string())?.track_categories(track_id)
}

#[tauri::command]
pub fn automation_set_track_codes(
    track_id: i64,
    codes: Vec<String>,
    state: State<AppState>,
) -> Result<Vec<String>, String> {
    let codes: Vec<String> = codes
        .iter()
        .map(|code| code.trim().to_lowercase())
        .filter(|code| !code.is_empty())
        .collect();
    {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.set_track_codes(track_id, &codes)?;
    }
    invalidate(&state)?;
    Ok(codes)
}

#[tauri::command]
pub fn automation_set_track_enabled(
    track_id: i64,
    enabled: bool,
    state: State<AppState>,
) -> Result<(), String> {
    {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.set_track_enabled(track_id, enabled)?;
    }
    invalidate(&state)?;
    Ok(())
}

/// A page of the tracks of a category (assigned ones, or those its filter matches),
/// sorted by artist and title.
#[derive(serde::Serialize)]
pub struct CategoryMembers {
    total: usize,
    tracks: Vec<riars_core::automation::model::CandidateRow>,
}

#[tauri::command]
pub fn automation_category_members(
    category_id: i64,
    limit: usize,
    offset: usize,
    state: State<AppState>,
) -> Result<CategoryMembers, String> {
    let rows = state.library.lock().map_err(|e| e.to_string())?.category_candidates(category_id)?;
    let total = rows.len();
    let tracks = rows.into_iter().skip(offset).take(limit.clamp(1, 500)).collect();
    Ok(CategoryMembers { total, tracks })
}

/// Library tracks by id (unknown ids are left out), e.g. the spots of a campaign.
#[tauri::command]
pub fn automation_tracks(ids: Vec<i64>, state: State<AppState>) -> Result<Vec<riars_core::Track>, String> {
    if ids.len() > 1000 {
        return Err("At most 1000 ids".to_string());
    }
    let library = state.library.lock().map_err(|e| e.to_string())?;
    let mut tracks = Vec::with_capacity(ids.len());
    for id in ids {
        tracks.extend(library.get_track(id)?);
    }
    Ok(tracks)
}

#[tauri::command]
pub fn automation_codes(state: State<AppState>) -> Result<Vec<String>, String> {
    state.library.lock().map_err(|e| e.to_string())?.list_codes()
}

#[tauri::command]
pub fn automation_rules(state: State<AppState>) -> Result<Vec<CategoryRule>, String> {
    state.library.lock().map_err(|e| e.to_string())?.list_category_rules()
}

#[tauri::command]
pub fn automation_save_rule(
    mut rule: CategoryRule,
    state: State<AppState>,
) -> Result<i64, String> {
    rule.id = 0;
    let id = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.add_category_rule(&rule)?
    };
    invalidate(&state)?;
    Ok(id)
}

#[tauri::command]
pub fn automation_delete_rule(id: i64, state: State<AppState>) -> Result<(), String> {
    let deleted = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.delete_category_rule(id)?
    };
    if !deleted {
        return Err(format!("No category rule {id}"));
    }
    invalidate(&state)?;
    Ok(())
}

#[tauri::command]
pub fn automation_apply_rules(state: State<AppState>) -> Result<usize, String> {
    let count = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.apply_category_rules()?
    };
    invalidate(&state)?;
    Ok(count)
}

#[tauri::command]
pub fn automation_clocks(state: State<AppState>) -> Result<Vec<Clock>, String> {
    state.library.lock().map_err(|e| e.to_string())?.list_clocks()
}

#[tauri::command]
pub fn automation_save_clock(clock: Clock, state: State<AppState>) -> Result<Clock, String> {
    let saved = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        validate::validate_clock(&clock, &library)?;
        library.save_clock(&clock)?;
        clock
    };
    invalidate(&state)?;
    let saved = state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .list_clocks()?
        .into_iter()
        .find(|clock| clock.id == saved.id)
        .unwrap_or(saved);
    Ok(saved)
}

#[tauri::command]
pub fn automation_delete_clock(id: String, state: State<AppState>) -> Result<(), String> {
    let deleted = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.delete_clock(&id)?
    };
    if !deleted {
        return Err(format!("No clock {id}"));
    }
    invalidate(&state)?;
    Ok(())
}

#[tauri::command]
pub fn automation_grid(state: State<AppState>) -> Result<Vec<GridRule>, String> {
    state.library.lock().map_err(|e| e.to_string())?.list_grid_rules()
}

#[tauri::command]
pub fn automation_save_grid_rule(
    rule: GridRule,
    state: State<AppState>,
) -> Result<GridRule, String> {
    let saved = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        validate::validate_grid_rule(&rule, &library)?;
        library.save_grid_rule(&rule)?;
        rule
    };
    invalidate(&state)?;
    let saved = state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .list_grid_rules()?
        .into_iter()
        .find(|rule| rule.id == saved.id)
        .unwrap_or(saved);
    Ok(saved)
}

#[tauri::command]
pub fn automation_delete_grid_rule(id: String, state: State<AppState>) -> Result<(), String> {
    let deleted = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.delete_grid_rule(&id)?
    };
    if !deleted {
        return Err(format!("No grid rule {id}"));
    }
    invalidate(&state)?;
    Ok(())
}

#[tauri::command]
pub fn automation_advertisers(state: State<AppState>) -> Result<Vec<Advertiser>, String> {
    state.library.lock().map_err(|e| e.to_string())?.list_advertisers()
}

#[tauri::command]
pub fn automation_save_advertiser(
    advertiser: Advertiser,
    state: State<AppState>,
) -> Result<Advertiser, String> {
    let id = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.save_advertiser(&advertiser)?
    };
    invalidate(&state)?;
    let saved = state
        .library
        .lock()
        .map_err(|e| e.to_string())?
        .list_advertisers()?
        .into_iter()
        .find(|saved| saved.id == id)
        .unwrap_or(Advertiser { id, ..advertiser });
    Ok(saved)
}

#[tauri::command]
pub fn automation_delete_advertiser(id: i64, state: State<AppState>) -> Result<(), String> {
    let deleted = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.delete_advertiser(id)?
    };
    if !deleted {
        return Err(format!("No advertiser {id}"));
    }
    invalidate(&state)?;
    Ok(())
}

#[tauri::command]
pub fn automation_campaigns(state: State<AppState>) -> Result<Vec<Campaign>, String> {
    state.library.lock().map_err(|e| e.to_string())?.list_campaigns()
}

#[tauri::command]
pub fn automation_save_campaign(
    campaign: Campaign,
    state: State<AppState>,
) -> Result<Campaign, String> {
    let id = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.save_campaign(&campaign)?
    };
    invalidate(&state)?;
    let saved = state.library.lock().map_err(|e| e.to_string())?.get_campaign(id)?;
    saved.ok_or_else(|| format!("No campaign {id}"))
}

#[tauri::command]
pub fn automation_delete_campaign(id: i64, state: State<AppState>) -> Result<(), String> {
    let deleted = {
        let mut library = state.library.lock().map_err(|e| e.to_string())?;
        library.delete_campaign(id)?
    };
    if !deleted {
        return Err(format!("No campaign {id}"));
    }
    invalidate(&state)?;
    Ok(())
}

#[tauri::command]
pub async fn automation_preview(
    app: AppHandle,
    hours: u32,
    from: Option<String>,
) -> Result<Vec<PlaylistItem>, String> {
    let hours = validate::preview_hours(Some(hours))?;
    let from = match from {
        Some(text) => riars_core::automation::parse_api_datetime(&text)?,
        None => chrono::Local::now().naive_local(),
    };
    let overlap = {
        let state = app.state::<AppState>();
        let engine = state.engine.lock().map_err(|e| e.to_string())?;
        let fade = engine.crossfade_config();
        if fade.enabled { fade.duration.as_millis() as u64 } else { 0 }
    };
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        riars_core::automation::preview(&state.library, from, hours, overlap)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn automation_log(
    app: AppHandle,
    from: Option<String>,
    to: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<PlayLogEntry>, String> {
    let (from, to) = validate::datetime_range(from.as_deref(), to.as_deref())?;
    let limit = limit.unwrap_or(500).clamp(1, 5000);
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let library = state.library.lock().map_err(|e| e.to_string())?;
        library.play_log(from, to, limit)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn automation_delivery(
    app: AppHandle,
    from: Option<String>,
    to: Option<String>,
) -> Result<Vec<DeliveryRow>, String> {
    let (from, to) = validate::date_range(from.as_deref(), to.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let library = state.library.lock().map_err(|e| e.to_string())?;
        library.delivery_report(from, to)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn automation_export_csv(
    app: AppHandle,
    kind: String,
    path: String,
    from: Option<String>,
    to: Option<String>,
    advertiser_id: Option<i64>,
    campaign_id: Option<i64>,
) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let library = state.library.lock().map_err(|e| e.to_string())?;
        let csv = match kind.as_str() {
            "log" => {
                let (from, to) = validate::datetime_range(from.as_deref(), to.as_deref())?;
                library.play_log_csv(from, to)?
            }
            "delivery" => {
                let (from, to) = validate::date_range(from.as_deref(), to.as_deref())?;
                library.delivery_csv(from, to)?
            }
            "campaign-log" => {
                let (from, to) = validate::datetime_range(from.as_deref(), to.as_deref())?;
                library.campaign_log_csv(from, to, advertiser_id, campaign_id)?
            }
            other => {
                return Err(format!(
                    "kind must be log, delivery or campaign-log, not {other}"
                ))
            }
        };
        let bytes = csv.into_bytes();
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(bytes.len() as u64)
    })
    .await
    .map_err(|e| e.to_string())?
}
