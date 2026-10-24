//! Data model of long-term automation: categories, format clocks, the dayparts grid
//! and station settings. See `docs/automation.md`.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::audio::engine::EndReason;
use crate::playlist::playlist::ItemKind;

/// A group of tracks the generator picks from: a music style, jingles, spots...
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub kind: ItemKind,
    /// UI colour, e.g. "#e91e63".
    #[serde(default)]
    pub color: String,
    /// Subcategory of this one: picking from the parent also picks from it.
    #[serde(default)]
    pub parent_id: Option<i64>,
    /// Smart category: tracks matching this filter instead of assigned ones.
    #[serde(default)]
    pub filter: Option<TrackFilter>,
    /// Separation rules; `None` uses the station defaults.
    #[serde(default)]
    pub separation: Option<Separation>,
}

/// Which tracks a smart category holds. Empty fields match everything; text
/// matches ignore case and accents.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackFilter {
    pub genre: Option<String>,
    pub artist: Option<String>,
    /// File path starts with this folder.
    pub folder: Option<String>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    pub energy_min: Option<u8>,
    pub energy_max: Option<u8>,
    pub min_duration_ms: Option<u64>,
    pub max_duration_ms: Option<u64>,
}

/// Rule that assigns scanned tracks to a category automatically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryRule {
    pub id: i64,
    pub category_id: i64,
    pub field: RuleField,
    /// Compared ignoring case and accents: equal for `Genre`/`Artist`, prefix for
    /// `Folder`.
    pub pattern: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleField {
    Genre,
    Artist,
    Folder,
}

/// How far apart repeats must be. Zero disables a rule.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Separation {
    /// Same track.
    pub song_minutes: u32,
    pub artist_minutes: u32,
    pub album_minutes: u32,
    /// Avoid the hour a track played in the day before (plus or minus 30 min).
    pub horizontal: bool,
    /// Imaging: the same element only after this many others of its category.
    pub imaging_others: u32,
}

impl Default for Separation {
    fn default() -> Self {
        Self { song_minutes: 240, artist_minutes: 45, album_minutes: 0, horizontal: false, imaging_others: 3 }
    }
}

/// A format clock: what one hour (or `duration_minutes`) sounds like.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clock {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default = "default_clock_minutes")]
    pub duration_minutes: u32,
    pub slots: Vec<ClockSlot>,
    /// Music to fill with when the slots run out before the next anchor or the end.
    #[serde(default)]
    pub fill_category: Option<i64>,
}

fn default_clock_minutes() -> u32 {
    60
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClockSlot {
    /// Time anchor inside the clock; slots without one follow the previous slot.
    #[serde(default)]
    pub at: Option<Anchor>,
    pub item: SlotItem,
}

/// "At minute 15": where a slot should start.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub minute: u32,
    #[serde(default)]
    pub second: u32,
    /// Hard: cut what plays at that time. Soft (default): start after the item
    /// playing then.
    #[serde(default)]
    pub hard: bool,
    /// Soft anchors: how late they may start (Rivendell's "wait up to").
    #[serde(default = "default_max_wait")]
    pub max_wait_seconds: u32,
}

fn default_max_wait() -> u32 {
    120
}

impl Anchor {
    pub fn offset_seconds(&self) -> i64 {
        self.minute as i64 * 60 + self.second as i64
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SlotItem {
    /// `count` tracks from a category, optionally only those with (or without)
    /// some codes.
    Category {
        category_id: i64,
        #[serde(default = "one")]
        count: u32,
        #[serde(default)]
        require_codes: Vec<String>,
        #[serde(default)]
        exclude_codes: Vec<String>,
    },
    /// One imaging element (jingle, sweeper, bumper, station ID) from a category.
    Imaging { category_id: i64 },
    /// An ad break, filled with spots of the campaigns due. `max_seconds` 0 means
    /// the station default. The intro and outro (imaging categories) frame the
    /// break when it has spots.
    AdBreak {
        name: String,
        #[serde(default)]
        max_seconds: u32,
        #[serde(default)]
        intro_category: Option<i64>,
        #[serde(default)]
        outro_category: Option<i64>,
    },
    /// A saved playlist: all of it, or the next `count` items (continuing where
    /// it was left the previous time).
    Playlist {
        playlist_id: String,
        #[serde(default)]
        count: Option<u32>,
    },
    /// A fixed file, such as a programme's theme.
    Track { path: String },
    /// Music from a category until the next anchor or the end of the clock.
    Fill { category_id: i64 },
}

fn one() -> u32 {
    1
}

/// What a grid rule puts on air.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GridTarget {
    Clock { clock_id: String },
    /// A saved playlist played in order, continuing where it was left.
    Playlist { playlist_id: String },
}

/// One rule of the dayparts grid: which clock or playlist plays when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridRule {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Higher wins; ties go to the rule covering less time.
    #[serde(default)]
    pub priority: i32,
    pub target: GridTarget,
    /// Bit 0 = Monday ... bit 6 = Sunday.
    #[serde(default = "all_days")]
    pub weekdays: u8,
    /// Bit 0 = January ... bit 11 = December.
    #[serde(default = "all_months")]
    pub months: u16,
    /// Bit 0..4 = first..fifth week of the month (days 1-7, 8-14...), bit 5 = last
    /// seven days of the month.
    #[serde(default = "all_weeks")]
    pub weeks_of_month: u8,
    /// Daily span; `end <= start` crosses midnight, `end == start` is all day.
    pub start: NaiveTime,
    pub end: NaiveTime,
    #[serde(default)]
    pub date_from: Option<NaiveDate>,
    #[serde(default)]
    pub date_to: Option<NaiveDate>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn all_days() -> u8 {
    0x7f
}
fn all_months() -> u16 {
    0xfff
}
fn all_weeks() -> u8 {
    0x3f
}
fn yes() -> bool {
    true
}

/// Imaging the generator adds on its own, outside the clocks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImagingRules {
    /// A jingle from this category every `jingle_every` songs (0 = never).
    pub jingle_category: Option<i64>,
    pub jingle_every: u32,
    /// A sweeper from this category between two songs of different categories.
    pub sweeper_on_change: Option<i64>,
}

/// Station-wide automation settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationConfig {
    /// The generator feeds the on-air playlist.
    pub enabled: bool,
    /// Clock for hours no grid rule covers.
    pub default_clock: Option<String>,
    /// Music when a category has nothing playable.
    pub fallback_category: Option<i64>,
    pub separation: Separation,
    pub imaging: ImagingRules,
    /// Keep at least this many items ahead of the one on air...
    pub min_items_ahead: u32,
    /// ...and at least this much audio.
    pub min_minutes_ahead: u32,
    /// Played items kept in the on-air playlist.
    pub keep_played: u32,
    /// Length of an ad break whose slot does not say.
    pub ad_break_seconds: u32,
    /// Items of these kinds never crossfade with their neighbours.
    pub no_overlap_kinds: Vec<ItemKind>,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_clock: None,
            fallback_category: None,
            separation: Separation::default(),
            imaging: ImagingRules::default(),
            min_items_ahead: 3,
            min_minutes_ahead: 15,
            keep_played: 20,
            ad_break_seconds: 180,
            no_overlap_kinds: vec![ItemKind::Spot],
        }
    }
}

/// A track the generator may pick, as read from the library: only what rotation
/// and separation need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateRow {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    /// Rotation weight in this category (1.0 = normal).
    pub weight: f32,
    pub last_played_at: Option<NaiveDateTime>,
    pub codes: Vec<String>,
}

/// One line of the proof-of-play log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayLogEntry {
    pub id: i64,
    pub aired_at: NaiveDateTime,
    pub ended_at: NaiveDateTime,
    pub track_id: Option<i64>,
    pub file_path: String,
    pub title: String,
    pub artist: String,
    pub kind: ItemKind,
    pub category_id: Option<i64>,
    pub campaign_id: Option<i64>,
    pub planned_at: Option<NaiveDateTime>,
    /// How long it was on air.
    pub played_ms: u64,
    pub reason: EndReason,
    /// Played to its end (or nearly: 95 % of its length).
    pub complete: bool,
}

/// A company that buys airtime.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Advertiser {
    pub id: i64,
    pub name: String,
    pub contact: String,
    pub notes: String,
}

/// When a campaign may air: days (bit 0 = Monday) and a daily span (`end <= start`
/// crosses midnight, `end == start` is all day).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Daypart {
    #[serde(default = "all_days")]
    pub weekdays: u8,
    pub start: NaiveTime,
    pub end: NaiveTime,
}

/// What a sponsorship is attached to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SponsorTarget {
    /// Every period of this clock (a programme with its own clock).
    Clock { clock_id: String },
    /// The span of this grid rule.
    GridRule { rule_id: String },
}

/// "This programme comes to you thanks to...": a spot at the opening and/or the
/// closing of the sponsored programme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sponsorship {
    pub target: SponsorTarget,
    #[serde(default = "yes")]
    pub opening: bool,
    #[serde(default)]
    pub closing: bool,
}

/// An audio spot of a campaign; spots of a campaign rotate by weight.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CampaignSpot {
    pub track_id: i64,
    #[serde(default = "one_f32")]
    pub weight: f32,
}

fn one_f32() -> f32 {
    1.0
}

/// An advertising campaign: how many spots a day, when, and between which dates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Campaign {
    #[serde(default)]
    pub id: i64,
    pub advertiser_id: i64,
    pub name: String,
    /// First and last day it airs (both included).
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    /// Spots a day in ad breaks (0 for a pure sponsorship).
    #[serde(default)]
    pub plays_per_day: u32,
    /// At most this many in one clock hour (0 = no limit).
    #[serde(default)]
    pub max_per_hour: u32,
    /// Empty: any time.
    #[serde(default)]
    pub dayparts: Vec<Daypart>,
    /// Higher goes first when breaks are full.
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub sponsorship: Option<Sponsorship>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub spots: Vec<CampaignSpot>,
}

/// How one campaign did on one day, for billing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveryRow {
    pub date: NaiveDate,
    pub campaign_id: i64,
    pub campaign: String,
    pub advertiser_id: i64,
    pub advertiser: String,
    /// Spots it should have had that day.
    pub target: u32,
    /// Spots aired to the end.
    pub aired: u32,
    /// Spots cut short.
    pub partial: u32,
    pub seconds: u64,
    pub missed: u32,
}

/// Earliest and latest dates the automation API accepts: anything outside is a typo,
/// and chrono's extremes would overflow date arithmetic.
const FIRST_YEAR: i32 = 1970;
const LAST_YEAR: i32 = 2200;

/// Parse a date-time from an API or tool argument: `YYYY-MM-DDTHH:MM:SS` or with a
/// space, optional seconds, within years 1970 to 2200.
pub fn parse_api_datetime(text: &str) -> Result<NaiveDateTime, String> {
    let text = text.trim();
    let parsed = ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
        .ok_or_else(|| format!("Invalid date and time \"{text}\": use YYYY-MM-DDTHH:MM:SS"))?;
    check_year(parsed.date()).map(|_| parsed)
}

/// Parse a date (`YYYY-MM-DD`) from an API or tool argument, within years 1970 to 2200.
pub fn parse_api_date(text: &str) -> Result<NaiveDate, String> {
    let text = text.trim();
    let parsed = NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| format!("Invalid date \"{text}\": use YYYY-MM-DD"))?;
    check_year(parsed).map(|_| parsed)
}

fn check_year(date: NaiveDate) -> Result<(), String> {
    use chrono::Datelike;
    if (FIRST_YEAR..=LAST_YEAR).contains(&date.year()) {
        Ok(())
    } else {
        Err(format!("Date {date} is out of range ({FIRST_YEAR} to {LAST_YEAR})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_dates() {
        let t = parse_api_datetime("2026-10-23T09:15:00").unwrap();
        assert_eq!(parse_api_datetime("2026-10-23 09:15").unwrap(), t);
        assert!(parse_api_datetime("-262143-01-01T00:00:00").is_err());
        assert!(parse_api_datetime("+262142-12-31T23:59:59").is_err());
        assert!(parse_api_datetime("23/10/2026").is_err());
        assert_eq!(parse_api_date("2026-10-23").unwrap(), t.date());
        assert!(parse_api_date("1900-01-01").is_err());
    }
}
