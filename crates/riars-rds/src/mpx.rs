//! The built-in FM stereo encoder, as the RDS worker drives it: opens the sound card
//! when the settings ask for it (retrying with backoff when it fails), and passes on
//! new levels and new RDS data without reopening it.

use std::time::{Duration, Instant};

use riars_core::audio::mpx::{MpxMsg, MpxOutput, MpxParams};
use riars_core::audio::StreamTap;
use riars_core::rds::groups::RdsFrame;
use riars_core::rds::{self, RdsConfig, RdsTexts};
use serde::{Deserialize, Serialize};

use crate::{OutputState, FIRST_RETRY, LAST_RETRY};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MpxStatus {
    pub state: OutputState,
    /// The sound card playing the MPX, while it does.
    pub device: Option<String>,
    pub sample_rate: Option<u32>,
    /// Times the on-air audio was missing (silence went out instead).
    pub underruns: u64,
    pub last_error: Option<String>,
}

impl Default for MpxStatus {
    fn default() -> Self {
        Self { state: OutputState::Off, device: None, sample_rate: None, underruns: 0, last_error: None }
    }
}

/// The worker's side of the MPX output. The tap is here while the card is closed and
/// inside `MpxOutput` while it plays.
#[derive(Default)]
pub(crate) struct Mpx {
    tap: Option<StreamTap>,
    output: Option<MpxOutput>,
    /// The card asked for when `output` was opened.
    device: String,
    params: Option<MpxParams>,
    frame: Option<RdsFrame>,
    /// RadioText A/B flag, flipped when the text changes.
    ab: bool,
    rt: Option<String>,
    retry_at: Option<Instant>,
    backoff: Option<Duration>,
    pub status: MpxStatus,
}

impl Mpx {
    pub fn set_tap(&mut self, tap: StreamTap) {
        self.close();
        self.tap = Some(tap);
        self.retry_at = None;
        self.backoff = None;
    }

    fn close(&mut self) {
        if let Some(output) = self.output.take() {
            log::info!("MPX output closed");
            match output.close() {
                Some(tap) => self.tap = Some(tap),
                None => log::warn!("MPX: the audio copy was lost; it comes back when the audio output restarts"),
            }
        }
        self.params = None;
        self.frame = None;
        self.status.device = None;
        self.status.sample_rate = None;
    }

    pub fn step(&mut self, config: &RdsConfig, texts: &RdsTexts) {
        let wanted = &config.mpx;
        if !wanted.enabled {
            self.close();
            self.retry_at = None;
            self.backoff = None;
            self.status.state = OutputState::Off;
            self.status.last_error = None;
            return;
        }
        if self.output.is_some() && self.device != wanted.device {
            self.close();
            self.retry_at = None;
        }
        if self.output.as_ref().is_some_and(|o| o.stats().failed.load(std::sync::atomic::Ordering::Relaxed)) {
            log::warn!("MPX output: the sound card failed; reopening");
            self.close();
            self.fail("The sound card stopped (unplugged or taken by another program)".to_string());
        }
        if self.rt.as_deref() != Some(texts.rt.as_str()) {
            if self.rt.is_some() {
                self.ab = !self.ab;
            }
            self.rt = Some(texts.rt.clone());
        }
        let params = wanted.params();
        let frame = rds::rds_frame(config, texts, self.ab);
        match &mut self.output {
            Some(output) => {
                // A full queue means the callback stopped taking messages: try again next pass.
                if self.params != Some(params) && output.send(MpxMsg::Params(params)) {
                    self.params = Some(params);
                }
                if self.frame != Some(frame) && output.send(MpxMsg::Rds(frame)) {
                    self.frame = Some(frame);
                }
                self.status.underruns = output.stats().underruns.load(std::sync::atomic::Ordering::Relaxed);
            }
            None => self.open(&wanted.device, params, frame),
        }
    }

    fn open(&mut self, device: &str, params: MpxParams, frame: RdsFrame) {
        let now = Instant::now();
        if self.retry_at.is_some_and(|at| now < at) {
            return;
        }
        let Some(tap) = self.tap.take() else {
            self.status.state = OutputState::Error;
            self.status.last_error = Some("No audio to encode: the audio output is not running".to_string());
            return;
        };
        let name = Some(device).filter(|d| !d.is_empty());
        match MpxOutput::open(tap, name, params, frame) {
            Ok(output) => {
                self.device = device.to_string();
                self.params = Some(params);
                self.frame = Some(frame);
                self.retry_at = None;
                self.backoff = None;
                self.status = MpxStatus {
                    state: OutputState::Ok,
                    device: Some(output.device_name().to_string()),
                    sample_rate: Some(output.sample_rate()),
                    underruns: 0,
                    last_error: None,
                };
                self.output = Some(output);
            }
            Err((e, tap)) => {
                if self.status.state != OutputState::Error {
                    log::warn!("MPX output: {e}");
                }
                // A stand-in tap (the real one was lost) is fed by nothing: wait for a new one.
                self.tap = Some(tap).filter(|t| !t.consumer.is_abandoned());
                self.fail(e);
            }
        }
    }

    /// Retry later, waiting longer each time.
    fn fail(&mut self, error: String) {
        let backoff = self.backoff.unwrap_or(FIRST_RETRY);
        self.retry_at = Some(Instant::now() + backoff);
        self.backoff = Some((backoff * 2).min(LAST_RETRY));
        self.status.state = OutputState::Error;
        self.status.last_error = Some(error);
    }
}
