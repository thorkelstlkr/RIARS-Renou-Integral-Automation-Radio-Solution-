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
            // Keep pointing at the same item when an earlier one is removed.
            if index < self.current_index {
                self.current_index -= 1;
            }
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
            let current = self.current_index;
            if current == from {
                self.current_index = to;
            } else if from < current && to >= current {
                self.current_index -= 1;
            } else if from > current && to <= current {
                self.current_index += 1;
            }
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current_index = 0;
    }

    pub fn current_item(&self) -> Option<&PlaylistItem> {
        self.items.get(self.current_index)
    }

    pub fn advance(&mut self) -> Option<&PlaylistItem> {
        self.current_index = self.next_index()?;
        self.items.get(self.current_index)
    }

    /// Index `advance` would move to, without moving.
    pub fn next_index(&self) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        match self.repeat_mode {
            RepeatMode::One => Some(self.current_index),
            RepeatMode::All => Some((self.current_index + 1) % self.items.len()),
            RepeatMode::Off => {
                Some(self.current_index + 1).filter(|&i| i < self.items.len())
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn abcd(current: usize) -> Playlist {
        let mut p = Playlist::new("Test".to_string());
        for t in ["a", "b", "c", "d"] {
            p.add_item(format!("{t}.mp3"), t.to_string(), String::new(), 0);
        }
        p.current_index = current;
        p
    }

    fn current(p: &Playlist) -> &str {
        &p.current_item().unwrap().title
    }

    #[test]
    fn remove_before_current_keeps_current() {
        let mut p = abcd(2);
        p.remove_item(0);
        assert_eq!(current(&p), "c");
    }

    #[test]
    fn move_current_follows_item() {
        let mut p = abcd(1);
        p.move_item(1, 3);
        assert_eq!(p.current_index, 3);
        assert_eq!(current(&p), "b");
    }

    #[test]
    fn move_across_current_keeps_current() {
        let mut p = abcd(1);
        p.move_item(0, 2);
        assert_eq!(current(&p), "b");
        p.move_item(3, 0);
        assert_eq!(current(&p), "b");
    }

    #[test]
    fn advance_respects_repeat_mode() {
        let mut p = abcd(3);
        assert!(p.advance().is_none());
        p.repeat_mode = RepeatMode::All;
        assert_eq!(p.advance().unwrap().title, "a");
    }
}
