use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamFormat {
    Mp3,
    Ogg,
    Aac,
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
        }
    }
}
