use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamFormat {
    Mp3,
    Ogg,
    Aac,
}

/// MP3 encoder effort. `Fast` keeps CPU low on small machines; at 128 kbps and up
/// the difference is hard to hear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EncoderQuality {
    #[default]
    Fast,
    Balanced,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcecastConfig {
    pub host: String,
    pub port: u16,
    pub mount: String,
    pub username: String,
    pub password: String,
    pub bitrate: u32,
    pub format: StreamFormat,
    pub station_name: String,
    pub station_description: String,
    pub station_genre: String,
    pub station_url: String,
    #[serde(default)]
    pub encoder_quality: EncoderQuality,
}

impl Default for IcecastConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8000,
            mount: "/stream".to_string(),
            username: "source".to_string(),
            password: String::new(),
            bitrate: 128,
            format: StreamFormat::Mp3,
            station_name: "RIARS Station".to_string(),
            station_description: String::new(),
            station_genre: String::new(),
            station_url: String::new(),
            encoder_quality: EncoderQuality::default(),
        }
    }
}
