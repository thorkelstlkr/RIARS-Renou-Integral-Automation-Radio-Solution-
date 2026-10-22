use chrono::{NaiveTime, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventTrigger {
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

pub struct Scheduler {
    events: Vec<ScheduledEvent>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn add_event(&mut self, event: ScheduledEvent) {
        self.events.push(event);
    }

    pub fn remove_event(&mut self, id: &str) -> bool {
        let before = self.events.len();
        self.events.retain(|e| e.id != id);
        self.events.len() < before
    }

    pub fn toggle_event(&mut self, id: &str) -> Option<bool> {
        self.events.iter_mut().find(|e| e.id == id).map(|e| {
            e.enabled = !e.enabled;
            e.enabled
        })
    }

    pub fn events(&self) -> &[ScheduledEvent] {
        &self.events
    }

    pub fn due_events(&self, now: NaiveTime, weekday: Weekday) -> Vec<&ScheduledEvent> {
        self.events
            .iter()
            .filter(|e| e.enabled)
            .filter(|e| match &e.trigger {
                EventTrigger::Time(t) => times_match(*t, now),
                EventTrigger::Daily { time } => times_match(*time, now),
                EventTrigger::Weekly { day, time } => *day == weekday && times_match(*time, now),
                EventTrigger::Interval { .. } => false,
            })
            .collect()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

fn times_match(a: NaiveTime, b: NaiveTime) -> bool {
    a.hour() == b.hour() && a.minute() == b.minute()
}
