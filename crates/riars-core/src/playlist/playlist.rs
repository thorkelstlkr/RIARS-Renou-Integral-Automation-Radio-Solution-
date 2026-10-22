use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: String,
    pub track_path: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub items: Vec<PlaylistItem>,
    pub current_index: usize,
    pub repeat_mode: RepeatMode,
    pub shuffle: bool,
}

impl Playlist {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            items: Vec::new(),
            current_index: 0,
            repeat_mode: RepeatMode::Off,
            shuffle: false,
        }
    }

    pub fn add_item(&mut self, path: String, title: String, artist: String, duration_ms: u64) {
        self.items.push(PlaylistItem {
            id: Uuid::new_v4().to_string(),
            track_path: path,
            title,
            artist,
            duration_ms,
        });
    }

    pub fn remove_item(&mut self, index: usize) -> Option<PlaylistItem> {
        if index < self.items.len() {
            let item = self.items.remove(index);
            if self.current_index >= self.items.len() && !self.items.is_empty() {
                self.current_index = self.items.len() - 1;
            }
            Some(item)
        } else {
            None
        }
    }

    pub fn move_item(&mut self, from: usize, to: usize) {
        if from < self.items.len() && to < self.items.len() {
            let item = self.items.remove(from);
            self.items.insert(to, item);
        }
    }

    pub fn current_item(&self) -> Option<&PlaylistItem> {
        self.items.get(self.current_index)
    }

    pub fn advance(&mut self) -> Option<&PlaylistItem> {
        if self.items.is_empty() {
            return None;
        }
        match self.repeat_mode {
            RepeatMode::One => {}
            RepeatMode::All => {
                self.current_index = (self.current_index + 1) % self.items.len();
            }
            RepeatMode::Off => {
                if self.current_index + 1 < self.items.len() {
                    self.current_index += 1;
                } else {
                    return None;
                }
            }
        }
        self.items.get(self.current_index)
    }

    pub fn total_duration_ms(&self) -> u64 {
        self.items.iter().map(|i| i.duration_ms).sum()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
