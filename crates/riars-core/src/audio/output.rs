//! Sound card output through cpal.
//!
//! The cpal stream lives on its own thread (it is not `Send`). The engine talks to the
//! audio callback through a bounded crossbeam channel for track changes and atomics for
//! play state, volume and position, so the callback never locks, allocates or does I/O.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use crossbeam_channel::{bounded, Receiver, Sender};

use super::player::TrackPlayer;

pub(crate) enum Command {
    Load(Box<TrackPlayer>),
    Seek(Duration),
}

/// State shared between the engine and the audio callback.
pub(crate) struct Shared {
    pub playing: AtomicBool,
    /// Set by the callback when the loaded track plays to the end.
    pub finished: AtomicBool,
    volume_bits: AtomicU32,
    pub position_ms: AtomicU64,
}

impl Shared {
    fn new(volume: f32) -> Self {
        Self {
            playing: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            volume_bits: AtomicU32::new(volume.to_bits()),
            position_ms: AtomicU64::new(0),
        }
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume_bits.store(volume.to_bits(), Ordering::Relaxed);
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }
}

pub(crate) struct AudioOutput {
    commands: Sender<Command>,
    /// Players replaced in the callback come back here to be freed off the audio thread.
    garbage: Receiver<Box<TrackPlayer>>,
    pub shared: Arc<Shared>,
    device_name: String,
    // Dropping the sender wakes the output thread so it drops the stream and exits.
    _shutdown: Sender<()>,
    _thread: JoinHandle<()>,
}

impl AudioOutput {
    /// Open the default output device and start a silent stream.
    pub fn open(volume: f32) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = bounded::<Command>(16);
        let (garbage_tx, garbage_rx) = bounded::<Box<TrackPlayer>>(16);
        let (shutdown_tx, shutdown_rx) = bounded::<()>(0);
        let (ready_tx, ready_rx) = bounded::<Result<String, String>>(1);
        let shared = Arc::new(Shared::new(volume));
        let callback_shared = Arc::clone(&shared);

        let thread = std::thread::Builder::new()
            .name("riars-audio-output".into())
            .spawn(move || match start_stream(cmd_rx, garbage_tx, callback_shared) {
                Ok((stream, device_name)) => {
                    let _ = ready_tx.send(Ok(device_name));
                    let _ = shutdown_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| format!("Cannot start audio thread: {e}"))?;

        let device_name = ready_rx
            .recv()
            .map_err(|_| "Audio thread exited during start-up".to_string())??;

        Ok(Self {
            commands: cmd_tx,
            garbage: garbage_rx,
            shared,
            device_name,
            _shutdown: shutdown_tx,
            _thread: thread,
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn send(&self, command: Command) {
        while self.garbage.try_recv().is_ok() {}
        if self.commands.send(command).is_err() {
            log::error!("Audio output thread is gone; command dropped");
        }
    }
}

fn start_stream(
    commands: Receiver<Command>,
    garbage: Sender<Box<TrackPlayer>>,
    shared: Arc<Shared>,
) -> Result<(cpal::Stream, String), String> {
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

    let stream = match format {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, commands, garbage, shared),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, commands, garbage, shared),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, commands, garbage, shared),
        cpal::SampleFormat::I32 => build_stream::<i32>(&device, &config, commands, garbage, shared),
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
    Ok((stream, device_name))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    commands: Receiver<Command>,
    garbage: Sender<Box<TrackPlayer>>,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let out_channels = config.channels as usize;
    let out_rate = config.sample_rate.0 as f64;
    let mut player: Option<Box<TrackPlayer>> = None;

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                while let Ok(command) = commands.try_recv() {
                    match command {
                        Command::Load(new_player) => {
                            if let Some(old) = player.replace(new_player) {
                                let _ = garbage.try_send(old);
                            }
                            shared.finished.store(false, Ordering::Relaxed);
                        }
                        Command::Seek(position) => {
                            if let Some(p) = player.as_mut() {
                                p.seek(position);
                            }
                            shared.finished.store(false, Ordering::Relaxed);
                        }
                    }
                }

                let playing = shared.playing.load(Ordering::Relaxed);
                let volume = shared.volume();

                match player.as_mut() {
                    Some(p) if playing => {
                        let step = p.sample_rate() as f64 / out_rate;
                        for frame in data.chunks_mut(out_channels) {
                            for (channel, out) in frame.iter_mut().enumerate() {
                                *out = T::from_sample(p.sample(channel) * volume);
                            }
                            p.advance(step);
                        }
                        if p.is_finished() {
                            shared.playing.store(false, Ordering::Relaxed);
                            shared.finished.store(true, Ordering::Relaxed);
                        }
                    }
                    _ => data.fill(T::EQUILIBRIUM),
                }

                if let Some(p) = player.as_ref() {
                    shared
                        .position_ms
                        .store(p.position().as_millis() as u64, Ordering::Relaxed);
                }
            },
            |err| log::error!("Audio output error: {err}"),
            None,
        )
        .map_err(|e| format!("Cannot build output stream: {e}"))
}
