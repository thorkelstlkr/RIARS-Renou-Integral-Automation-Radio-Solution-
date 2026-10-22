use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MixerChannel {
    pub name: String,
    pub volume: f32,
    pub pan: f32,
    pub muted: bool,
}

impl MixerChannel {
    pub fn new(name: String) -> Self {
        Self {
            name,
            volume: 1.0,
            pan: 0.0,
            muted: false,
        }
    }
}

pub struct Mixer {
    channels: Vec<MixerChannel>,
    master_volume: f32,
    sample_rate: u32,
}

impl Mixer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            channels: vec![
                MixerChannel::new("Main".to_string()),
                MixerChannel::new("Aux 1".to_string()),
            ],
            master_volume: 1.0,
            sample_rate,
        }
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
    }

    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> &[MixerChannel] {
        &self.channels
    }

    pub fn set_channel_volume(&mut self, index: usize, volume: f32) {
        if let Some(ch) = self.channels.get_mut(index) {
            ch.volume = volume.clamp(0.0, 1.0);
        }
    }

    pub fn set_channel_mute(&mut self, index: usize, muted: bool) {
        if let Some(ch) = self.channels.get_mut(index) {
            ch.muted = muted;
        }
    }

    pub fn apply_master(&self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            *sample *= self.master_volume;
        }
    }
}
