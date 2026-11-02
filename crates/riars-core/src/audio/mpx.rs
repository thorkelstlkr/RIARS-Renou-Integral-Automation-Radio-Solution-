//! FM stereo encoder with RDS: turns the on-air audio into an MPX (multiplex) signal for a
//! transmitter's MPX input, on a sound card running at four times the audio rate (192 kHz
//! for 48 kHz audio, 176.4 kHz for 44.1 kHz).
//!
//! Per input frame: pre-emphasis, a soft clipper, then ×4 interpolation through a FIR
//! low-pass at 17 kHz that also keeps the audio out of the pilot. Per output sample:
//! `audio·((L+R)/2 + (L−R)/2·sin 2θ) + pilot·sin θ + rds·b(t)·sin 3θ`, θ being the 19 kHz
//! pilot's phase, so the 38 kHz and 57 kHz subcarriers are locked to it. The RDS data
//! (1187.5 bit/s, exactly 16 pilot cycles per bit) is differentially and biphase coded,
//! then shaped by a 2.4 kHz low-pass.
//!
//! Real-time rules: after `new`, `MpxEncoder` does not allocate, lock or block. The
//! sound card's clock and the on-air card's clock drift apart; the callback keeps about
//! 100 ms of input buffered, dropping or repeating a single frame when it strays.

use std::f64::consts::TAU;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use crossbeam_channel::{bounded, Receiver, Sender};

use super::output::StreamTap;
use crate::rds::groups::{GroupEncoder, RdsFrame};

/// Interpolation factor: output rate / input rate.
const FACTOR: usize = 4;
/// FIR taps per polyphase branch (320 in all).
const TAPS_PER_PHASE: usize = 80;
/// Audio low-pass cutoff (centre of the transition band), Hz.
const AUDIO_CUTOFF: f64 = 17_000.0;
const PILOT_HZ: f64 = 19_000.0;
/// Pilot cycles per RDS bit (19000 / 1187.5).
const CYCLES_PER_BIT: f64 = 16.0;
/// RDS data low-pass, Hz.
const RDS_CUTOFF: f64 = 2_400.0;
/// Peak of the shaped RDS data for ±1 symbols.
const RDS_OVERSHOOT: f64 = 1.25;
/// Input kept buffered, seconds.
const TARGET_FILL: f64 = 0.1;

/// Pre-emphasis time constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreEmphasis {
    Off,
    /// Europe and most of the world.
    Us50,
    /// The Americas and South Korea.
    Us75,
}

/// Levels and switches, all applied without reopening the card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MpxParams {
    pub pre_emphasis: PreEmphasis,
    /// Stereo (pilot + 38 kHz); mono sends (L+R)/2 only, without a pilot.
    pub stereo: bool,
    pub rds: bool,
    /// Pilot share of the peak deviation, 0..=0.12 (9 % is usual).
    pub pilot: f32,
    /// RDS share of the peak deviation, 0..=0.12 (3–5 % is usual).
    pub rds_level: f32,
    /// Overall output gain (linear): full deviation is ±`gain` at the card.
    pub gain: f32,
}

impl Default for MpxParams {
    fn default() -> Self {
        Self { pre_emphasis: PreEmphasis::Us50, stereo: true, rds: true, pilot: 0.09, rds_level: 0.04, gain: 0.7 }
    }
}

/// Messages from the control thread to the audio callback. Copy: no allocation.
#[derive(Debug, Clone, Copy)]
pub enum MpxMsg {
    Params(MpxParams),
    Rds(RdsFrame),
}

/// RBJ biquad, transposed direct form II, in f64 (the RDS low-pass sits close to DC).
#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn lowpass(cutoff: f64, rate: f64, q: f64) -> Self {
        let w = TAU * cutoff / rate;
        let alpha = w.sin() / (2.0 * q);
        let cos = w.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// First-order pre-emphasis shelf: a zero at 1/(2πτ) and a pole at 20 kHz, so the boost
/// stops rising above the audio band (about +11.7 dB at 15 kHz for 50 µs).
#[derive(Debug, Clone, Copy, Default)]
struct PreEmph {
    b0: f32,
    b1: f32,
    a1: f32,
    x1: f32,
    y1: f32,
}

impl PreEmph {
    fn new(kind: PreEmphasis, rate: f64) -> Self {
        let tau = match kind {
            PreEmphasis::Off => return Self { b0: 1.0, ..Self::default() },
            PreEmphasis::Us50 => 50e-6,
            PreEmphasis::Us75 => 75e-6,
        };
        let tau_pole = 1.0 / (TAU * 20_000.0);
        // Bilinear transform of (1 + sτ) / (1 + sτp).
        let k = 2.0 * rate;
        let den = 1.0 + k * tau_pole;
        Self {
            b0: ((1.0 + k * tau) / den) as f32,
            b1: ((1.0 - k * tau) / den) as f32,
            a1: ((1.0 - k * tau_pole) / den) as f32,
            x1: 0.0,
            y1: 0.0,
        }
    }

    /// The gain at DC is 1; at high frequencies it rises, so normalise nothing here: the
    /// clipper after it holds the peaks.
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// Transparent below 0.7, then a smooth knee up to a hard ceiling of 1.
fn soft_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.7;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let y = KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh();
        y.copysign(x)
    }
}

/// ×4 polyphase interpolator for one channel. The history is stored twice, so the
/// window is always one contiguous slice.
struct Interpolator {
    history: Vec<f32>,
    pos: usize,
}

impl Interpolator {
    fn new() -> Self {
        Self { history: vec![0.0; 2 * TAPS_PER_PHASE], pos: 0 }
    }

    fn push(&mut self, x: f32) {
        self.pos = (self.pos + 1) % TAPS_PER_PHASE;
        if let Some(slot) = self.history.get_mut(self.pos) {
            *slot = x;
        }
        if let Some(slot) = self.history.get_mut(self.pos + TAPS_PER_PHASE) {
            *slot = x;
        }
    }

    /// Oldest to newest.
    fn window(&self) -> &[f32] {
        self.history.get(self.pos + 1..=self.pos + TAPS_PER_PHASE).unwrap_or(&[])
    }
}

/// The FIR taps by phase, each reversed to match `Interpolator::window` (oldest first).
fn design_phases(out_rate: f64) -> Vec<[f32; TAPS_PER_PHASE]> {
    // Below the input's Nyquist frequency, for input rates under 38 kHz.
    let cutoff = AUDIO_CUTOFF.min(0.45 * out_rate / FACTOR as f64);
    let n = FACTOR * TAPS_PER_PHASE;
    let fc = cutoff / out_rate;
    let middle = (n - 1) as f64 / 2.0;
    let mut h: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 - middle;
            let sinc = if t == 0.0 { 2.0 * fc } else { (TAU * fc * t).sin() / (std::f64::consts::PI * t) };
            let x = TAU * i as f64 / (n - 1) as f64;
            let blackman = 0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos();
            sinc * blackman
        })
        .collect();
    // Zero-stuffing divides the level by the factor: give it back.
    let sum: f64 = h.iter().sum();
    h.iter_mut().for_each(|v| *v *= FACTOR as f64 / sum);
    (0..FACTOR)
        .map(|phase| {
            let mut taps = [0.0f32; TAPS_PER_PHASE];
            for (j, tap) in taps.iter_mut().enumerate() {
                *tap = h[phase + FACTOR * (TAPS_PER_PHASE - 1 - j)] as f32;
            }
            taps
        })
        .collect()
}

/// The RDS data side: bits from the group encoder, differential + biphase coding, shaping.
struct RdsModulator {
    encoder: GroupEncoder,
    group: [u32; 4],
    /// Next bit of `group`, 0..104.
    bit: usize,
    /// Current transmitted symbol (after differential coding).
    symbol: bool,
    shape: [Biquad; 2],
}

impl RdsModulator {
    fn new(frame: RdsFrame, rate: f64) -> Self {
        let mut encoder = GroupEncoder::new(frame);
        let group = encoder.next_group();
        // Fourth-order Butterworth.
        let shape = [Biquad::lowpass(RDS_CUTOFF, rate, 0.541_196), Biquad::lowpass(RDS_CUTOFF, rate, 1.306_563)];
        Self { encoder, group, bit: 0, symbol: false, shape }
    }

    /// Start the next bit: d = next data bit, s = d XOR s_prev.
    fn next_bit(&mut self) {
        if self.bit >= 104 {
            self.group = self.encoder.next_group();
            self.bit = 0;
        }
        let block = self.group.get(self.bit / 26).copied().unwrap_or(0);
        let data = (block >> (25 - self.bit % 26)) & 1 == 1;
        self.symbol ^= data;
        self.bit += 1;
    }

    /// Shaped baseband for a position `in_bit` (pilot cycles, 0..16) inside the bit.
    fn sample(&mut self, in_bit: f64) -> f64 {
        let first_half = in_bit < CYCLES_PER_BIT / 2.0;
        let level = if self.symbol == first_half { 1.0 } else { -1.0 };
        let [a, b] = &mut self.shape;
        // The filter rings about 20 % over on each transition: scale it back to the level
        // (the clamp only catches what is left, without splattering the spectrum).
        (b.run(a.run(level)) / RDS_OVERSHOOT).clamp(-1.0, 1.0)
    }
}

/// The stereo encoder itself, without any sound card: audio frames in, MPX samples out.
pub struct MpxEncoder {
    params: MpxParams,
    in_rate: f64,
    out_rate: f64,
    pre: [PreEmph; 2],
    interp: [Interpolator; 2],
    phases: Vec<[f32; TAPS_PER_PHASE]>,
    /// Pilot cycles inside the current RDS bit, 0..16: the phase of everything.
    cycles: f64,
    cycles_per_sample: f64,
    rds: RdsModulator,
}

impl MpxEncoder {
    /// `in_rate` is the audio's rate; the MPX comes out at four times that.
    pub fn new(in_rate: u32, params: MpxParams, frame: RdsFrame) -> Self {
        let in_rate = in_rate as f64;
        let out_rate = in_rate * FACTOR as f64;
        let mut rds = RdsModulator::new(frame, out_rate);
        rds.next_bit();
        Self {
            params,
            in_rate,
            out_rate,
            pre: [PreEmph::new(params.pre_emphasis, in_rate); 2],
            interp: [Interpolator::new(), Interpolator::new()],
            phases: design_phases(out_rate),
            cycles: 0.0,
            cycles_per_sample: PILOT_HZ / out_rate,
            rds,
        }
    }

    pub fn out_rate(&self) -> u32 {
        self.out_rate as u32
    }

    pub fn set_params(&mut self, params: MpxParams) {
        if params.pre_emphasis != self.params.pre_emphasis {
            self.pre = [PreEmph::new(params.pre_emphasis, self.in_rate); 2];
        }
        self.params = params;
    }

    pub fn set_rds(&mut self, frame: RdsFrame) {
        self.rds.encoder.set_frame(frame);
    }

    /// One stereo input frame in, `FACTOR` MPX samples out.
    pub fn process(&mut self, left: f32, right: f32, out: &mut [f32; FACTOR]) {
        let p = self.params;
        for (channel, x) in [left, right].into_iter().enumerate() {
            let y = soft_clip(self.pre[channel].run(x));
            self.interp[channel].push(y);
        }
        let pilot = if p.stereo { p.pilot.clamp(0.0, 0.12) as f64 } else { 0.0 };
        let rds_level = if p.rds { p.rds_level.clamp(0.0, 0.12) as f64 } else { 0.0 };
        let audio = (1.0 - pilot - rds_level).max(0.0);
        let gain = if p.gain.is_finite() { p.gain.clamp(0.0, 1.0) as f64 } else { 0.0 };
        for (phase, sample) in out.iter_mut().enumerate() {
            let taps = self.phases.get(phase).map_or(&[][..], |t| &t[..]);
            let dot = |window: &[f32]| -> f64 {
                window.iter().zip(taps).map(|(x, h)| (x * h) as f64).sum::<f64>().clamp(-1.0, 1.0)
            };
            let (l, r) = (dot(self.interp[0].window()), dot(self.interp[1].window()));
            // One sine and cosine; the 38 and 57 kHz carriers follow from them.
            let (sin1, cos1) = (TAU * self.cycles.fract()).sin_cos();
            let sin2 = 2.0 * sin1 * cos1;
            let sin3 = sin1 * (3.0 - 4.0 * sin1 * sin1);
            let mid = (l + r) / 2.0;
            let mut mpx = if p.stereo {
                audio * (mid + (l - r) / 2.0 * sin2) + pilot * sin1
            } else {
                audio * mid
            };
            let baseband = self.rds.sample(self.cycles);
            mpx += rds_level * baseband * sin3;
            *sample = (mpx.clamp(-1.0, 1.0) * gain) as f32;
            self.cycles += self.cycles_per_sample;
            if self.cycles >= CYCLES_PER_BIT {
                self.cycles -= CYCLES_PER_BIT;
                self.rds.next_bit();
            }
        }
    }
}

/// Counters the control side can read.
#[derive(Debug, Default)]
pub struct MpxStats {
    /// Input frames that were missing (the on-air card stopped or fell behind).
    pub underruns: AtomicU64,
    /// Frames dropped or repeated to follow the clock drift.
    pub adjustments: AtomicU64,
    pub running: AtomicBool,
    /// The card reported an error (unplugged, for example): close and reopen it.
    pub failed: AtomicBool,
}

/// What the callback owns; handed back on close, so the tap can be used again.
struct Callback {
    encoder: MpxEncoder,
    tap: StreamTap,
    messages: rtrb::Consumer<MpxMsg>,
    stats: Arc<MpxStats>,
    stop: Arc<AtomicBool>,
    /// MPX samples of the current input frame not written yet.
    pending: [f32; FACTOR],
    next: usize,
    /// The last input frame, repeated when the input runs slow.
    last: (f32, f32),
    filling: bool,
}

impl Callback {
    fn input_frames(&self) -> usize {
        self.tap.consumer.slots() / (self.tap.channels.max(1) as usize)
    }

    fn pop_frame(&mut self) -> Option<(f32, f32)> {
        let channels = self.tap.channels.max(1) as usize;
        if self.input_frames() == 0 {
            return None;
        }
        let consumer = &mut self.tap.consumer;
        let left = consumer.pop().unwrap_or(0.0);
        let right = if channels > 1 { consumer.pop().unwrap_or(0.0) } else { left };
        for _ in 2..channels {
            let _ = consumer.pop();
        }
        Some((left, right))
    }

    fn skip_frames(&mut self, frames: usize) {
        let samples = frames * self.tap.channels.max(1) as usize;
        if let Ok(chunk) = self.tap.consumer.read_chunk(samples.min(self.tap.consumer.slots())) {
            chunk.commit_all();
        }
    }

    /// Keep about `TARGET_FILL` seconds of input: wait for it at the start and after an
    /// underrun, throw away a backlog, and drop one frame when the input runs fast.
    /// Returns whether to repeat the last frame (input running slow).
    fn follow_clock(&mut self) -> bool {
        let target = (self.tap.sample_rate as f64 * TARGET_FILL) as usize;
        let fill = self.input_frames();
        if self.filling {
            if fill >= target {
                self.filling = false;
            }
            return false;
        }
        if fill > 4 * target {
            self.skip_frames(fill - target);
            self.stats.adjustments.fetch_add(1, Ordering::Relaxed);
        } else if fill > 2 * target {
            self.skip_frames(1);
            self.stats.adjustments.fetch_add(1, Ordering::Relaxed);
        } else if fill < target / 4 {
            self.stats.adjustments.fetch_add(1, Ordering::Relaxed);
            return true;
        }
        false
    }

    fn render<T: SizedSample + FromSample<f32>>(&mut self, data: &mut [T], channels: usize) {
        while let Ok(message) = self.messages.pop() {
            match message {
                MpxMsg::Params(params) => self.encoder.set_params(params),
                MpxMsg::Rds(frame) => self.encoder.set_rds(frame),
            }
        }
        // A stale backlog left from before the card opened goes at once.
        let mut repeat = self.follow_clock();
        for frame in data.chunks_mut(channels.max(1)) {
            if self.next >= FACTOR {
                let input = if repeat {
                    repeat = false;
                    Some(self.last)
                } else if self.filling {
                    Some((0.0, 0.0))
                } else {
                    self.pop_frame()
                };
                let (left, right) = input.unwrap_or_else(|| {
                    self.stats.underruns.fetch_add(1, Ordering::Relaxed);
                    self.filling = true;
                    (0.0, 0.0)
                });
                self.last = (left, right);
                self.encoder.process(left, right, &mut self.pending);
                self.next = 0;
            }
            let value = T::from_sample(self.pending.get(self.next).copied().unwrap_or(0.0));
            frame.iter_mut().for_each(|s| *s = value);
            self.next += 1;
        }
    }
}

/// The stereo encoder playing on a sound card. Messages are sent from one thread (the
/// RDS worker); `close` gives the tap back.
pub struct MpxOutput {
    messages: rtrb::Producer<MpxMsg>,
    device_name: String,
    sample_rate: u32,
    stats: Arc<MpxStats>,
    stop: Arc<AtomicBool>,
    returned: Receiver<Box<Callback>>,
    shutdown: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl MpxOutput {
    /// Open `device` (the default card when `None`) at four times the tap's rate. On
    /// failure the tap comes back with the error.
    pub fn open(
        tap: StreamTap,
        device: Option<&str>,
        params: MpxParams,
        frame: RdsFrame,
    ) -> Result<Self, (String, StreamTap)> {
        let encoder = MpxEncoder::new(tap.sample_rate, params, frame);
        let rate = encoder.out_rate();
        let (producer, consumer) = rtrb::RingBuffer::new(64);
        let stats = Arc::new(MpxStats::default());
        let stop = Arc::new(AtomicBool::new(false));
        let callback = Box::new(Callback {
            encoder,
            tap,
            messages: consumer,
            stats: Arc::clone(&stats),
            stop: Arc::clone(&stop),
            pending: [0.0; FACTOR],
            next: FACTOR,
            last: (0.0, 0.0),
            filling: true,
        });
        let (ready_tx, ready_rx) = bounded::<Result<String, (String, Box<Callback>)>>(1);
        let (returned_tx, returned_rx) = bounded::<Box<Callback>>(1);
        let (shutdown_tx, shutdown_rx) = bounded::<()>(0);
        let name = device.map(str::to_string);
        let thread = std::thread::Builder::new()
            .name("riars-mpx".into())
            .spawn(move || {
                // A spare sender, kept here: the callback's copy may be dropped with it.
                let keep = returned_tx.clone();
                match start(callback, name.as_deref(), rate, returned_tx) {
                    Ok((stream, device_name)) => {
                        let _ = ready_tx.send(Ok(device_name));
                        let _ = shutdown_rx.recv();
                        drop(stream);
                    }
                    Err((e, callback)) => {
                        let _ = ready_tx.send(Err((e, callback)));
                    }
                }
                drop(keep);
            });
        let thread = match thread {
            Ok(thread) => thread,
            Err(e) => return Err((format!("Cannot start the MPX thread: {e}"), take_tap_back(ready_rx))),
        };
        // A driver that hangs while opening must not hang the RDS worker with it.
        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(device_name)) => {
                stats.running.store(true, Ordering::Relaxed);
                log::info!("MPX output: {device_name}, {rate} Hz");
                Ok(Self {
                    messages: producer,
                    device_name,
                    sample_rate: rate,
                    stats,
                    stop,
                    returned: returned_rx,
                    shutdown: Some(shutdown_tx),
                    thread: Some(thread),
                })
            }
            Ok(Err((e, callback))) => {
                let _ = thread.join();
                // A stream that failed to build or start dropped the real callback, which
                // came back through `returned`; `callback` is then a stand-in.
                let tap = returned_rx.try_recv().map_or(callback.tap, |c| c.tap);
                Err((e, tap))
            }
            Err(_) => Err(("The sound card did not start within 10 seconds".to_string(), dead_tap())),
        }
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn stats(&self) -> &MpxStats {
        &self.stats
    }

    /// False when the queue is full (the callback is not running).
    pub fn send(&mut self, message: MpxMsg) -> bool {
        self.messages.push(message).is_ok()
    }

    /// Stop the card and take the tap back (lost if the callback no longer runs).
    pub fn close(mut self) -> Option<StreamTap> {
        self.stop.store(true, Ordering::Relaxed);
        // A running callback hands itself back within one buffer; a dead stream hands it
        // back when the thread drops it.
        let mut callback = self.returned.recv_timeout(Duration::from_millis(500)).ok();
        self.shutdown.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if callback.is_none() {
            callback = self.returned.try_recv().ok();
        }
        self.stats.running.store(false, Ordering::Relaxed);
        callback.map(|c| c.tap)
    }
}

impl Drop for MpxOutput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.shutdown.take();
    }
}

/// Only reached if the thread could not even start: the callback (and its tap) went with
/// the closure. Give back an empty tap so the caller's types stay simple.
fn take_tap_back(ready: Receiver<Result<String, (String, Box<Callback>)>>) -> StreamTap {
    match ready.try_recv() {
        Ok(Err((_, callback))) => callback.tap,
        _ => dead_tap(),
    }
}

fn dead_tap() -> StreamTap {
    let (_, consumer) = rtrb::RingBuffer::new(1);
    StreamTap { consumer, sample_rate: 48_000, channels: 2 }
}

/// Sound cards that can play at `rate` (for the settings).
pub fn devices_supporting(rate: u32) -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else { return Vec::new() };
    devices
        .filter(|d| pick_config(d, rate).is_some())
        .filter_map(|d| d.name().ok())
        .collect()
}

fn pick_config(device: &cpal::Device, rate: u32) -> Option<cpal::SupportedStreamConfig> {
    let configs = device.supported_output_configs().ok()?;
    let mut fits: Vec<_> = configs
        .filter(|c| c.min_sample_rate().0 <= rate && rate <= c.max_sample_rate().0)
        .filter(|c| matches!(c.sample_format(), cpal::SampleFormat::F32 | cpal::SampleFormat::I16 | cpal::SampleFormat::I32))
        .collect();
    // Fewest channels first (the MPX is mono), then the widest sample format.
    fits.sort_by_key(|c| (c.channels(), std::cmp::Reverse(c.sample_format().sample_size())));
    fits.into_iter().next().map(|c| c.with_sample_rate(cpal::SampleRate(rate)))
}

type Started = Result<(cpal::Stream, String), (String, Box<Callback>)>;

fn start(callback: Box<Callback>, name: Option<&str>, rate: u32, returned: Sender<Box<Callback>>) -> Started {
    let host = cpal::default_host();
    let device = match name {
        Some(wanted) => host.output_devices().ok().and_then(|mut d| d.find(|d| d.name().is_ok_and(|n| n == wanted))),
        None => host.default_output_device(),
    };
    let Some(device) = device else {
        return Err((format!("Sound card \"{}\" not found", name.unwrap_or("default")), callback));
    };
    let device_name = device.name().unwrap_or_else(|_| "unknown".into());
    let Some(supported) = pick_config(&device, rate) else {
        return Err((format!("\"{device_name}\" cannot play at {rate} Hz"), callback));
    };
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let stream = match format {
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, callback, returned),
        cpal::SampleFormat::I32 => build::<i32>(&device, &config, callback, returned),
        _ => build::<f32>(&device, &config, callback, returned),
    }?;
    if let Err(e) = stream.play() {
        return Err((format!("Cannot start the MPX output: {e}"), dead_callback()));
    }
    Ok((stream, device_name))
}

/// The callback went into a stream that failed to start: its tap is lost.
fn dead_callback() -> Box<Callback> {
    let (_, messages) = rtrb::RingBuffer::new(1);
    Box::new(Callback {
        encoder: MpxEncoder::new(48_000, MpxParams::default(), RdsFrame::default()),
        tap: dead_tap(),
        messages,
        stats: Arc::default(),
        stop: Arc::default(),
        pending: [0.0; FACTOR],
        next: FACTOR,
        last: (0.0, 0.0),
        filling: true,
    })
}

/// Owns the callback inside the stream and sends it back once: when asked to stop, or
/// when the stream is dropped without having run (a card that failed). `try_send` of a
/// Box neither allocates nor blocks.
struct Holder {
    slot: Option<Box<Callback>>,
    returned: Sender<Box<Callback>>,
}

impl Holder {
    fn give_back(&mut self) {
        if let Some(callback) = self.slot.take() {
            let _ = self.returned.try_send(callback);
        }
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        self.give_back();
    }
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    callback: Box<Callback>,
    returned: Sender<Box<Callback>>,
) -> Result<cpal::Stream, (String, Box<Callback>)> {
    let channels = config.channels as usize;
    let stop = Arc::clone(&callback.stop);
    let stats = Arc::clone(&callback.stats);
    let mut holder = Holder { slot: Some(callback), returned };
    let result = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            if stop.load(Ordering::Relaxed) {
                holder.give_back();
            }
            match holder.slot.as_mut() {
                Some(callback) => callback.render(data, channels),
                None => data.iter_mut().for_each(|s| *s = T::EQUILIBRIUM),
            }
        },
        move |err| {
            log::error!("MPX output error: {err}");
            stats.failed.store(true, Ordering::Relaxed);
        },
        None,
    );
    result.map_err(|e| (format!("Cannot open the MPX output: {e}"), dead_callback()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rds::decode;

    fn frame() -> RdsFrame {
        let mut rt = [b' '; 64];
        rt[..19].copy_from_slice(b"Rosalia - Malamente");
        RdsFrame {
            pi: 0xE2A1,
            pty: 10,
            ps: *b"RENOU   ",
            rt,
            rt_len: 19,
            stereo: true,
            ..RdsFrame::default()
        }
    }

    /// `seconds` of MPX from a 48 kHz input made by `input(t)`.
    fn render(params: MpxParams, seconds: f64, input: impl Fn(f64) -> (f32, f32)) -> Vec<f32> {
        let mut encoder = MpxEncoder::new(48_000, params, frame());
        let mut out = Vec::new();
        let mut buf = [0.0; FACTOR];
        for n in 0..(48_000.0 * seconds) as usize {
            let (l, r) = input(n as f64 / 48_000.0);
            encoder.process(l, r, &mut buf);
            out.extend_from_slice(&buf);
        }
        out
    }

    /// Amplitude of the component at `freq` (Goertzel over the whole signal).
    fn amplitude(signal: &[f32], rate: f64, freq: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, x) in signal.iter().enumerate() {
            let w = TAU * freq * n as f64 / rate;
            re += *x as f64 * w.cos();
            im += *x as f64 * w.sin();
        }
        2.0 * (re * re + im * im).sqrt() / signal.len() as f64
    }

    fn unity(params: MpxParams) -> MpxParams {
        MpxParams { gain: 1.0, pre_emphasis: PreEmphasis::Off, ..params }
    }

    #[test]
    fn the_rds_comes_back_out_of_the_mpx() {
        let tone = |t: f64| (0.3 * (TAU * 1000.0 * t).sin() as f32, 0.3 * (TAU * 440.0 * t).sin() as f32);
        let mpx = render(MpxParams::default(), 3.0, tone);
        let groups = decode::groups_from_bits(&decode::demodulate(&mpx, 192_000));
        let decoded = decode::parse(&groups);
        // 1187.5 bit/s is 11.4 groups a second.
        assert!(decoded.groups > 25, "only {} groups", decoded.groups);
        assert_eq!(decoded.pi, Some(0xE2A1));
        assert_eq!(decoded.pty, Some(10));
        assert_eq!(decoded.ps_text().as_deref(), Some("RENOU   "));
        assert_eq!(decoded.rt_text().as_deref(), Some("Rosalia - Malamente"));
        assert_eq!(decoded.di[3], Some(true), "stereo flag");
    }

    #[test]
    fn pilot_and_subcarrier_levels() {
        let params = unity(MpxParams { rds: false, ..MpxParams::default() });
        // Left only: L+R and L−R both carry the tone.
        let left = render(params, 1.0, |t| (0.5 * (TAU * 1000.0 * t).sin() as f32, 0.0));
        let audio = 1.0 - 0.09;
        assert!((amplitude(&left, 192_000.0, 19_000.0) - 0.09).abs() < 0.005, "pilot");
        assert!((amplitude(&left, 192_000.0, 1_000.0) - audio * 0.25).abs() < 0.01, "L+R");
        // DSB-SC: (L−R)/2 · sin 2θ splits into two sidebands of half the amplitude.
        assert!((amplitude(&left, 192_000.0, 39_000.0) - audio * 0.125).abs() < 0.01, "L−R");
        // The same tone on both sides leaves nothing at 38 kHz ± 1 kHz: separation.
        let both = render(params, 1.0, |t| {
            let x = 0.5 * (TAU * 1000.0 * t).sin() as f32;
            (x, x)
        });
        assert!(amplitude(&both, 192_000.0, 39_000.0) < 0.001, "separation");
        // Mono: no pilot.
        let mono = render(unity(MpxParams { stereo: false, rds: false, ..MpxParams::default() }), 1.0, |_| (0.0, 0.0));
        assert!(amplitude(&mono, 192_000.0, 19_000.0) < 1e-6);
    }

    #[test]
    fn loud_noise_stays_inside_full_deviation_and_off_the_pilot() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut noise = move |_t: f64| {
            let mut next = || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 40) as f32 / (1u64 << 23) as f32 - 1.0
            };
            (2.0 * next(), 2.0 * next())
        };
        let params = MpxParams { gain: 1.0, ..MpxParams::default() };
        let mut encoder = MpxEncoder::new(48_000, params, frame());
        let mut peak = 0.0f32;
        let mut buf = [0.0; FACTOR];
        let mut quiet = Vec::new();
        for n in 0..48_000 {
            let (l, r) = noise(n as f64);
            encoder.process(l, r, &mut buf);
            peak = buf.iter().fold(peak, |p, x| p.max(x.abs()));
            quiet.extend_from_slice(&buf);
        }
        assert!(peak <= 1.0 + 1e-3, "peak {peak}");
        // Without pilot and RDS, audio noise near 19 kHz must be far below the 9 % pilot.
        let mut encoder = MpxEncoder::new(48_000, MpxParams { pilot: 0.0, rds: false, gain: 1.0, ..params }, frame());
        quiet.clear();
        for n in 0..48_000 {
            let (l, r) = noise(n as f64);
            encoder.process(l, r, &mut buf);
            quiet.extend_from_slice(&buf);
        }
        let near_pilot = amplitude(&quiet, 192_000.0, 19_000.0);
        assert!(near_pilot < 0.09 / 100.0, "audio at 19 kHz: {near_pilot}");
    }

    #[test]
    fn slow_input_repeats_the_last_frame_instead_of_silence() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(48_000);
        let params = unity(MpxParams { stereo: false, rds: false, ..MpxParams::default() });
        let (_messages_in, messages) = rtrb::RingBuffer::new(4);
        let mut callback = Callback {
            encoder: MpxEncoder::new(48_000, params, frame()),
            tap: StreamTap { consumer, sample_rate: 48_000, channels: 2 },
            messages,
            stats: Arc::default(),
            stop: Arc::default(),
            pending: [0.0; FACTOR],
            next: FACTOR,
            last: (0.0, 0.0),
            filling: false,
        };
        // A steady DC input, little of it buffered (under a quarter of the target), and as
        // much arriving as each callback takes: every callback repeats one frame.
        let mut push = |frames: usize| {
            for _ in 0..2 * frames {
                producer.push(0.5).unwrap();
            }
        };
        let mut out = vec![0.0f32; 4 * 1_000];
        let mut all = Vec::new();
        for _ in 0..20 {
            push(1_000);
            callback.render(&mut out, 1);
            all.extend_from_slice(&out);
        }
        assert!(callback.stats.adjustments.load(Ordering::Relaxed) >= 19, "never ran slow");
        // Once the filter has settled, every sample is the DC level: a repeated frame keeps
        // it, a silent one would dip.
        let settled = &all[4 * 200..];
        let dip = settled.iter().fold(1.0f32, |m, x| m.min(*x));
        assert!(settled.iter().all(|x| (x - 0.5).abs() < 0.002), "dip to {dip}");
    }

    #[test]
    fn interpolation_keeps_the_level_in_the_audio_band() {
        let params = unity(MpxParams { stereo: false, rds: false, ..MpxParams::default() });
        for freq in [100.0, 1_000.0, 10_000.0, 15_000.0] {
            let out = render(params, 0.5, |t| {
                let x = 0.5 * (TAU * freq * t).sin() as f32;
                (x, x)
            });
            let level = amplitude(&out, 192_000.0, freq);
            assert!((level - 0.5).abs() < 0.03, "{freq} Hz: {level}");
        }
    }
}
