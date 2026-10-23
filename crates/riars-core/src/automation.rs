//! Runs the scheduler's due events against the engine.

use std::path::Path;
use std::sync::Mutex;

use chrono::NaiveDateTime;

use crate::audio::engine::{play_index_unlocked, AudioEngine};
use crate::library::db::MediaLibrary;
use crate::playlist::scheduler::{EventAction, ScheduledEvent, Scheduler};

/// Actions handled outside riars-core (streaming lives in riars-stream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamCommand {
    Start,
    Stop,
}

pub struct Fired {
    pub event: ScheduledEvent,
    pub result: Result<(), String>,
}

/// Fire the events due at `now` and run their actions. Blocking (a track may be
/// decoded): call it from a worker thread, about once a second.
pub fn run_due(
    engine: &Mutex<AudioEngine>,
    library: &Mutex<MediaLibrary>,
    scheduler: &Mutex<Scheduler>,
    now: NaiveDateTime,
    stream: &mut dyn FnMut(StreamCommand) -> Result<(), String>,
) -> Vec<Fired> {
    let due = match scheduler.lock() {
        Ok(mut s) => s.take_due(now),
        Err(_) => return Vec::new(),
    };
    due.into_iter()
        .map(|event| {
            let result = run_action(engine, library, &event.action, stream);
            match &result {
                Ok(()) => log::info!("Scheduled event \"{}\" ran", event.name),
                Err(e) => log::warn!("Scheduled event \"{}\" failed: {e}", event.name),
            }
            Fired { event, result }
        })
        .collect()
}

fn run_action(
    engine: &Mutex<AudioEngine>,
    library: &Mutex<MediaLibrary>,
    action: &EventAction,
    stream: &mut dyn FnMut(StreamCommand) -> Result<(), String>,
) -> Result<(), String> {
    let lock = || engine.lock().map_err(|e| e.to_string());
    match action {
        EventAction::PlayTrack { track_path } => {
            // Add it to the playlist and play it now.
            let index = {
                let mut engine = lock()?;
                let title = Path::new(track_path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let playlist = engine.playlist_mut();
                playlist.add_item(track_path.clone(), title, String::new(), 0);
                playlist.len() - 1
            };
            play_index_unlocked(engine, index, false)
        }
        EventAction::SetVolume { level } => {
            lock()?.set_volume(*level);
            Ok(())
        }
        EventAction::FadeToVolume { level, duration_ms } => {
            lock()?.fade_to_volume(*level, *duration_ms);
            Ok(())
        }
        EventAction::StartStreaming => stream(StreamCommand::Start),
        EventAction::StopStreaming => stream(StreamCommand::Stop),
        EventAction::PlayPlaylist { playlist_id } => {
            // Id or name of a saved playlist: put it on air and play it from the top.
            let playlist = library.lock().map_err(|e| e.to_string())?.load_playlist(playlist_id)?;
            if playlist.is_empty() {
                return Err(format!("Saved playlist {playlist_id} is empty"));
            }
            lock()?.set_playlist(playlist);
            play_index_unlocked(engine, 0, false)
        }
    }
}

/// Fire scheduled events about twice a second, forever, saving the schedule when
/// events fired (one-shots disable themselves). Run it on its own thread.
pub fn scheduler_loop(
    engine: &Mutex<AudioEngine>,
    library: &Mutex<MediaLibrary>,
    scheduler: &Mutex<Scheduler>,
    schedule_path: &Path,
    stream: &mut dyn FnMut(StreamCommand) -> Result<(), String>,
) -> ! {
    loop {
        let now = chrono::Local::now().naive_local();
        if !run_due(engine, library, scheduler, now, stream).is_empty() {
            if let Ok(scheduler) = scheduler.lock() {
                if let Err(e) = scheduler.save(schedule_path) {
                    log::warn!("{e}");
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
