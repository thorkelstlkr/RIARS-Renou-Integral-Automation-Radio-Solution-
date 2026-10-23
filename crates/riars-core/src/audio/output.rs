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
use super::player::TrackPlayer;

pub(crate) enum Command {
    Load(Box<TrackPlayer>),
    /// Play this track the moment the current one ends (gapless).
    Queue(Box<TrackPlayer>),
    Seek(Duration),
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
    /// Crossfade length in ms between queued tracks; 0 disables it.
    crossfade_ms: AtomicU64,
    crossfade_curve: AtomicU8,
    /// Highest |sample| per side (left, right) since the last `take_levels`, as f32
    /// bits: for non-negative floats the bit patterns order like the values, so
    /// `fetch_max` on the bits keeps the loudest peak.
    peak_bits: [AtomicU32; 2],
    /// Samples at or beyond full scale since the last `take_levels`.
    clipped: AtomicU32,
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
            crossfade_ms: AtomicU64::new(0),
            crossfade_curve: AtomicU8::new(0),
            peak_bits: [AtomicU32::new(0), AtomicU32::new(0)],
            clipped: AtomicU32::new(0),
        }
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
    pub fn open(volume: f32, use_device: bool) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = bounded::<Command>(16);
        let (garbage_tx, garbage_rx) = bounded::<Box<TrackPlayer>>(16);
        let (shutdown_tx, shutdown_rx) = bounded::<()>(0);
        let (ready_tx, ready_rx) = bounded::<Result<(String, StreamTap), String>>(1);
        let shared = Arc::new(Shared::new(volume));
        let callback_shared = Arc::clone(&shared);

        let parts = Parts { commands: cmd_rx, garbage: garbage_tx, shared: callback_shared };
        let thread = std::thread::Builder::new()
            .name("riars-audio-output".into())
            .spawn(move || {
                if use_device {
                    match start_stream(parts.clone()) {
                        Ok((stream, device_name, tap)) => {
                            let _ = ready_tx.send(Ok((device_name, tap)));
                            let _ = shutdown_rx.recv();
                            drop(stream);
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
    }

    pub fn send(&self, command: Command) {
        self.collect_garbage();
        if self.commands.send(command).is_err() {
            log::error!("Audio output thread is gone; command dropped");
        }
    }
}

fn start_stream(parts: Parts) -> Result<(cpal::Stream, String, StreamTap), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("No audio output device found")?;
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
}

/// The engine-side handles, cloneable so a failed device can fall back to the clock.
#[derive(Clone)]
struct Parts {
    commands: Receiver<Command>,
    garbage: Sender<Box<TrackPlayer>>,
    shared: Arc<Shared>,
}

impl Renderer {
    fn new(parts: Parts, sample_rate: u32, channels: u16) -> (Self, StreamTap) {
        // Two seconds of audio between the callback and the streaming encoder.
        let (producer, consumer) =
            rtrb::RingBuffer::new(sample_rate as usize * channels as usize * 2);
        let volume_now = parts.shared.volume();
        let renderer = Self {
            parts,
            decks: Decks::default(),
            tap: producer,
            out_channels: channels as usize,
            out_rate: sample_rate as f64,
            volume_now,
        };
        (renderer, StreamTap { consumer, sample_rate, channels })
    }

    /// Fill one buffer. Real-time safe.
    fn process<T>(&mut self, data: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
        f32: FromSample<T>,
    {
        let Parts { commands, garbage, shared } = &self.parts;
        let decks = &mut self.decks;
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Load(new_player) => {
                    decks.fade_len = None;
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
            }
        }

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
                &fade,
                &mut retire,
            );
            if outcome.switches > 0 {
                shared.switches.fetch_add(outcome.switches as u64, Ordering::Relaxed);
            }
            if outcome.finished {
                shared.playing.store(false, Ordering::Relaxed);
                shared.finished.store(true, Ordering::Relaxed);
            }
        }

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
        for (slot, peak) in shared.peak_bits.iter().zip(peaks) {
            slot.fetch_max(peak.to_bits(), Ordering::Relaxed);
        }
        if clipped > 0 {
            shared.clipped.fetch_add(clipped, Ordering::Relaxed);
        }

        // Silence included, so a stream stays alive while paused.
        for sample in data.iter() {
            if self.tap.push(f32::from_sample_(*sample)).is_err() {
                break;
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

/// The current track and the one queued after it, like the two decks of a DJ mixer.
#[derive(Default)]
pub(crate) struct Decks {
    pub current: Option<Box<TrackPlayer>>,
    pub next: Option<Box<TrackPlayer>>,
    /// Length in output frames of the crossfade in progress, fixed when it starts, so a
    /// track queued late fades over the time left instead of jumping mid-curve.
    fade_len: Option<f64>,
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct RenderOutcome {
    /// How many times `next` became `current`.
    pub switches: u32,
    /// `current` ended with nothing queued.
    pub finished: bool,
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
        fade: &CrossfadeConfig,
        retire: &mut impl FnMut(Box<TrackPlayer>),
    ) -> RenderOutcome {
        let mut outcome = RenderOutcome::default();
        for frame in data.chunks_mut(channels) {
            self.switch_if_finished(&mut outcome, retire);
            let Some(cur) = self.current.as_mut().filter(|p| !p.is_finished()) else {
                frame.fill(T::EQUILIBRIUM);
                continue;
            };
            let cur_step = cur.sample_rate() as f64 / out_rate;
            let fading = match self.next.as_mut() {
                Some(next) if fade.enabled => {
                    let next_step = next.sample_rate() as f64 / out_rate;
                    // Never fade over more than half of either track.
                    let window = (fade.duration.as_secs_f64() * out_rate)
                        .min(cur.total_frames() as f64 / cur_step / 2.0)
                        .min(next.total_frames() as f64 / next_step / 2.0);
                    let remaining = cur.remaining_frames() / cur_step;
                    if window >= 1.0 && remaining <= window {
                        let len = *self.fade_len.get_or_insert(remaining.max(1.0));
                        let (out_gain, in_gain) = fade.gain_at(1.0 - (remaining / len) as f32);
                        for (channel, out) in frame.iter_mut().enumerate() {
                            let mixed =
                                cur.sample(channel) * out_gain + next.sample(channel) * in_gain;
                            *out = T::from_sample(mixed * volume);
                        }
                        next.advance(next_step);
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if !fading {
                for (channel, out) in frame.iter_mut().enumerate() {
                    *out = T::from_sample(cur.sample(channel) * volume);
                }
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
        let outcome = decks.render(&mut out, 1, 1000.0, 1.0, fade, &mut |_| retired += 1);
        assert_eq!(retired, outcome.switches as usize);
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
        assert_eq!(outcome, RenderOutcome { switches: 1, finished: true });
    }

    #[test]
    fn switch_when_track_ends_on_last_frame_of_buffer() {
        let mut decks = Decks { current: Some(track(1.0, 4)), next: Some(track(0.5, 4)), ..Default::default() };
        let (out, outcome) = render(&mut decks, 4, &no_fade());
        assert_eq!(out, [1.0; 4]);
        assert_eq!(outcome, RenderOutcome { switches: 1, finished: false });
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
        let parts = Parts { commands, garbage, shared: Arc::clone(&shared) };
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
        let parts = Parts { commands, garbage, shared: Arc::clone(&shared) };
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
    fn no_fade_without_a_queued_track() {
        let mut decks = Decks { current: Some(track(1.0, 3)), ..Default::default() };
        let (out, outcome) = render(&mut decks, 5, &linear(2));
        assert_eq!(out, [1.0, 1.0, 1.0, 0.0, 0.0]);
        assert_eq!(outcome, RenderOutcome { switches: 0, finished: true });
    }
}
