use std::collections::HashMap;
use std::path::Path;

use chrono::{Datelike, Duration, NaiveDateTime, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventTrigger {
    /// Once, at the next occurrence of this time; the event then disables itself.
    Time(NaiveTime),
    Interval { minutes: u32 },
    Weekly { day: Weekday, time: NaiveTime },
    Daily { time: NaiveTime },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventAction {
    PlayPlaylist { playlist_id: String },
    PlayTrack { track_path: String },
    StartStreaming,
    StopStreaming,
    SetVolume { level: f32 },
    FadeToVolume { level: f32, duration_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledEvent {
    pub id: String,
    pub name: String,
    pub trigger: EventTrigger,
    pub action: EventAction,
    pub enabled: bool,
}

impl ScheduledEvent {
    pub fn new(name: String, trigger: EventTrigger, action: EventAction) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            trigger,
            action,
            enabled: true,
        }
    }
}

/// After a long gap (sleep, suspend), only events from this last stretch fire, so the
/// station does not run a backlog of missed events all at once.
const MAX_CATCH_UP_SECS: i64 = 60;

pub struct Scheduler {
    events: Vec<ScheduledEvent>,
    /// When `take_due` last ran; events fire when their time falls after it.
    last_check: Option<NaiveDateTime>,
    /// Next firing time of each Interval event, by id.
    next_interval: HashMap<String, NaiveDateTime>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self { events: Vec::new(), last_check: None, next_interval: HashMap::new() }
    }

    /// Load events saved with `save`; a missing file gives an empty schedule.
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut scheduler = Self::new();
        match std::fs::read_to_string(path) {
            Ok(json) => {
                scheduler.events = serde_json::from_str(&json)
                    .map_err(|e| format!("Invalid schedule file {}: {e}", path.display()))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
        }
        Ok(scheduler)
    }

    /// Write the events as JSON, through a temporary file so a crash cannot leave
    /// a half-written schedule.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.events).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("Cannot write {}: {e}", path.display()))
    }

    pub fn add_event(&mut self, event: ScheduledEvent) {
        self.events.push(event);
    }

    pub fn remove_event(&mut self, id: &str) -> bool {
        let before = self.events.len();
        self.events.retain(|e| e.id != id);
        self.next_interval.remove(id);
        self.events.len() < before
    }

    pub fn toggle_event(&mut self, id: &str) -> Option<bool> {
        self.next_interval.remove(id);
        self.events.iter_mut().find(|e| e.id == id).map(|e| {
            e.enabled = !e.enabled;
            e.enabled
        })
    }

    pub fn events(&self) -> &[ScheduledEvent] {
        &self.events
    }

    /// Events whose time came since the previous call, each exactly once. Call it
    /// often (every second or less). The first call only starts the clock. One-shot
    /// `Time` events disable themselves after firing.
    pub fn take_due(&mut self, now: NaiveDateTime) -> Vec<ScheduledEvent> {
        let Some(prev) = self.last_check.replace(now) else {
            return Vec::new();
        };
        if now <= prev {
            return Vec::new(); // clock went back
        }
        let since = prev.max(now - Duration::seconds(MAX_CATCH_UP_SECS));
        let fired_in = |time: NaiveTime, day: Option<Weekday>| {
            [since.date(), now.date()].into_iter().any(|date| {
                let at = date.and_time(time);
                since < at && at <= now && day.is_none_or(|d| at.weekday() == d)
            })
        };

        let mut due = Vec::new();
        for event in self.events.iter_mut().filter(|e| e.enabled) {
            let fire = match &event.trigger {
                EventTrigger::Time(t) => fired_in(*t, None),
                EventTrigger::Daily { time } => fired_in(*time, None),
                EventTrigger::Weekly { day, time } => fired_in(*time, Some(*day)),
                EventTrigger::Interval { minutes } => {
                    let every = Duration::minutes((*minutes).max(1) as i64);
                    let next = self.next_interval.entry(event.id.clone()).or_insert(now + every);
                    if *next <= now {
                        while *next <= now {
                            *next += every;
                        }
                        true
                    } else {
                        false
                    }
                }
            };
            if fire {
                if matches!(event.trigger, EventTrigger::Time(_)) {
                    event.enabled = false;
                }
                due.push(event.clone());
            }
        }
        due
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn at(day: u32, h: u32, m: u32, s: u32) -> NaiveDateTime {
        // 2026-10-19 is a Monday.
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap().and_hms_opt(h, m, s).unwrap()
    }

    fn time(h: u32, m: u32, s: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, s).unwrap()
    }

    fn with(trigger: EventTrigger) -> Scheduler {
        let mut s = Scheduler::new();
        s.add_event(ScheduledEvent::new("e".into(), trigger, EventAction::StartStreaming));
        s
    }

    /// Call take_due every `step` seconds from `from` to `to`; return how many fired.
    fn run(s: &mut Scheduler, from: NaiveDateTime, to: NaiveDateTime, step: i64) -> usize {
        let mut fired = 0;
        let mut t = from;
        while t <= to {
            fired += s.take_due(t).len();
            t += Duration::seconds(step);
        }
        fired
    }

    #[test]
    fn daily_fires_once_with_fast_ticks() {
        let mut s = with(EventTrigger::Daily { time: time(8, 0, 0) });
        assert_eq!(run(&mut s, at(19, 7, 59, 0), at(19, 8, 1, 0), 1), 1);
        // and again the next day
        assert_eq!(run(&mut s, at(20, 7, 59, 59), at(20, 8, 0, 1), 1), 1);
    }

    #[test]
    fn late_tick_still_fires() {
        let mut s = with(EventTrigger::Daily { time: time(8, 0, 0) });
        s.take_due(at(19, 7, 59, 58));
        assert_eq!(s.take_due(at(19, 8, 0, 3)).len(), 1);
    }

    #[test]
    fn first_call_and_backlog_do_not_fire() {
        let mut s = with(EventTrigger::Daily { time: time(8, 0, 0) });
        assert!(s.take_due(at(19, 8, 0, 0)).is_empty()); // first call starts the clock
        // Woken up 3 hours later: the 08:00 of the next day is missed, nothing fires.
        s.take_due(at(20, 7, 0, 0));
        assert!(s.take_due(at(20, 10, 0, 0)).is_empty());
    }

    #[test]
    fn across_midnight() {
        let mut s = with(EventTrigger::Daily { time: time(0, 0, 0) });
        assert_eq!(run(&mut s, at(19, 23, 59, 50), at(20, 0, 0, 10), 1), 1);
    }

    #[test]
    fn weekly_only_on_its_day() {
        let mut s = with(EventTrigger::Weekly { day: Weekday::Tue, time: time(9, 0, 0) });
        assert_eq!(run(&mut s, at(19, 8, 59, 0), at(19, 9, 1, 0), 1), 0); // Monday
        assert_eq!(run(&mut s, at(20, 8, 59, 0), at(20, 9, 1, 0), 1), 1); // Tuesday
    }

    #[test]
    fn one_shot_disables_itself() {
        let mut s = with(EventTrigger::Time(time(12, 0, 0)));
        assert_eq!(run(&mut s, at(19, 11, 59, 0), at(19, 12, 1, 0), 1), 1);
        assert!(!s.events()[0].enabled);
        assert_eq!(run(&mut s, at(20, 11, 59, 0), at(20, 12, 1, 0), 1), 0);
    }

    #[test]
    fn interval_every_minute() {
        let mut s = with(EventTrigger::Interval { minutes: 1 });
        // Clock starts at 10:00:00, then the event fires at 10:01, 10:02, 10:03.
        assert_eq!(run(&mut s, at(19, 10, 0, 0), at(19, 10, 3, 30), 1), 3);
    }

    #[test]
    fn disabled_does_not_fire() {
        let mut s = with(EventTrigger::Daily { time: time(8, 0, 0) });
        let id = s.events()[0].id.clone();
        s.toggle_event(&id);
        assert_eq!(run(&mut s, at(19, 7, 59, 0), at(19, 8, 1, 0), 1), 0);
    }

    #[test]
    fn save_and_load() {
        let path = std::env::temp_dir().join(format!("riars-schedule-{}.json", std::process::id()));
        let s = with(EventTrigger::Weekly { day: Weekday::Fri, time: time(18, 30, 0) });
        s.save(&path).unwrap();
        let loaded = Scheduler::load(&path).unwrap();
        assert_eq!(loaded.events().len(), 1);
        assert!(matches!(loaded.events()[0].trigger, EventTrigger::Weekly { day: Weekday::Fri, .. }));
        std::fs::remove_file(&path).unwrap();
        assert!(Scheduler::load(&path).unwrap().events().is_empty());
    }
}
