//! Live broadcasting on the output thread: the microphone and the sound bank (cartwall).
//!
//! The microphone is captured by a cpal input stream on its own thread and reaches the
//! output callback through a lock-free ring, like decoded tracks. Its processing (high
//! pass, gain, gate, compressor, on/off ramp) and the bank voices run inside the output
//! callback: no locks, no allocation, no I/O. If the input card fails or goes away the
//! microphone just falls silent; the output never stops for it.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use crossbeam_channel::{bounded, Sender};
use serde::{Deserialize, Serialize};

use super::player::TrackPlayer;

/// Sound-bank voices that can play at once.
pub const MAX_CART_VOICES: usize = 8;
/// Frames the live layer handles per pass; longer buffers are done in several passes.
pub(crate) const LIVE_BLOCK: usize = 4096;
/// Bank voices lower the music by this much while they play, when set to duck.
const CART_DUCK: f32 = 0.5; // −6 dB
/// A stopped bank sound fades out over this long instead of cutting with a click.
const CART_STOP_MS: f64 = 30.0;
/// Opening and closing the microphone.
const MIC_RAMP_MS: f64 = 150.0;
/// Microphone audio waiting in the ring: aim for this, and drop the excess beyond the
/// maximum (the input and output cards run on different clocks).
const MIC_TARGET_MS: f64 = 20.0;
const MIC_MAX_MS: f64 = 80.0;
/// Level above which the microphone counts as speaking (for ducking while speaking),
/// and how long it still counts after the voice stops.
const SPEAKING_LEVEL: f32 = 0.01; // −40 dBFS
const SPEAKING_HOLD_MS: f64 = 600.0;

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// When the music goes down for the microphone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MicDuck {
    /// The whole time the microphone is open.
    #[default]
    WhileOpen,
    /// Only while someone speaks into it.
    WhileSpeaking,
}

/// Microphone settings, saved with the session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MicConfig {
    /// Input card by name; `None` is the system default.
    pub device: Option<String>,
    pub gain_db: f32,
    /// Cut rumble and pops below 80 Hz.
    pub high_pass: bool,
    /// Silence the microphone below `gate_threshold_db` (room noise between words).
    pub gate: bool,
    pub gate_threshold_db: f32,
    /// Even out loud and quiet speech.
    pub compressor: bool,
    /// How far the music goes down (dB, 0 = not at all).
    pub duck_db: f32,
    pub duck: MicDuck,
    /// Let the API and AI agents open the microphone. Off: only the operator can.
    pub allow_remote: bool,
}

impl Default for MicConfig {
    fn default() -> Self {
        Self {
            device: None,
            gain_db: 0.0,
            high_pass: true,
            gate: false,
            gate_threshold_db: -50.0,
            compressor: true,
            duck_db: 12.0,
            duck: MicDuck::WhileOpen,
            allow_remote: false,
        }
    }
}

impl MicConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(-20.0..=30.0).contains(&self.gain_db) {
            return Err("Microphone gain must be between -20 and 30 dB".to_string());
        }
        if !(-80.0..=-10.0).contains(&self.gate_threshold_db) {
            return Err("Gate threshold must be between -80 and -10 dBFS".to_string());
        }
        if !(0.0..=40.0).contains(&self.duck_db) {
            return Err("Music ducking must be between 0 and 40 dB".to_string());
        }
        Ok(())
    }
}

/// Microphone controls read by the output callback.
pub(crate) struct MicControls {
    /// The operator wants the microphone on air.
    pub on: AtomicBool,
    gain_bits: AtomicU32,
    high_pass: AtomicBool,
    gate: AtomicBool,
    gate_level_bits: AtomicU32,
    compressor: AtomicBool,
    /// Gain of the music while ducked (1 = no ducking).
    duck_gain_bits: AtomicU32,
    duck_while_speaking: AtomicBool,
    /// Loudest sample since the last read, as f32 bits (see `Shared::peak_bits`).
    pub peak_bits: AtomicU32,
    pub speaking: AtomicBool,
    /// Times the ring ran dry (the input card stopped or is late).
    pub underruns: AtomicU64,
}

impl MicControls {
    pub fn new() -> Self {
        let controls = Self {
            on: AtomicBool::new(false),
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            high_pass: AtomicBool::new(true),
            gate: AtomicBool::new(false),
            gate_level_bits: AtomicU32::new(0.0f32.to_bits()),
            compressor: AtomicBool::new(true),
            duck_gain_bits: AtomicU32::new(1.0f32.to_bits()),
            duck_while_speaking: AtomicBool::new(false),
            peak_bits: AtomicU32::new(0),
            speaking: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
        };
        controls.apply(&MicConfig::default());
        controls
    }

    pub fn apply(&self, config: &MicConfig) {
        self.gain_bits.store(db_to_gain(config.gain_db).to_bits(), Ordering::Relaxed);
        self.high_pass.store(config.high_pass, Ordering::Relaxed);
        self.gate.store(config.gate, Ordering::Relaxed);
        self.gate_level_bits.store(db_to_gain(config.gate_threshold_db).to_bits(), Ordering::Relaxed);
        self.compressor.store(config.compressor, Ordering::Relaxed);
        self.duck_gain_bits.store(db_to_gain(-config.duck_db).to_bits(), Ordering::Relaxed);
        self.duck_while_speaking.store(config.duck == MicDuck::WhileSpeaking, Ordering::Relaxed);
    }

    /// Read and reset the microphone peak.
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak_bits.swap(0, Ordering::Relaxed))
    }

    fn f32(bits: &AtomicU32) -> f32 {
        f32::from_bits(bits.load(Ordering::Relaxed))
    }
}

/// What the bank is playing, for the control side: per voice slot, the serial of the
/// voice (0 = free), its button and the milliseconds it has left.
pub(crate) struct CartStatus {
    pub serial: [AtomicU64; MAX_CART_VOICES],
    pub button: [AtomicU32; MAX_CART_VOICES],
    pub left_ms: [AtomicU32; MAX_CART_VOICES],
}

impl CartStatus {
    pub fn new() -> Self {
        Self {
            serial: std::array::from_fn(|_| AtomicU64::new(0)),
            button: std::array::from_fn(|_| AtomicU32::new(0)),
            left_ms: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}

// --- Microphone input -------------------------------------------------------------

/// Names of the cards that can record.
pub fn input_devices() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

pub fn default_input_device() -> Option<String> {
    cpal::default_host().default_input_device().and_then(|d| d.name().ok())
}

/// The capture side of the microphone: a cpal input stream kept on its own thread
/// (cpal streams are not `Send`). Dropping it stops the capture.
pub(crate) struct MicInput {
    pub device_name: String,
    /// Cleared by the stream's error callback.
    pub alive: Arc<AtomicBool>,
    _shutdown: Sender<()>,
    _thread: JoinHandle<()>,
}

/// The output-thread side of the microphone: the ring it reads and its processing state.
pub(crate) struct MicFeed {
    consumer: rtrb::Consumer<f32>,
    in_rate: f64,
    /// Resampling: the two input samples around the read position, and where it is.
    prev: f32,
    cur: f32,
    pos: f64,
    /// Enough audio collected to start (after start-up or a dry spell).
    primed: bool,
    hp_x: f32,
    hp_y: f32,
    env: f32,
    gate_gain: f32,
    comp_gain: f32,
    open: f32,
    speaking_left: u32,
}

impl MicInput {
    /// Start capturing from `device` (or the default input card).
    pub fn open(device: Option<&str>) -> Result<(Self, MicFeed), String> {
        let (ready_tx, ready_rx) = bounded::<Result<(String, MicFeed), String>>(1);
        let (shutdown_tx, shutdown_rx) = bounded::<()>(0);
        let alive = Arc::new(AtomicBool::new(true));
        let stream_alive = Arc::clone(&alive);
        let wanted = device.map(str::to_string);
        let thread = std::thread::Builder::new()
            .name("riars-mic".into())
            .spawn(move || match start_input(wanted.as_deref(), stream_alive) {
                Ok((stream, name, feed)) => {
                    let _ = ready_tx.send(Ok((name, feed)));
                    let _ = shutdown_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| format!("Cannot start the microphone thread: {e}"))?;
        let (device_name, feed) = ready_rx
            .recv()
            .map_err(|_| "The microphone thread exited during start-up".to_string())??;
        Ok((Self { device_name, alive, _shutdown: shutdown_tx, _thread: thread }, feed))
    }
}

fn start_input(name: Option<&str>, alive: Arc<AtomicBool>) -> Result<(cpal::Stream, String, MicFeed), String> {
    let host = cpal::default_host();
    let device = match name {
        Some(wanted) => host
            .input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().is_ok_and(|n| n == wanted))
            .ok_or_else(|| format!("Input card \"{wanted}\" not found"))?,
        None => host.default_input_device().ok_or("No input card found")?,
    };
    let device_name = device.name().unwrap_or_else(|_| "unknown".into());
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Cannot query the input card: {e}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate.0;
    // Half a second of room; the callback keeps it near MIC_TARGET_MS.
    let (producer, consumer) = rtrb::RingBuffer::new((rate as usize / 2).max(1024));
    let channels = config.channels as usize;
    let stream = match format {
        cpal::SampleFormat::F32 => build_input::<f32>(&device, &config, producer, channels, alive),
        cpal::SampleFormat::I16 => build_input::<i16>(&device, &config, producer, channels, alive),
        cpal::SampleFormat::U16 => build_input::<u16>(&device, &config, producer, channels, alive),
        cpal::SampleFormat::I32 => build_input::<i32>(&device, &config, producer, channels, alive),
        other => Err(format!("Unsupported input sample format: {other}")),
    }?;
    stream.play().map_err(|e| format!("Cannot start the microphone: {e}"))?;
    log::info!("Microphone: {device_name}, {rate} Hz, {channels} channels, {format}");
    Ok((stream, device_name, MicFeed::new(consumer, rate)))
}

fn build_input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: rtrb::Producer<f32>,
    channels: usize,
    alive: Arc<AtomicBool>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels.max(1);
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                // Mono: the average of the channels. When the ring is full the newest
                // audio is dropped (the output side is not reading).
                for frame in data.chunks(channels) {
                    let sum: f32 = frame.iter().map(|s| f32::from_sample_(*s)).sum();
                    let _ = producer.push(sum / channels as f32);
                }
            },
            move |err| {
                log::error!("Microphone error: {err}");
                alive.store(false, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| format!("Cannot open the microphone: {e}"))
}

impl MicFeed {
    pub(crate) fn new(consumer: rtrb::Consumer<f32>, in_rate: u32) -> Self {
        Self {
            consumer,
            in_rate: in_rate.max(1) as f64,
            prev: 0.0,
            cur: 0.0,
            pos: 0.0,
            primed: false,
            hp_x: 0.0,
            hp_y: 0.0,
            env: 0.0,
            gate_gain: 1.0,
            comp_gain: 1.0,
            open: 0.0,
            speaking_left: 0,
        }
    }

    /// Fill `out` (mono, one value per output frame) with the processed microphone.
    /// Real-time safe.
    pub(crate) fn render(&mut self, out: &mut [f32], out_rate: f64, controls: &MicControls) {
        let ms = |ms: f64| (ms * out_rate / 1000.0).max(1.0);
        let target = (MIC_TARGET_MS * self.in_rate / 1000.0) as usize;
        let max = (MIC_MAX_MS * self.in_rate / 1000.0) as usize;
        let waiting = self.consumer.slots();
        if waiting > max {
            // Behind (clock drift, a stall): drop the oldest audio, keep the target.
            if let Ok(chunk) = self.consumer.read_chunk(waiting - target) {
                chunk.commit_all();
            }
        }
        if !self.primed && self.consumer.slots() >= target {
            self.primed = true;
        }

        let step = self.in_rate / out_rate;
        let high_pass = controls.high_pass.load(Ordering::Relaxed);
        // One-pole high pass at 80 Hz.
        let rc = 1.0 / (2.0 * std::f64::consts::PI * 80.0);
        let hp_a = (rc / (rc + 1.0 / out_rate)) as f32;
        let gain = MicControls::f32(&controls.gain_bits);
        let gate = controls.gate.load(Ordering::Relaxed);
        let gate_level = MicControls::f32(&controls.gate_level_bits);
        let compressor = controls.compressor.load(Ordering::Relaxed);
        let open_target = if controls.on.load(Ordering::Relaxed) { 1.0 } else { 0.0 };
        let open_step = (1.0 / ms(MIC_RAMP_MS)) as f32;
        let env_attack = 1.0 - (-1.0 / ms(5.0)).exp() as f32;
        let env_release = 1.0 - (-1.0 / ms(150.0)).exp() as f32;
        let gate_open_step = (1.0 / ms(5.0)) as f32;
        let gate_close_step = (1.0 / ms(200.0)) as f32;
        let hold = ms(SPEAKING_HOLD_MS) as u32;
        let mut peak = 0.0f32;

        for value in out.iter_mut() {
            self.pos += step;
            while self.pos >= 1.0 {
                self.pos -= 1.0;
                self.prev = self.cur;
                self.cur = if self.primed {
                    match self.consumer.pop() {
                        Ok(sample) => sample,
                        Err(_) => {
                            // Dry: wait for the target again rather than crackle.
                            self.primed = false;
                            controls.underruns.fetch_add(1, Ordering::Relaxed);
                            0.0
                        }
                    }
                } else {
                    0.0
                };
            }
            let mut x = self.prev + (self.cur - self.prev) * self.pos as f32;
            if high_pass {
                let y = hp_a * (self.hp_y + x - self.hp_x);
                self.hp_x = x;
                self.hp_y = y;
                x = y;
            }
            x *= gain;
            let level = x.abs();
            let k = if level > self.env { env_attack } else { env_release };
            self.env += (level - self.env) * k;
            if gate {
                let target = if self.env >= gate_level { 1.0 } else { 0.0 };
                let step = if target > self.gate_gain { gate_open_step } else { gate_close_step };
                self.gate_gain += (target - self.gate_gain).clamp(-step, step);
                x *= self.gate_gain;
            }
            if compressor {
                // 3:1 above −18 dBFS, with 4 dB of make-up gain.
                const THRESHOLD: f32 = 0.126;
                let wanted = if self.env > THRESHOLD { (THRESHOLD / self.env).powf(2.0 / 3.0) } else { 1.0 };
                self.comp_gain += (wanted - self.comp_gain) * if wanted < self.comp_gain { env_attack } else { env_release };
                x *= self.comp_gain * 1.585;
            }
            if self.env >= SPEAKING_LEVEL {
                self.speaking_left = hold;
            } else {
                self.speaking_left = self.speaking_left.saturating_sub(1);
            }
            peak = peak.max(x.abs());
            self.open += (open_target - self.open).clamp(-open_step, open_step);
            *value = x * self.open;
        }
        controls.peak_bits.fetch_max(peak.to_bits(), Ordering::Relaxed);
        controls.speaking.store(self.speaking_left > 0, Ordering::Relaxed);
    }

    /// The microphone is on air or still fading.
    pub(crate) fn audible(&self, controls: &MicControls) -> bool {
        self.open > 0.0 || controls.on.load(Ordering::Relaxed)
    }

    /// Gain the music should head for because of the microphone (1 = none).
    pub(crate) fn duck(&self, controls: &MicControls) -> f32 {
        let on = controls.on.load(Ordering::Relaxed);
        let speaking = !controls.duck_while_speaking.load(Ordering::Relaxed) || self.speaking_left > 0;
        if on && speaking { MicControls::f32(&controls.duck_gain_bits) } else { 1.0 }
    }
}

// --- Sound bank -------------------------------------------------------------------

/// A sound-bank sound handed to the output thread.
pub(crate) struct CartVoice {
    pub player: Box<TrackPlayer>,
    pub button: u32,
    /// Unique per press, so the control side can tell one play from the next.
    pub serial: u64,
    pub looping: bool,
    pub duck: bool,
    pub gain: f32,
}

struct Playing {
    voice: CartVoice,
    /// 1 while playing; falls to 0 once stopped, then the voice is retired.
    fade: f32,
    stopping: bool,
}

/// The voices of the bank playing now. Lives on the output thread.
pub(crate) struct Carts {
    voices: [Option<Playing>; MAX_CART_VOICES],
}

impl Default for Carts {
    fn default() -> Self {
        Self { voices: std::array::from_fn(|_| None) }
    }
}

impl Carts {
    /// Start a voice in a free slot; with none free, the oldest one makes room.
    pub fn start(&mut self, voice: CartVoice, retire: &mut impl FnMut(Box<TrackPlayer>)) {
        let slot = match self.voices.iter().position(Option::is_none) {
            Some(free) => free,
            None => {
                let oldest = (0..MAX_CART_VOICES)
                    .min_by_key(|&i| self.voices[i].as_ref().map_or(0, |p| p.voice.serial))
                    .unwrap_or(0);
                if let Some(old) = self.voices[oldest].take() {
                    retire(old.voice.player);
                }
                oldest
            }
        };
        self.voices[slot] = Some(Playing { voice, fade: 1.0, stopping: false });
    }

    /// Fade out the voices of `button` (all voices with `None`).
    pub fn stop(&mut self, button: Option<u32>) {
        for playing in self.voices.iter_mut().flatten() {
            if button.is_none_or(|b| b == playing.voice.button) {
                playing.stopping = true;
            }
        }
    }

    pub fn is_active(&self) -> bool {
        self.voices.iter().any(Option::is_some)
    }

    /// Gain the music should head for because of the bank (1 = none).
    pub fn duck(&self) -> f32 {
        let ducking = self.voices.iter().flatten().any(|p| p.voice.duck && !p.stopping);
        if ducking { CART_DUCK } else { 1.0 }
    }

    /// Add the voices into `out` (interleaved, `channels` per frame). Real-time safe:
    /// finished voices go to `retire`.
    pub fn mix(&mut self, out: &mut [f32], channels: usize, out_rate: f64, retire: &mut impl FnMut(Box<TrackPlayer>)) {
        let fade_step = (1.0 / (CART_STOP_MS * out_rate / 1000.0)) as f32;
        for slot in self.voices.iter_mut() {
            let Some(playing) = slot else { continue };
            let step = playing.voice.player.sample_rate() as f64 / out_rate;
            for frame in out.chunks_mut(channels) {
                if playing.voice.player.is_finished() {
                    if playing.voice.looping && !playing.stopping {
                        playing.voice.player.seek(std::time::Duration::ZERO);
                    } else {
                        break;
                    }
                }
                if playing.stopping {
                    playing.fade -= fade_step;
                    if playing.fade <= 0.0 {
                        break;
                    }
                }
                let gain = playing.voice.gain * playing.fade;
                for (channel, value) in frame.iter_mut().enumerate() {
                    *value += playing.voice.player.sample(channel) * gain;
                }
                playing.voice.player.advance(step);
            }
            let done = playing.fade <= 0.0 || (playing.voice.player.is_finished() && !(playing.voice.looping && !playing.stopping));
            if done {
                if let Some(old) = slot.take() {
                    retire(old.voice.player);
                }
            }
        }
    }

    /// Publish the voices for the control side.
    pub fn publish(&self, status: &CartStatus) {
        for (i, slot) in self.voices.iter().enumerate() {
            match slot {
                Some(playing) => {
                    let player = &playing.voice.player;
                    let left = player.remaining_frames() / player.sample_rate().max(1) as f64 * 1000.0;
                    status.serial[i].store(playing.voice.serial, Ordering::Relaxed);
                    status.button[i].store(playing.voice.button, Ordering::Relaxed);
                    status.left_ms[i].store(if playing.voice.looping { u32::MAX } else { left as u32 }, Ordering::Relaxed);
                }
                None => status.serial[i].store(0, Ordering::Relaxed),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sound(level: f32, frames: usize) -> Box<TrackPlayer> {
        Box::new(TrackPlayer::from_samples(PathBuf::from("c.wav"), vec![level; frames], 1, 1000))
    }

    fn voice(button: u32, serial: u64, level: f32, frames: usize) -> CartVoice {
        CartVoice { player: sound(level, frames), button, serial, looping: false, duck: false, gain: 1.0 }
    }

    #[test]
    fn carts_mix_stop_with_a_fade_and_make_room() {
        let mut carts = Carts::default();
        let mut retired = 0;
        carts.start(voice(1, 1, 0.25, 100), &mut |_| retired += 1);
        carts.start(voice(2, 2, 0.5, 100), &mut |_| retired += 1);
        let mut out = vec![0.0f32; 10];
        carts.mix(&mut out, 1, 1000.0, &mut |_| retired += 1);
        assert!(out.iter().all(|v| (v - 0.75).abs() < 1e-6), "{out:?}");

        // Stopping fades over 30 ms, then the voice is retired.
        carts.stop(Some(1));
        let mut out = vec![0.0f32; 40];
        carts.mix(&mut out, 1, 1000.0, &mut |_| retired += 1);
        assert!(out[0] > 0.7 && out[0] < 0.75 && (out[39] - 0.5).abs() < 1e-6, "{out:?}");
        assert_eq!(retired, 1);

        // Full bank: the oldest voice makes room.
        for serial in 3..=10 {
            carts.start(voice(serial as u32, serial, 0.1, 100), &mut |_| retired += 1);
        }
        assert_eq!(retired, 2);
        assert!(carts.voices.iter().flatten().all(|p| p.voice.serial != 2));
    }

    #[test]
    fn loops_until_stopped_and_ducking_follows_the_voices() {
        let mut carts = Carts::default();
        let mut retired = 0;
        let mut looped = voice(1, 1, 0.5, 10);
        looped.looping = true;
        looped.duck = true;
        carts.start(looped, &mut |_| retired += 1);
        assert_eq!(carts.duck(), CART_DUCK);
        let mut out = vec![0.0f32; 35];
        carts.mix(&mut out, 1, 1000.0, &mut |_| retired += 1);
        assert!(out.iter().all(|v| (v - 0.5).abs() < 1e-6), "a 10-frame sound loops: {out:?}");
        carts.stop(None);
        assert_eq!(carts.duck(), 1.0);
        let mut out = vec![0.0f32; 40];
        carts.mix(&mut out, 1, 1000.0, &mut |_| retired += 1);
        assert!(!carts.is_active());
        assert_eq!(retired, 1);
    }

    /// Render `frames` output frames at 2 kHz from a 1 kHz input fed at the same pace,
    /// like a real card: half as many input samples, plus the 20 ms the feed keeps.
    fn run(feed: &mut MicFeed, producer: &mut rtrb::Producer<f32>, controls: &MicControls, frames: usize, level: f32) -> Vec<f32> {
        let mut out = vec![0.0f32; frames];
        for chunk in out.chunks_mut(20) {
            while feed.consumer.slots() < 30 {
                producer.push(level).unwrap();
            }
            feed.render(chunk, 2000.0, controls);
        }
        out
    }

    #[test]
    fn microphone_resamples_ramps_and_ducks() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(4096);
        let mut feed = MicFeed::new(consumer, 1000);
        let controls = MicControls::new();
        controls.apply(&MicConfig { high_pass: false, compressor: false, ..MicConfig::default() });
        // Closed: silent, but the meter still reads the voice.
        let out = run(&mut feed, &mut producer, &controls, 200, 0.5);
        assert!(out.iter().all(|v| *v == 0.0));
        assert!(controls.take_peak() > 0.4);
        assert_eq!(feed.duck(&controls), 1.0);
        assert_eq!(controls.underruns.load(Ordering::Relaxed), 0);

        // Open: ramps up over 150 ms (300 frames at 2 kHz), then passes the voice.
        controls.on.store(true, Ordering::Relaxed);
        let out = run(&mut feed, &mut producer, &controls, 400, 0.5);
        assert!(out[10] > 0.0 && out[10] < 0.1, "{}", out[10]);
        assert!((out[399] - 0.5).abs() < 1e-3, "{}", out[399]);
        assert!((feed.duck(&controls) - db_to_gain(-12.0)).abs() < 1e-6);
        assert!(controls.speaking.load(Ordering::Relaxed));

        // Ducking only while speaking: after the hold of silence the music comes back.
        controls.apply(&MicConfig { duck: MicDuck::WhileSpeaking, high_pass: false, compressor: false, ..MicConfig::default() });
        assert!(feed.duck(&controls) < 1.0, "still speaking");
        run(&mut feed, &mut producer, &controls, 3000, 0.0); // ~0.6 s of decay plus the 0.6 s hold
        assert_eq!(feed.duck(&controls), 1.0);
        assert!(!controls.speaking.load(Ordering::Relaxed));

        // A card that stops sending: silence, counted, never a panic.
        let mut out = vec![1.0f32; 400];
        feed.render(&mut out, 2000.0, &controls);
        assert!(controls.underruns.load(Ordering::Relaxed) > 0);
        assert!(out[399].abs() < 1e-6);
    }

    #[test]
    fn microphone_drops_audio_it_is_late_for() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(4096);
        let mut feed = MicFeed::new(consumer, 1000);
        let controls = MicControls::new();
        for _ in 0..1000 {
            producer.push(0.1).unwrap(); // a second waiting, far over the 80 ms maximum
        }
        let mut out = vec![0.0f32; 10];
        feed.render(&mut out, 1000.0, &controls);
        assert!(feed.consumer.slots() <= 20, "{}", feed.consumer.slots());
    }
}
