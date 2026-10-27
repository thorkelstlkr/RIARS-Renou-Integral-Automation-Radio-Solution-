//! Sound card output through cpal.
//!
//! The cpal stream lives on its own thread (it is not `Send`). The engine talks to the
//! audio callback through a bounded crossbeam channel for track changes and atomics for
//! play state, volume and position, so the callback never locks, allocates or does I/O.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use crossbeam_channel::{bounded, Receiver, Sender};

use super::crossfade::{CrossfadeConfig, FadeCurve};
use super::live::{CartStatus, CartVoice, Carts, MicControls, MicFeed, LIVE_BLOCK};
use super::player::TrackPlayer;

pub(crate) enum Command {
    Load(Box<TrackPlayer>),
    /// Play this track the moment the current one ends (gapless).
    Queue(Box<TrackPlayer>),
    Seek(Duration),
    /// Start reading this microphone (or stop reading it with `None`).
    Mic(Option<Box<MicFeed>>),
    /// Play a sound-bank sound over everything.
    CartPlay(CartVoice),
    /// Fade out the bank sounds of a button (all of them with `None`).
    CartStop(Option<u32>),
}

/// State shared between the engine and the audio callback.
pub(crate) struct Shared {
    pub playing: AtomicBool,
    /// Set by the callback when the loaded track plays to the end.
    pub finished: AtomicBool,
    /// Target volume; the callback moves towards it at `ramp_ms_per_unit`.
    volume_bits: AtomicU32,
    /// Milliseconds to move the volume by 1.0; 0 jumps straight to the target.
    ramp_ms_per_unit: AtomicU32,
    pub position_ms: AtomicU64,
    /// Incremented by the callback each time it switches to a queued track.
    pub switches: AtomicU64,
    /// Overlays (imaging over a track) started and ended so far.
    pub overlays_started: AtomicU64,
    pub overlays_ended: AtomicU64,
    /// Crossfade length in ms between queued tracks; 0 disables it.
    crossfade_ms: AtomicU64,
    crossfade_curve: AtomicU8,
    /// Highest |sample| per side (left, right) since the last `take_levels`, as f32
    /// bits: for non-negative floats the bit patterns order like the values, so
    /// `fetch_max` on the bits keeps the loudest peak.
    peak_bits: [AtomicU32; 2],
    /// Samples at or beyond full scale since the last `take_levels`.
    clipped: AtomicU32,
    /// Consecutive output frames of silence while playing (dead air), measured before
    /// the volume so turning it down is not silence. Reset by sound or by stopping.
    pub silent_frames: AtomicU64,
    /// Below this linear level (pre-volume) a block counts as silent; 0 disables.
    silence_level_bits: AtomicU32,
    /// Output sample rate, to turn `silent_frames` into seconds.
    pub out_rate: AtomicU32,
    pub mic: MicControls,
    pub carts: CartStatus,
}

/// Counted as clipping: the loudest 16-bit sample (32767/32768), since material at
/// full scale never reaches exactly 1.0.
const CLIP_LEVEL: f32 = 32767.0 / 32768.0;

/// Output levels since the previous read: linear peaks (1.0 = full scale).
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct Levels {
    pub peak_left: f32,
    pub peak_right: f32,
    /// Samples that hit full scale.
    pub clipped: u32,
}

impl Shared {
    fn new(volume: f32) -> Self {
        Self {
            playing: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            volume_bits: AtomicU32::new(volume.to_bits()),
            ramp_ms_per_unit: AtomicU32::new(0),
            position_ms: AtomicU64::new(0),
            switches: AtomicU64::new(0),
            overlays_started: AtomicU64::new(0),
            overlays_ended: AtomicU64::new(0),
            crossfade_ms: AtomicU64::new(0),
            crossfade_curve: AtomicU8::new(0),
            peak_bits: [AtomicU32::new(0), AtomicU32::new(0)],
            clipped: AtomicU32::new(0),
            silent_frames: AtomicU64::new(0),
            silence_level_bits: AtomicU32::new(0.0f32.to_bits()),
            out_rate: AtomicU32::new(44_100),
            mic: MicControls::new(),
            carts: CartStatus::new(),
        }
    }

    /// Linear level under which output counts as silence (0 turns detection off).
    pub fn set_silence_level(&self, level: f32) {
        self.silence_level_bits.store(level.max(0.0).to_bits(), Ordering::Relaxed);
        self.silent_frames.store(0, Ordering::Relaxed);
    }

    /// Read and reset the levels measured since the last call.
    pub fn take_levels(&self) -> Levels {
        Levels {
            peak_left: f32::from_bits(self.peak_bits[0].swap(0, Ordering::Relaxed)),
            peak_right: f32::from_bits(self.peak_bits[1].swap(0, Ordering::Relaxed)),
            clipped: self.clipped.swap(0, Ordering::Relaxed),
        }
    }

    pub fn set_crossfade(&self, config: &CrossfadeConfig) {
        let ms = if config.enabled { config.duration.as_millis() as u64 } else { 0 };
        let curve = match config.curve {
            FadeCurve::Linear => 0,
            FadeCurve::EqualPower => 1,
            FadeCurve::SCurve => 2,
        };
        self.crossfade_curve.store(curve, Ordering::Relaxed);
        self.crossfade_ms.store(ms, Ordering::Relaxed);
    }

    fn crossfade(&self) -> CrossfadeConfig {
        let ms = self.crossfade_ms.load(Ordering::Relaxed);
        CrossfadeConfig {
            duration: Duration::from_millis(ms),
            curve: match self.crossfade_curve.load(Ordering::Relaxed) {
                0 => FadeCurve::Linear,
                2 => FadeCurve::SCurve,
                _ => FadeCurve::EqualPower,
            },
            enabled: ms > 0,
        }
    }

    pub fn set_volume(&self, volume: f32) {
        self.ramp_ms_per_unit.store(0, Ordering::Relaxed);
        self.volume_bits.store(volume.to_bits(), Ordering::Relaxed);
    }

    /// Glide to `volume` at `ms_per_unit` milliseconds per 1.0 of change.
    pub fn ramp_volume(&self, volume: f32, ms_per_unit: u32) {
        self.ramp_ms_per_unit.store(ms_per_unit, Ordering::Relaxed);
        self.volume_bits.store(volume.to_bits(), Ordering::Relaxed);
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }
}

/// A copy of everything sent to the sound card, for streaming. Interleaved f32 at the
/// device's rate and channel count. Whoever reads it should keep up: when it is full,
/// new audio is dropped.
pub struct StreamTap {
    pub consumer: rtrb::Consumer<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Device name reported when running on the internal clock.
pub const NO_DEVICE: &str = "none (internal clock)";

pub(crate) struct AudioOutput {
    commands: Sender<Command>,
    /// Players replaced in the callback come back here to be freed off the audio thread.
    garbage: Receiver<Box<TrackPlayer>>,
    mic_garbage: Receiver<Box<MicFeed>>,
    pub shared: Arc<Shared>,
    device_name: String,
    tap: Option<StreamTap>,
    // Dropping the sender wakes the output thread so it drops the stream and exits.
    _shutdown: Sender<()>,
    _thread: JoinHandle<()>,
}

impl AudioOutput {
    /// Open the default output device and start a silent stream.
    /// Open the default output device, or run on the internal clock when
    /// `use_device` is false or no device works.
    /// Open on a given sound card. A named card that is missing falls back to the
    /// default one (with a warning); `Clock` never touches a card.
    pub fn open_on(volume: f32, choice: DeviceChoice) -> Result<Self, String> {
        let use_device = !matches!(choice, DeviceChoice::Clock);
        let strict = matches!(choice, DeviceChoice::Only(_));
        let name = match &choice {
            DeviceChoice::Named(n) | DeviceChoice::Only(n) => Some(n.clone()),
            _ => None,
        };
        let (cmd_tx, cmd_rx) = bounded::<Command>(16);
        // Room for a whole bank of voices retired in one buffer.
        let (garbage_tx, garbage_rx) = bounded::<Box<TrackPlayer>>(32);
        let (mic_garbage_tx, mic_garbage_rx) = bounded::<Box<MicFeed>>(4);
        let (shutdown_tx, shutdown_rx) = bounded::<()>(0);
        let (ready_tx, ready_rx) = bounded::<Result<(String, StreamTap), String>>(1);
        let shared = Arc::new(Shared::new(volume));
        let callback_shared = Arc::clone(&shared);

        let parts = Parts { commands: cmd_rx, garbage: garbage_tx, mic_garbage: mic_garbage_tx, shared: callback_shared };
        let thread = std::thread::Builder::new()
            .name("riars-audio-output".into())
            .spawn(move || {
                if use_device {
                    match start_stream(parts.clone(), name.as_deref(), strict) {
                        Ok((stream, device_name, tap)) => {
                            let _ = ready_tx.send(Ok((device_name, tap)));
                            let _ = shutdown_rx.recv();
                            drop(stream);
                            return;
                        }
                        Err(e) if strict => {
                            let _ = ready_tx.send(Err(e));
                            return;
                        }
                        Err(e) => log::warn!("No sound card ({e}); using the internal clock"),
                    }
                }
                let (renderer, tap) = Renderer::new(parts, CLOCK_RATE, 2);
                let _ = ready_tx.send(Ok((NO_DEVICE.to_string(), tap)));
                run_clock(renderer, shutdown_rx);
            })
            .map_err(|e| format!("Cannot start audio thread: {e}"))?;

        let (device_name, tap) = ready_rx
            .recv()
            .map_err(|_| "Audio thread exited during start-up".to_string())??;

        Ok(Self {
            commands: cmd_tx,
            garbage: garbage_rx,
            mic_garbage: mic_garbage_rx,
            shared,
            device_name,
            tap: Some(tap),
            _shutdown: shutdown_tx,
            _thread: thread,
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn take_tap(&mut self) -> Option<StreamTap> {
        self.tap.take()
    }

    /// Free players the audio thread retired (they can hold a whole decoded track).
    pub fn collect_garbage(&self) {
        while self.garbage.try_recv().is_ok() {}
        while self.mic_garbage.try_recv().is_ok() {}
    }

    pub fn send(&self, command: Command) {
        self.collect_garbage();
        if self.commands.send(command).is_err() {
            log::error!("Audio output thread is gone; command dropped");
        }
    }
}

/// Which sound card an output plays on.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceChoice {
    /// The internal clock: no card at all.
    Clock,
    Default,
    /// This card, or the default one if it is missing.
    Named(String),
    /// This card or nothing (for the preview output: never fall back to the air).
    Only(String),
}

/// Names of the sound cards that can play audio.
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Name of the system's default sound card.
pub fn default_output_device() -> Option<String> {
    cpal::default_host().default_output_device().and_then(|d| d.name().ok())
}

fn start_stream(parts: Parts, name: Option<&str>, strict: bool) -> Result<(cpal::Stream, String, StreamTap), String> {
    let host = cpal::default_host();
    let named = name.and_then(|wanted| {
        host.output_devices().ok()?.find(|d| d.name().is_ok_and(|n| n == wanted))
    });
    let device = match (named, name) {
        (Some(device), _) => device,
        (None, Some(wanted)) if strict => return Err(format!("Sound card \"{wanted}\" not found")),
        (None, wanted) => {
            if let Some(wanted) = wanted {
                log::warn!("Sound card \"{wanted}\" not found; using the default one");
            }
            host.default_output_device().ok_or("No audio output device found")?
        }
    };
    let device_name = device.name().unwrap_or_else(|_| "unknown".into());
    let supported = device
        .default_output_config()
        .map_err(|e| format!("Cannot query output config: {e}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let (renderer, tap) = Renderer::new(parts, config.sample_rate.0, config.channels);

    let stream = match format {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, renderer),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, renderer),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, renderer),
        cpal::SampleFormat::I32 => build_stream::<i32>(&device, &config, renderer),
        other => Err(format!("Unsupported output sample format: {other}")),
    }?;
    stream
        .play()
        .map_err(|e| format!("Cannot start output stream: {e}"))?;

    log::info!(
        "Audio output: {device_name}, {} Hz, {} channels, {format}",
        config.sample_rate.0,
        config.channels
    );
    Ok((stream, device_name, tap))
}

/// What the audio callback needs: the commands from the engine, where to send retired
/// players, the shared state, the decks and the stream tap. Used by the cpal callback
/// and by the internal clock when there is no sound card.
struct Renderer {
    parts: Parts,
    decks: Decks,
    tap: rtrb::Producer<f32>,
    out_channels: usize,
    out_rate: f64,
    /// Volume applied now, moving towards the shared target.
    volume_now: f32,
    mic: Option<Box<MicFeed>>,
    carts: Carts,
    /// Scratch for the live layer, allocated once: the microphone (mono) and the bank
    /// (interleaved), one pass of `LIVE_BLOCK` frames at a time.
    mic_buf: Vec<f32>,
    live_buf: Vec<f32>,
}

/// The engine-side handles, cloneable so a failed device can fall back to the clock.
#[derive(Clone)]
struct Parts {
    commands: Receiver<Command>,
    garbage: Sender<Box<TrackPlayer>>,
    mic_garbage: Sender<Box<MicFeed>>,
    shared: Arc<Shared>,
}

impl Renderer {
    fn new(parts: Parts, sample_rate: u32, channels: u16) -> (Self, StreamTap) {
        // Two seconds of audio between the callback and the streaming encoder.
        let (producer, consumer) =
            rtrb::RingBuffer::new(sample_rate as usize * channels as usize * 2);
        let volume_now = parts.shared.volume();
        parts.shared.out_rate.store(sample_rate, Ordering::Relaxed);
        let renderer = Self {
            parts,
            decks: Decks::default(),
            tap: producer,
            out_channels: channels as usize,
            out_rate: sample_rate as f64,
            volume_now,
            mic: None,
            carts: Carts::default(),
            mic_buf: vec![0.0; LIVE_BLOCK],
            live_buf: vec![0.0; LIVE_BLOCK * channels as usize],
        };
        (renderer, StreamTap { consumer, sample_rate, channels })
    }

    /// Fill one buffer. Real-time safe.
    fn process<T>(&mut self, data: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
        f32: FromSample<T>,
    {
        let Parts { commands, garbage, mic_garbage, shared } = &self.parts;
        let decks = &mut self.decks;
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Load(new_player) => {
                    decks.fade_len = None;
                    shared.silent_frames.store(0, Ordering::Relaxed); // a new track, a new count
                    if let Some(old) = decks.current.replace(new_player) {
                        let _ = garbage.try_send(old);
                    }
                    if let Some(old) = decks.next.take() {
                        let _ = garbage.try_send(old);
                    }
                    shared.finished.store(false, Ordering::Relaxed);
                }
                Command::Queue(next) => {
                    if let Some(old) = decks.next.replace(next) {
                        let _ = garbage.try_send(old);
                    }
                }
                Command::Seek(position) => {
                    if let Some(p) = decks.current.as_mut() {
                        p.seek(position);
                    }
                    shared.finished.store(false, Ordering::Relaxed);
                }
                Command::Mic(feed) => {
                    if let Some(old) = std::mem::replace(&mut self.mic, feed) {
                        let _ = mic_garbage.try_send(old);
                    }
                }
                Command::CartPlay(voice) => {
                    self.carts.start(voice, &mut |old| {
                        let _ = garbage.try_send(old);
                    });
                }
                Command::CartStop(button) => self.carts.stop(button),
            }
        }
        // The music heads lower while the microphone or a ducking bank sound is on.
        let live_duck = self
            .mic
            .as_ref()
            .map_or(1.0, |m| m.duck(&shared.mic))
            .min(self.carts.duck());

        let target = shared.volume();
        let ramp = shared.ramp_ms_per_unit.load(Ordering::Relaxed);
        self.volume_now = if ramp == 0 {
            target
        } else {
            let block_ms = (data.len() / self.out_channels) as f64 * 1000.0 / self.out_rate;
            let step = (block_ms / ramp as f64) as f32;
            self.volume_now + (target - self.volume_now).clamp(-step, step)
        };

        let playing = shared.playing.load(Ordering::Relaxed);
        if !playing || decks.current.is_none() {
            data.fill(T::EQUILIBRIUM);
        } else {
            let fade = shared.crossfade();
            let mut retire = |old| {
                let _ = garbage.try_send(old);
            };
            let outcome = decks.render(
                data,
                self.out_channels,
                self.out_rate,
                self.volume_now,
                live_duck,
                &fade,
                &mut retire,
            );
            if outcome.overlays_started > 0 {
                shared.overlays_started.fetch_add(outcome.overlays_started as u64, Ordering::Relaxed);
            }
            if outcome.overlays_ended > 0 {
                shared.overlays_ended.fetch_add(outcome.overlays_ended as u64, Ordering::Relaxed);
            }
            if outcome.switches > 0 {
                shared.switches.fetch_add(outcome.switches as u64, Ordering::Relaxed);
            }
            if outcome.finished {
                shared.playing.store(false, Ordering::Relaxed);
                shared.finished.store(true, Ordering::Relaxed);
            }
        }

        // The live layer goes over the music, playing or not: the microphone and the bank.
        if self.mic.is_some() || self.carts.is_active() {
            let mic_on = self.mic.as_ref().is_some_and(|m| m.audible(&shared.mic));
            let channels = self.out_channels;
            for chunk in data.chunks_mut(LIVE_BLOCK * channels) {
                let frames = chunk.len() / channels;
                let live = &mut self.live_buf[..chunk.len()];
                live.fill(0.0);
                let mut retire = |old| {
                    let _ = garbage.try_send(old);
                };
                let carts_on = self.carts.is_active();
                self.carts.mix(live, channels, self.out_rate, &mut retire);
                if let Some(mic) = self.mic.as_mut() {
                    // Always run, so the meter and the speaking flag work with it closed.
                    let voice = &mut self.mic_buf[..frames];
                    mic.render(voice, self.out_rate, &shared.mic);
                    if mic_on {
                        for (frame, value) in live.chunks_mut(channels).zip(voice.iter()) {
                            frame.iter_mut().for_each(|s| *s += value);
                        }
                    }
                }
                if mic_on || carts_on {
                    for (out, value) in chunk.iter_mut().zip(live.iter()) {
                        let mixed = f32::from_sample_(*out) + value * self.volume_now;
                        *out = T::from_sample(limit(mixed));
                    }
                }
            }
        }
        self.carts.publish(&shared.carts);

        // Levels of what actually goes out (after volume). Channels past the first two
        // count as right; mono feeds both sides.
        let mut peaks = [0.0f32; 2];
        let mut clipped = 0u32;
        for (i, sample) in data.iter().enumerate() {
            let value = f32::from_sample_(*sample).abs();
            let side = (i % self.out_channels).min(1);
            peaks[side] = peaks[side].max(value);
            if value >= CLIP_LEVEL {
                clipped += 1;
            }
        }
        if self.out_channels == 1 {
            peaks[1] = peaks[0];
        }
        // Dead air: count silent frames while playing, judged before the volume.
        let silence_level = f32::from_bits(shared.silence_level_bits.load(Ordering::Relaxed));
        if playing && silence_level > 0.0 {
            let loudest = peaks[0].max(peaks[1]);
            let source = if self.volume_now > 0.0 { loudest / self.volume_now } else { f32::MAX };
            if source < silence_level {
                let frames = (data.len() / self.out_channels) as u64;
                shared.silent_frames.fetch_add(frames, Ordering::Relaxed);
            } else {
                shared.silent_frames.store(0, Ordering::Relaxed);
            }
        } else {
            shared.silent_frames.store(0, Ordering::Relaxed);
        }

        for (slot, peak) in shared.peak_bits.iter().zip(peaks) {
            slot.fetch_max(peak.to_bits(), Ordering::Relaxed);
        }
        if clipped > 0 {
            shared.clipped.fetch_add(clipped, Ordering::Relaxed);
        }

        // Silence included, so a stream stays alive while paused. When nobody reads
        // the tap it is full and this copies nothing.
        let free = self.tap.slots().min(data.len());
        if free > 0 {
            if let Ok(mut chunk) = self.tap.write_chunk_uninit(free) {
                let (first, second) = chunk.as_mut_slices();
                let split = first.len();
                for (slot, sample) in first.iter_mut().zip(&data[..split]) {
                    slot.write(f32::from_sample_(*sample));
                }
                for (slot, sample) in second.iter_mut().zip(&data[split..free]) {
                    slot.write(f32::from_sample_(*sample));
                }
                // SAFETY: all `free` slots were written just above.
                unsafe { chunk.commit_all() };
            }
        }

        if let Some(p) = decks.current.as_ref() {
            shared
                .position_ms
                .store(p.position().as_millis() as u64, Ordering::Relaxed);
        }
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut renderer: Renderer,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
    f32: FromSample<T>,
{
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| renderer.process(data),
            |err| log::error!("Audio output error: {err}"),
            None,
        )
        .map_err(|e| format!("Cannot build output stream: {e}"))
}

/// Sample rate of the internal clock used without a sound card.
const CLOCK_RATE: u32 = 44_100;

/// Without a sound card, drive the renderer from a timer at real-time pace, so playback,
/// auto-advance and streaming work on headless machines. Paced against the start time,
/// so sleep jitter does not accumulate as drift.
fn run_clock(mut renderer: Renderer, shutdown: Receiver<()>) {
    const FRAMES: usize = CLOCK_RATE as usize / 100; // 10 ms
    let mut buffer = vec![0.0f32; FRAMES * 2];
    let start = Instant::now();
    let mut frames_done: u64 = 0;
    while !matches!(shutdown.try_recv(), Err(crossbeam_channel::TryRecvError::Disconnected)) {
        renderer.process(&mut buffer);
        frames_done += FRAMES as u64;
        let due = start + Duration::from_secs_f64(frames_done as f64 / CLOCK_RATE as f64);
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
}

/// The current track and the one queued after it, like the two decks of a DJ mixer,
/// plus a third for imaging played over the current track.
pub(crate) struct Decks {
    pub current: Option<Box<TrackPlayer>>,
    pub next: Option<Box<TrackPlayer>>,
    /// Length in output frames of the crossfade in progress, fixed when it starts, so a
    /// track queued late fades over the time left instead of jumping mid-curve.
    fade_len: Option<f64>,
    /// Imaging playing over the current track.
    overlay: Option<Box<TrackPlayer>>,
    /// Gain of the current track now and where it is heading (ducking).
    duck_now: f32,
    duck_target: f32,
}

impl Default for Decks {
    fn default() -> Self {
        Self { current: None, next: None, fade_len: None, overlay: None, duck_now: 1.0, duck_target: 1.0 }
    }
}

/// Ducking ramps: quick down so the first word is clear, slower back up.
const DUCK_ATTACK_MS: f64 = 150.0;
const DUCK_RELEASE_MS: f64 = 400.0;
/// The limiter leaves the signal alone below this level.
const LIMIT_KNEE: f32 = 0.9;

/// Soft limiter: transparent under the knee, then bends smoothly towards full scale,
/// so normalisation plus imaging over a track never clips hard. Real-time safe.
#[inline]
fn limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= LIMIT_KNEE {
        x
    } else {
        let room = 1.0 - LIMIT_KNEE;
        (LIMIT_KNEE + room * ((a - LIMIT_KNEE) / room).tanh()).copysign(x)
    }
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct RenderOutcome {
    /// How many times `next` became `current`.
    pub switches: u32,
    /// `current` ended with nothing queued.
    pub finished: bool,
    /// Overlays that started and ended during this buffer.
    pub overlays_started: u32,
    pub overlays_ended: u32,
}

impl Decks {
    /// Fill `data` (interleaved, `channels` per frame) from the current track, crossfading
    /// into the queued one over its last `fade.duration`, or switching on the exact frame
    /// it ends when the fade is off. Real-time safe: replaced players go to `retire`.
    pub fn render<T: SizedSample + FromSample<f32>>(
        &mut self,
        data: &mut [T],
        channels: usize,
        out_rate: f64,
        volume: f32,
        live_duck: f32,
        fade: &CrossfadeConfig,
        retire: &mut impl FnMut(Box<TrackPlayer>),
    ) -> RenderOutcome {
        let mut outcome = RenderOutcome::default();
        let attack = (1.0 / (DUCK_ATTACK_MS * out_rate / 1000.0)) as f32;
        let release = (1.0 / (DUCK_RELEASE_MS * out_rate / 1000.0)) as f32;
        for frame in data.chunks_mut(channels) {
            self.switch_if_finished(&mut outcome, retire);
            // Imaging attached to the current track starts at its frame.
            if let Some(due) = self.current.as_mut().and_then(|c| c.take_due_overlay()) {
                if let Some(old) = self.overlay.replace(due.player) {
                    retire(old);
                    outcome.overlays_ended += 1;
                }
                self.duck_target = due.duck;
                outcome.overlays_started += 1;
            }
            if self.overlay.as_ref().is_some_and(|o| o.is_finished()) {
                if let Some(old) = self.overlay.take() {
                    retire(old);
                }
                self.duck_target = 1.0;
                outcome.overlays_ended += 1;
            }
            let target = self.duck_target.min(live_duck);
            let step = if target < self.duck_now { attack } else { release };
            self.duck_now += (target - self.duck_now).clamp(-step, step);

            let Some(cur) = self.current.as_mut().filter(|p| !p.is_finished()) else {
                frame.fill(T::EQUILIBRIUM);
                continue;
            };
            let cur_step = cur.sample_rate() as f64 / out_rate;
            // Crossfade gains when the next track fades in now.
            let fade_gains = match self.next.as_ref() {
                Some(next) if fade.enabled && !next.no_overlap() && !cur.no_overlap() => {
                    let next_step = next.sample_rate() as f64 / out_rate;
                    // Never fade over more than half of either track.
                    let window = (fade.duration.as_secs_f64() * out_rate)
                        .min(cur.total_frames() as f64 / cur_step / 2.0)
                        .min(next.total_frames() as f64 / next_step / 2.0);
                    let remaining = cur.remaining_frames() / cur_step;
                    if window >= 1.0 && remaining <= window {
                        let len = *self.fade_len.get_or_insert(remaining.max(1.0));
                        Some((fade.gain_at(1.0 - (remaining / len) as f32), next_step))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let duck = self.duck_now;
            // Only what RIARS adds can go past full scale (a raised track, imaging on top):
            // anything else passes untouched, so the clip meter still means the source.
            let limiting = cur.gain() > 1.0 || self.overlay.is_some() || duck < 1.0;
            for (channel, out) in frame.iter_mut().enumerate() {
                let mut mixed = match (fade_gains, self.next.as_ref()) {
                    (Some(((out_gain, in_gain), _)), Some(next)) => {
                        cur.sample(channel) * cur.gain() * out_gain + next.sample(channel) * next.gain() * in_gain
                    }
                    _ => cur.sample(channel) * cur.gain(),
                } * duck;
                if let Some(over) = self.overlay.as_ref() {
                    mixed += over.sample(channel) * over.gain();
                }
                let mixed = mixed * volume;
                *out = T::from_sample(if limiting { limit(mixed) } else { mixed });
            }
            if let (Some((_, next_step)), Some(next)) = (fade_gains, self.next.as_mut()) {
                next.advance(next_step);
            }
            if let Some(over) = self.overlay.as_mut() {
                let over_step = over.sample_rate() as f64 / out_rate;
                over.advance(over_step);
            }
            cur.advance(cur_step);
        }
        // The track can end on the buffer's last frame: switch here too, or playback
        // would stop with the next track still queued.
        self.switch_if_finished(&mut outcome, retire);
        if self.current.as_ref().is_some_and(|p| p.is_finished()) {
            outcome.finished = true;
        }
        outcome
    }

    fn switch_if_finished(
        &mut self,
        outcome: &mut RenderOutcome,
        retire: &mut impl FnMut(Box<TrackPlayer>),
    ) {
        if self.current.as_ref().is_some_and(|p| p.is_finished()) {
            if let Some(next) = self.next.take() {
                if let Some(old) = self.current.replace(next) {
                    retire(old);
                }
                self.fade_len = None;
                outcome.switches += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;

    /// Mono track of `frames` samples, all equal to `level`, at 1 kHz.
    fn track(level: f32, frames: usize) -> Box<TrackPlayer> {
        Box::new(TrackPlayer::from_samples(PathBuf::from("t.wav"), vec![level; frames], 1, 1000))
    }

    fn render(decks: &mut Decks, frames: usize, fade: &CrossfadeConfig) -> (Vec<f32>, RenderOutcome) {
        let mut out = vec![9.0f32; frames];
        let mut retired = 0;
        let outcome = decks.render(&mut out, 1, 1000.0, 1.0, 1.0, fade, &mut |_| retired += 1);
        assert_eq!(retired, (outcome.switches + outcome.overlays_ended) as usize);
        (out, outcome)
    }

    fn no_fade() -> CrossfadeConfig {
        CrossfadeConfig { enabled: false, ..Default::default() }
    }

    fn linear(ms: u64) -> CrossfadeConfig {
        CrossfadeConfig { duration: Duration::from_millis(ms), curve: FadeCurve::Linear, enabled: true }
    }

    #[test]
    fn gapless_switch_without_fade() {
        let mut decks = Decks { current: Some(track(1.0, 4)), next: Some(track(0.5, 3)), ..Default::default() };
        let (out, outcome) = render(&mut decks, 9, &no_fade());
        assert_eq!(out, [1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5, 0.0, 0.0]);
        assert_eq!(outcome, RenderOutcome { switches: 1, finished: true, ..Default::default() });
    }

    #[test]
    fn switch_when_track_ends_on_last_frame_of_buffer() {
        let mut decks = Decks { current: Some(track(1.0, 4)), next: Some(track(0.5, 4)), ..Default::default() };
        let (out, outcome) = render(&mut decks, 4, &no_fade());
        assert_eq!(out, [1.0; 4]);
        assert_eq!(outcome, RenderOutcome { switches: 1, finished: false, ..Default::default() });
        let (out, _) = render(&mut decks, 2, &no_fade());
        assert_eq!(out, [0.5, 0.5]);
    }

    #[test]
    fn linear_crossfade_over_last_frames() {
        // 4 ms fade at 1 kHz = 4 frames: the outgoing track ramps 1 -> 0.25 while the
        // incoming one (level 0) ramps in, then the incoming one carries on.
        let mut decks = Decks { current: Some(track(1.0, 10)), next: Some(track(0.0, 10)), ..Default::default() };
        let (out, outcome) = render(&mut decks, 10, &linear(4));
        let expected = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.75, 0.5, 0.25];
        for (got, want) in out.iter().zip(expected) {
            assert!((got - want).abs() < 1e-5, "{out:?}");
        }
        assert_eq!(outcome.switches, 1);
        // The fade spans 4 frames (the first at gain 1.0), so 6 of the incoming track remain.
        assert!((decks.current.as_ref().unwrap().remaining_frames() - 6.0).abs() < 1e-9);
    }

    #[test]
    fn no_overlap_tracks_switch_without_fading() {
        let mut spot = track(0.5, 3);
        spot.set_no_overlap(true);
        let mut decks = Decks { current: Some(track(1.0, 4)), next: Some(spot), ..Default::default() };
        let (out, _) = render(&mut decks, 7, &linear(2));
        assert_eq!(out, [1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5]);
        // And out of it.
        let mut spot = track(0.5, 4);
        spot.set_no_overlap(true);
        let mut decks = Decks { current: Some(spot), next: Some(track(1.0, 4)), ..Default::default() };
        let (out, _) = render(&mut decks, 6, &linear(2));
        assert_eq!(out, [0.5, 0.5, 0.5, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn overlay_plays_over_the_track_from_its_frame_and_ducks_it() {
        // 1 kHz: host at 0.5 for 1000 frames; imaging at 0.25 for 100 frames from frame 200,
        // ducking the host to half. Attack 150 ms = 150 frames, release 400 frames.
        let mut host = track(0.5, 1000);
        host.set_overlay(crate::audio::player::Overlay { player: track(0.25, 100), start_frame: 200.0, duck: 0.5 });
        let mut decks = Decks { current: Some(host), ..Default::default() };
        let (out, outcome) = render(&mut decks, 1000, &no_fade());
        assert_eq!(out[199], 0.5); // untouched before the overlay
        assert!(out[201] > 0.7, "{}", out[201]); // host + imaging
        assert!(out[299] < out[201], "ducking pulls the host down");
        assert_eq!((outcome.overlays_started, outcome.overlays_ended), (1, 1));
        assert!(out[999] > out[350], "released after the overlay");
    }

    #[test]
    fn overlay_is_dropped_with_a_track_that_never_reaches_it() {
        let mut host = track(0.5, 100);
        host.set_overlay(crate::audio::player::Overlay { player: track(0.25, 50), start_frame: 500.0, duck: 0.5 });
        let mut decks = Decks { current: Some(host), next: Some(track(0.1, 100)), ..Default::default() };
        let (_, outcome) = render(&mut decks, 150, &no_fade());
        assert_eq!((outcome.switches, outcome.overlays_started), (1, 0));
    }

    #[test]
    fn gain_and_limiter() {
        let mut loud = track(0.6, 10);
        loud.set_gain(2.0); // 1.2 before the limiter
        let mut decks = Decks { current: Some(loud), ..Default::default() };
        let (out, _) = render(&mut decks, 5, &no_fade());
        assert!(out[0] < 1.0 && out[0] > 0.9, "{}", out[0]);
        assert_eq!(limit(0.5), 0.5);
        assert!(limit(-3.0) >= -1.0 && limit(-3.0) < -0.95);
        assert!(limit(0.95) < limit(0.99));
    }

    #[test]
    fn fade_never_exceeds_half_a_short_track() {
        // A 3 s fade on 8-frame tracks shrinks to 4 frames.
        let mut decks = Decks { current: Some(track(1.0, 8)), next: Some(track(0.0, 8)), ..Default::default() };
        let (out, _) = render(&mut decks, 8, &linear(3000));
        assert_eq!(&out[..5], &[1.0; 5]);
        assert!(out[5] < 1.0 && out[7] > 0.0, "{out:?}");
    }

    #[test]
    fn late_queued_track_fades_from_the_start_of_the_curve() {
        // The next track arrives with 2 frames left of a 4-frame fade window: the fade
        // starts at full gain and spans those 2 frames, with no jump to mid-curve.
        let mut decks = Decks { current: Some(track(1.0, 10)), ..Default::default() };
        let (out, _) = render(&mut decks, 8, &linear(4));
        assert_eq!(out, [1.0; 8]);
        decks.next = Some(track(0.0, 10));
        let (out, outcome) = render(&mut decks, 3, &linear(4));
        assert_eq!(out, [1.0, 0.5, 0.0]);
        assert_eq!(outcome.switches, 1);
    }

    #[test]
    fn volume_glides_to_target() {
        // 1 kHz, mono, 10-frame blocks; glide from 0 to 1 at 100 ms per unit.
        let (_cmd_tx, commands) = crossbeam_channel::bounded(4);
        let (garbage, _garbage_rx) = crossbeam_channel::bounded(4);
        let shared = Arc::new(Shared::new(0.0));
        let parts = Parts { commands, garbage, mic_garbage: crossbeam_channel::bounded(4).0, shared: Arc::clone(&shared) };
        let (mut renderer, _tap) = Renderer::new(parts, 1000, 1);
        renderer.decks.current = Some(track(1.0, 1000));
        shared.playing.store(true, Ordering::Relaxed);
        shared.ramp_volume(1.0, 100);

        let mut levels = Vec::new();
        for _ in 0..12 {
            let mut block = [0.0f32; 10];
            renderer.process(&mut block);
            levels.push(block[0]);
        }
        // +0.1 per 10 ms block, then holds at 1.0: no jump.
        for (i, level) in levels.iter().enumerate() {
            let want = ((i + 1) as f32 * 0.1).min(1.0);
            assert!((level - want).abs() < 1e-4, "{levels:?}");
        }
        // A plain set_volume jumps.
        shared.set_volume(0.2);
        let mut block = [0.0f32; 10];
        renderer.process(&mut block);
        assert!((block[0] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn levels_track_peaks_and_clipping() {
        let (_cmd_tx, commands) = crossbeam_channel::bounded(4);
        let (garbage, _garbage_rx) = crossbeam_channel::bounded(4);
        let shared = Arc::new(Shared::new(1.0));
        let parts = Parts { commands, garbage, mic_garbage: crossbeam_channel::bounded(4).0, shared: Arc::clone(&shared) };
        let (mut renderer, _tap) = Renderer::new(parts, 1000, 2);
        // Stereo source: left 0.5, right 0.25.
        let samples = [0.5f32, 0.25].repeat(100);
        renderer.decks.current =
            Some(Box::new(TrackPlayer::from_samples(PathBuf::from("s.wav"), samples, 2, 1000)));
        shared.playing.store(true, Ordering::Relaxed);

        let mut block = [0.0f32; 20];
        renderer.process(&mut block);
        renderer.process(&mut block);
        let levels = shared.take_levels();
        assert_eq!((levels.peak_left, levels.peak_right, levels.clipped), (0.5, 0.25, 0));
        // Reading resets them.
        assert_eq!(shared.take_levels(), Levels::default());

        // Volume applies, and full scale counts as clipping.
        shared.set_volume(2.0);
        renderer.process(&mut block);
        let levels = shared.take_levels();
        assert_eq!((levels.peak_left, levels.peak_right), (1.0, 0.5));
        assert_eq!(levels.clipped, 10);

        // A 16-bit full-scale sample counts too; just below it does not.
        shared.set_volume(32767.0 / 32768.0 / 0.5);
        renderer.process(&mut block);
        assert_eq!(shared.take_levels().clipped, 10);
        shared.set_volume(0.999 / 0.5);
        renderer.process(&mut block);
        assert_eq!(shared.take_levels().clipped, 0);
    }

    #[test]
    fn counts_silence_before_volume() {
        let (_cmd_tx, commands) = crossbeam_channel::bounded(4);
        let (garbage, _garbage_rx) = crossbeam_channel::bounded(4);
        let shared = Arc::new(Shared::new(1.0));
        let parts = Parts { commands, garbage, mic_garbage: crossbeam_channel::bounded(4).0, shared: Arc::clone(&shared) };
        let (mut renderer, _tap) = Renderer::new(parts, 1000, 1);
        shared.set_silence_level(0.01);
        shared.playing.store(true, Ordering::Relaxed);
        let mut block = [0.0f32; 10];
        let frames = || shared.silent_frames.load(Ordering::Relaxed);

        renderer.decks.current = Some(track(0.001, 1000)); // under the threshold
        renderer.process(&mut block);
        renderer.process(&mut block);
        assert_eq!(frames(), 20);

        // Volume at zero is not silence of the programme.
        shared.set_volume(0.0);
        renderer.process(&mut block);
        assert_eq!(frames(), 0);

        // Quiet music turned down still counts by its source level: 0.5 x 0.1 is loud.
        shared.set_volume(0.1);
        renderer.decks.current = Some(track(0.5, 1000));
        renderer.process(&mut block);
        assert_eq!(frames(), 0);

        // Paused is not dead air either.
        renderer.decks.current = Some(track(0.0, 1000));
        renderer.process(&mut block);
        assert_eq!(frames(), 10);
        shared.playing.store(false, Ordering::Relaxed);
        renderer.process(&mut block);
        assert_eq!(frames(), 0);
    }

    #[test]
    fn no_fade_without_a_queued_track() {
        let mut decks = Decks { current: Some(track(1.0, 3)), ..Default::default() };
        let (out, outcome) = render(&mut decks, 5, &linear(2));
        assert_eq!(out, [1.0, 1.0, 1.0, 0.0, 0.0]);
        assert_eq!(outcome, RenderOutcome { switches: 0, finished: true, ..Default::default() });
    }

    #[test]
    fn live_layer_plays_over_silence_and_ducks_the_music() {
        // 1 kHz mono output, a 1 kHz microphone fed at a steady 0.25.
        let (cmd_tx, commands) = crossbeam_channel::bounded(8);
        let (garbage, _garbage_rx) = crossbeam_channel::bounded(8);
        let shared = Arc::new(Shared::new(1.0));
        let parts = Parts { commands, garbage, mic_garbage: crossbeam_channel::bounded(4).0, shared: Arc::clone(&shared) };
        let (mut renderer, _tap) = Renderer::new(parts, 1000, 1);
        shared.mic.apply(&crate::audio::live::MicConfig { high_pass: false, compressor: false, ..Default::default() });
        let (mut mic_in, mic_out) = rtrb::RingBuffer::new(4096);
        cmd_tx.send(Command::Mic(Some(Box::new(MicFeed::new(mic_out, 1000))))).unwrap();
        let mut block = |renderer: &mut Renderer, frames: usize| {
            let mut out = vec![0.0f32; frames];
            for chunk in out.chunks_mut(10) {
                while mic_in.slots() > 4096 - 30 {
                    mic_in.push(0.25).unwrap();
                }
                renderer.process(chunk);
            }
            out
        };

        // Nothing playing and the microphone closed: silence.
        assert!(block(&mut renderer, 100).iter().all(|v| *v == 0.0));

        // A bank sound plays with the playlist stopped.
        cmd_tx
            .send(Command::CartPlay(CartVoice { player: track(0.5, 50), button: 7, serial: 1, looping: false, duck: false, gain: 1.0 }))
            .unwrap();
        let out = block(&mut renderer, 40);
        assert!((out[5] - 0.5).abs() < 1e-6, "{out:?}");
        assert_eq!(shared.carts.button[0].load(Ordering::Relaxed), 7);
        block(&mut renderer, 40);
        assert_eq!(shared.carts.serial[0].load(Ordering::Relaxed), 0, "over and retired");

        // Music at 0.5; opening the microphone ducks it 12 dB and adds the voice.
        renderer.decks.current = Some(track(0.5, 100_000));
        shared.playing.store(true, Ordering::Relaxed);
        assert!((block(&mut renderer, 50)[49] - 0.5).abs() < 1e-6);
        shared.mic.on.store(true, Ordering::Relaxed);
        let out = block(&mut renderer, 600);
        let expected = 0.5 * 10f32.powf(-12.0 / 20.0) + 0.25;
        assert!((out[599] - expected).abs() < 0.01, "{} vs {expected}", out[599]);

        // Closing it brings the music back and the voice goes.
        shared.mic.on.store(false, Ordering::Relaxed);
        let out = block(&mut renderer, 1000);
        assert!((out[999] - 0.5).abs() < 1e-3, "{}", out[999]);
    }
}
