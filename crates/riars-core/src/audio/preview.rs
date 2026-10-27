//! Pre-listening (cue/PFL) on a second sound card: one player, its own audio thread,
//! nothing to do with the air (no playlist, no proof of play, no silence alarm), so a
//! problem here can never reach the on-air output.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::output::{AudioOutput, Command, DeviceChoice};
use super::player::TrackPlayer;

/// Where pre-listening plays: the sound card, and what it is playing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PreviewState {
    pub device: Option<String>,
    pub playing: bool,
    pub file: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
}

#[derive(Default)]
pub struct PreviewPlayer {
    device: Option<String>,
    output: Option<AudioOutput>,
    file: Option<String>,
    duration_ms: u64,
}

impl PreviewPlayer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Use this sound card (by name); `None` turns pre-listening off. Fails if the card
    /// is not there: pre-listening never falls back to another card.
    pub fn set_device(&mut self, device: Option<String>) -> Result<(), String> {
        self.output = None;
        self.file = None;
        self.device = device.clone();
        if let Some(name) = device {
            self.output = Some(AudioOutput::open_on(1.0, DeviceChoice::Only(name))?);
        }
        Ok(())
    }

    pub fn device(&self) -> Option<&str> {
        self.device.as_deref()
    }

    /// Play `path` from `from_ms` at `gain_db` (the track's normalisation, so it sounds
    /// as on air). Decodes the file: call it from a worker thread, not the UI one.
    pub fn play(&mut self, path: &Path, from_ms: u64, gain_db: f32) -> Result<(), String> {
        let output = self.output.as_ref().ok_or("No pre-listen sound card chosen")?;
        let mut player = TrackPlayer::from_file(path, 44_100)?;
        player.set_gain(super::normalize::db_to_gain(gain_db));
        self.duration_ms = player.duration().as_millis() as u64;
        output.shared.position_ms.store(0, Ordering::Relaxed);
        output.shared.finished.store(false, Ordering::Relaxed);
        output.send(Command::Load(Box::new(player)));
        if from_ms > 0 {
            output.send(Command::Seek(Duration::from_millis(from_ms)));
            output.shared.position_ms.store(from_ms, Ordering::Relaxed);
        }
        output.shared.playing.store(true, Ordering::Relaxed);
        self.file = Some(path.to_string_lossy().into_owned());
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(output) = &self.output {
            output.shared.playing.store(false, Ordering::Relaxed);
        }
        self.file = None;
    }

    pub fn state(&self) -> PreviewState {
        let (playing, position_ms) = self.output.as_ref().map_or((false, 0), |o| {
            let finished = o.shared.finished.load(Ordering::Relaxed);
            (o.shared.playing.load(Ordering::Relaxed) && !finished && self.file.is_some(), o.shared.position_ms.load(Ordering::Relaxed))
        });
        PreviewState {
            device: self.device.clone(),
            playing,
            file: self.file.clone(),
            position_ms,
            duration_ms: self.duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_card_is_an_error_and_never_the_air() {
        let mut p = PreviewPlayer::new();
        assert!(p.set_device(Some("No such card 1234".into())).is_err());
        assert!(p.play(Path::new("x.wav"), 0, 0.0).is_err(), "nothing plays without a card");
        p.set_device(None).unwrap();
        assert_eq!(p.state(), PreviewState::default());
    }

    /// Lists this machine's sound cards and opens the first one for pre-listening,
    /// without playing anything (run with --ignored --nocapture).
    #[test]
    #[ignore]
    fn real_cards() {
        let cards = crate::audio::output_devices();
        println!("cards: {cards:?}, default: {:?}", crate::audio::default_output_device());
        if let Some(first) = cards.first() {
            let mut p = PreviewPlayer::new();
            p.set_device(Some(first.clone())).unwrap();
            println!("pre-listen opened on {first}: {:?}", p.state());
        }
    }
}
