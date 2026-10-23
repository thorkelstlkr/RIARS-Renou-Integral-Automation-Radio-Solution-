//! Validation shared by the REST API, the MCP server and the desktop app.

use std::collections::HashSet;

use chrono::{Local, NaiveDate, NaiveDateTime};

use crate::library::db::MediaLibrary;

use super::model::{AutomationConfig, Category, Clock, GridRule, GridTarget, SlotItem};
use super::runner::MAX_PREVIEW_HOURS;
use super::{parse_api_date, parse_api_datetime};

pub fn validate_config(config: &AutomationConfig) -> Result<(), String> {
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

pub fn validate_category(category: &Category) -> Result<(), String> {
    if category.name.trim().is_empty() {
        return Err("category name is required".to_string());
    }
    Ok(())
}

fn clock_category_ids(clock: &Clock) -> Vec<i64> {
    let mut ids = Vec::new();
    if let Some(id) = clock.fill_category {
        ids.push(id);
    }
    for slot in &clock.slots {
        match &slot.item {
            SlotItem::Category { category_id, .. }
            | SlotItem::Imaging { category_id }
            | SlotItem::Fill { category_id } => ids.push(*category_id),
            SlotItem::AdBreak { intro_category, outro_category, .. } => {
                if let Some(id) = intro_category {
                    ids.push(*id);
                }
                if let Some(id) = outro_category {
                    ids.push(*id);
                }
            }
            SlotItem::Playlist { .. } | SlotItem::Track { .. } => {}
        }
    }
    ids
}

pub fn validate_clock(clock: &Clock, library: &MediaLibrary) -> Result<(), String> {
    if clock.id.trim().is_empty() {
        return Err("clock id is required".to_string());
    }
    if clock.name.trim().is_empty() {
        return Err("clock name is required".to_string());
    }
    if !(5..=1440).contains(&clock.duration_minutes) {
        return Err("duration_minutes must be between 5 and 1440".to_string());
    }
    let clock_seconds = clock.duration_minutes as i64 * 60;
    for slot in &clock.slots {
        if let Some(anchor) = &slot.at {
            if anchor.second >= 60 {
                return Err("anchor second must be below 60".to_string());
            }
            if anchor.offset_seconds() >= clock_seconds {
                return Err(format!(
                    "anchor minute {} is outside a {}-minute clock",
                    anchor.minute, clock.duration_minutes
                ));
            }
        }
    }
    let categories: HashSet<i64> =
        library.list_categories()?.into_iter().map(|category| category.id).collect();
    for id in clock_category_ids(clock) {
        if !categories.contains(&id) {
            return Err(format!("No category {id}"));
        }
    }
    for slot in &clock.slots {
        match &slot.item {
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
            _ => {}
        }
    }
    Ok(())
}

pub fn validate_grid_rule(rule: &GridRule, library: &MediaLibrary) -> Result<(), String> {
    if rule.id.trim().is_empty() {
        return Err("grid rule id is required".to_string());
    }
    if let (Some(from), Some(to)) = (rule.date_from, rule.date_to) {
        if to < from {
            return Err("date_to must not be before date_from".to_string());
        }
    }
    match &rule.target {
        GridTarget::Clock { clock_id } => {
            if !library.list_clocks()?.iter().any(|clock| &clock.id == clock_id) {
                return Err(format!("No clock {clock_id}"));
            }
        }
        GridTarget::Playlist { playlist_id } => {
            library
                .load_playlist(playlist_id)
                .map_err(|e| format!("No playlist {playlist_id}: {e}"))?;
        }
    }
    Ok(())
}

pub fn validate_weight(weight: f32, remove: bool) -> Result<(), String> {
    if !remove && !(0.01..=100.0).contains(&weight) {
        return Err("weight must be between 0.01 and 100".to_string());
    }
    Ok(())
}

pub fn datetime_range(
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(NaiveDateTime, NaiveDateTime), String> {
    let to = match to {
        Some(text) => parse_api_datetime(text)?,
        None => Local::now().naive_local(),
    };
    let from = match from {
        Some(text) => parse_api_datetime(text)?,
        None => to - chrono::Duration::hours(24),
    };
    if to < from {
        return Err("to must not be before from".to_string());
    }
    if (to - from).num_days() > 366 {
        return Err("date span must be at most 366 days".to_string());
    }
    Ok((from, to))
}

pub fn date_range(from: Option<&str>, to: Option<&str>) -> Result<(NaiveDate, NaiveDate), String> {
    let to = match to {
        Some(text) => parse_api_date(text)?,
        None => Local::now().date_naive(),
    };
    let from = match from {
        Some(text) => parse_api_date(text)?,
        None => to,
    };
    if to < from {
        return Err("to must not be before from".to_string());
    }
    if (to - from).num_days() > 365 {
        return Err("date span must be at most 366 days".to_string());
    }
    Ok((from, to))
}

pub fn preview_hours(hours: Option<u32>) -> Result<u32, String> {
    let hours = hours.unwrap_or(1);
    if !(1..=MAX_PREVIEW_HOURS).contains(&hours) {
        return Err(format!("hours must be between 1 and {MAX_PREVIEW_HOURS}"));
    }
    Ok(hours)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use chrono::NaiveTime;

    use crate::automation::model::{
        Anchor, AutomationConfig, Category, Clock, ClockSlot, GridRule, GridTarget, SlotItem,
    };
    use crate::playlist::playlist::ItemKind;

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

    fn temp_db(name: &str) -> (MediaLibrary, PathBuf) {
        let path =
            std::env::temp_dir().join(format!("riars-validate-{}-{name}.db", std::process::id()));
        remove_db_files(&path);
        let library = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        (library, path)
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

    fn clock(id: &str, name: &str, duration_minutes: u32, slots: Vec<ClockSlot>) -> Clock {
        Clock {
            id: id.to_string(),
            name: name.to_string(),
            color: String::new(),
            duration_minutes,
            slots,
            fill_category: None,
        }
    }

    fn slot(item: SlotItem) -> ClockSlot {
        ClockSlot { at: None, item }
    }

    fn at(minute: u32, second: u32) -> ClockSlot {
        ClockSlot {
            at: Some(Anchor { minute, second, hard: false, max_wait_seconds: 120 }),
            item: SlotItem::Fill { category_id: 1 },
        }
    }

    fn grid_rule(target: GridTarget) -> GridRule {
        GridRule {
            id: "g".to_string(),
            name: "Rule".to_string(),
            priority: 0,
            target,
            weekdays: 0x7f,
            months: 0xfff,
            weeks_of_month: 0x3f,
            start: NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            date_from: None,
            date_to: None,
            enabled: true,
        }
    }

    #[test]
    fn config_limits() {
        let mut config = AutomationConfig::default();
        assert!(validate_config(&config).is_ok());

        config.min_items_ahead = 0;
        assert_eq!(
            validate_config(&config).unwrap_err(),
            "min_items_ahead must be between 1 and 50"
        );
        config.min_items_ahead = 51;
        assert!(validate_config(&config).is_err());

        let mut config = AutomationConfig::default();
        config.min_minutes_ahead = 0;
        assert_eq!(
            validate_config(&config).unwrap_err(),
            "min_minutes_ahead must be between 1 and 240"
        );
        config.min_minutes_ahead = 241;
        assert!(validate_config(&config).is_err());

        let mut config = AutomationConfig::default();
        config.keep_played = 501;
        assert_eq!(validate_config(&config).unwrap_err(), "keep_played must be at most 500");

        let mut config = AutomationConfig::default();
        config.ad_break_seconds = 9;
        assert_eq!(
            validate_config(&config).unwrap_err(),
            "ad_break_seconds must be between 10 and 1800"
        );
        config.ad_break_seconds = 1801;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn category_name_cannot_be_blank() {
        assert!(validate_category(&category("Pop")).is_ok());
        assert_eq!(validate_category(&category("   ")).unwrap_err(), "category name is required");
    }

    #[test]
    fn clock_sizes_ids_and_anchors() {
        let (library, path) = temp_db("clock-anchors");

        assert_eq!(
            validate_clock(&clock("", "Clock", 60, Vec::new()), &library).unwrap_err(),
            "clock id is required"
        );
        assert_eq!(
            validate_clock(&clock("c", "  ", 60, Vec::new()), &library).unwrap_err(),
            "clock name is required"
        );
        assert_eq!(
            validate_clock(&clock("c", "Clock", 4, Vec::new()), &library).unwrap_err(),
            "duration_minutes must be between 5 and 1440"
        );
        assert_eq!(
            validate_clock(&clock("c", "Clock", 1441, Vec::new()), &library).unwrap_err(),
            "duration_minutes must be between 5 and 1440"
        );
        assert_eq!(
            validate_clock(&clock("c", "Clock", 60, vec![at(1, 60)]), &library).unwrap_err(),
            "anchor second must be below 60"
        );
        assert_eq!(
            validate_clock(&clock("c", "Clock", 60, vec![at(60, 0)]), &library).unwrap_err(),
            "anchor minute 60 is outside a 60-minute clock"
        );

        drop(library);
        remove_db_files(&path);
    }

    #[test]
    fn clock_references_must_exist() {
        let (mut library, path) = temp_db("clock-references");
        let pop = library.create_category(&category("Pop")).unwrap();

        let missing = clock("c", "Clock", 60, vec![slot(SlotItem::Fill { category_id: 9999 })]);
        assert_eq!(validate_clock(&missing, &library).unwrap_err(), "No category 9999");

        let valid = clock("c", "Clock", 60, vec![slot(SlotItem::Fill { category_id: pop })]);
        assert!(validate_clock(&valid, &library).is_ok());

        let missing_playlist = clock(
            "c",
            "Clock",
            60,
            vec![slot(SlotItem::Playlist { playlist_id: "missing".to_string(), count: None })],
        );
        assert!(validate_clock(&missing_playlist, &library).is_err());

        let missing_file = std::env::temp_dir().join(format!(
            "riars-validate-{}-missing.mp3",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&missing_file);
        let missing_track = clock(
            "c",
            "Clock",
            60,
            vec![slot(SlotItem::Track { path: missing_file.to_string_lossy().into_owned() })],
        );
        assert!(validate_clock(&missing_track, &library).is_err());

        drop(library);
        remove_db_files(&path);
    }

    #[test]
    fn grid_rules_need_ids_ranges_and_targets() {
        let (mut library, path) = temp_db("grid-rules");

        let mut rule = grid_rule(GridTarget::Clock { clock_id: "missing".to_string() });
        rule.id = "   ".to_string();
        assert_eq!(validate_grid_rule(&rule, &library).unwrap_err(), "grid rule id is required");

        let mut rule = grid_rule(GridTarget::Clock { clock_id: "missing".to_string() });
        rule.date_from = Some(NaiveDate::from_ymd_opt(2024, 2, 1).unwrap());
        rule.date_to = Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
        assert_eq!(
            validate_grid_rule(&rule, &library).unwrap_err(),
            "date_to must not be before date_from"
        );

        let rule = grid_rule(GridTarget::Clock { clock_id: "missing".to_string() });
        assert_eq!(validate_grid_rule(&rule, &library).unwrap_err(), "No clock missing");

        let pop = library.create_category(&category("Pop")).unwrap();
        library
            .save_clock(&clock(
                "c",
                "Clock",
                60,
                vec![slot(SlotItem::Fill { category_id: pop })],
            ))
            .unwrap();
        let rule = grid_rule(GridTarget::Clock { clock_id: "c".to_string() });
        assert!(validate_grid_rule(&rule, &library).is_ok());

        let rule =
            grid_rule(GridTarget::Playlist { playlist_id: "missing".to_string() });
        assert!(validate_grid_rule(&rule, &library).is_err());

        drop(library);
        remove_db_files(&path);
    }

    #[test]
    fn weight_is_checked_only_when_adding() {
        assert!(validate_weight(0.01, false).is_ok());
        assert!(validate_weight(100.0, false).is_ok());
        assert_eq!(
            validate_weight(0.0, false).unwrap_err(),
            "weight must be between 0.01 and 100"
        );
        assert!(validate_weight(100.01, false).is_err());
        assert!(validate_weight(0.0, true).is_ok());
        assert!(validate_weight(-5.0, true).is_ok());
    }

    #[test]
    fn datetime_ranges_reject_backwards_and_long_spans() {
        let (from, to) = datetime_range(Some("2024-01-01T00:00:00"), Some("2024-01-02T00:00:00"))
            .unwrap();
        assert_eq!(to - from, chrono::Duration::days(1));

        let (from, to) = datetime_range(None, Some("2024-01-02T00:00:00")).unwrap();
        assert_eq!(to - from, chrono::Duration::hours(24));

        assert_eq!(
            datetime_range(Some("2024-01-02T00:00:00"), Some("2024-01-01T00:00:00")).unwrap_err(),
            "to must not be before from"
        );
        assert_eq!(
            datetime_range(Some("2024-01-01T00:00:00"), Some("2026-01-01T00:00:00")).unwrap_err(),
            "date span must be at most 366 days"
        );
        assert!(datetime_range(Some("1900-01-01T00:00:00"), None).is_err());
    }

    #[test]
    fn date_ranges_reject_backwards_and_long_spans() {
        let (from, to) = date_range(Some("2024-01-01"), Some("2024-01-02")).unwrap();
        assert_eq!(to - from, chrono::Duration::days(1));

        let (from, to) = date_range(None, Some("2024-01-02")).unwrap();
        assert_eq!(from, to);

        assert_eq!(
            date_range(Some("2024-01-02"), Some("2024-01-01")).unwrap_err(),
            "to must not be before from"
        );
        assert_eq!(
            date_range(Some("2024-01-01"), Some("2026-01-01")).unwrap_err(),
            "date span must be at most 366 days"
        );
        assert!(date_range(Some("1900-01-01"), None).is_err());
    }

    #[test]
    fn preview_hours_default_to_one() {
        assert_eq!(preview_hours(None).unwrap(), 1);
        assert_eq!(preview_hours(Some(1)).unwrap(), 1);
        assert_eq!(preview_hours(Some(MAX_PREVIEW_HOURS)).unwrap(), MAX_PREVIEW_HOURS);
        assert_eq!(
            preview_hours(Some(0)).unwrap_err(),
            format!("hours must be between 1 and {MAX_PREVIEW_HOURS}")
        );
        assert!(preview_hours(Some(MAX_PREVIEW_HOURS + 1)).is_err());
    }
}
