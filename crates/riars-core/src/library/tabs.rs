//! Playlist tabs: lists open for editing next to the one on air. Each tab keeps a whole
//! [`Playlist`] (every item field, unlike saved playlists) and an optional planned start
//! time, so a programme's inserts can be laid out ahead. The tab on air is the engine's
//! playlist (same id); its stored copy is refreshed when it leaves the air.

use chrono::NaiveTime;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::db::MediaLibrary;
use crate::playlist::playlist::Playlist;

/// Longest tab name, in characters.
pub const MAX_TAB_NAME: usize = 80;
/// Most tabs open at once.
pub const MAX_TABS: usize = 30;

/// A tab, without its items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistTab {
    pub id: String,
    pub name: String,
    /// When the list is planned to start ("HH:MM:SS"), for its Start column.
    pub start_time: Option<String>,
    pub items: usize,
    pub duration_ms: u64,
}

pub(crate) fn init_schema(db: &rusqlite::Connection) -> Result<(), String> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS playlist_tabs (
             id          TEXT PRIMARY KEY,
             name        TEXT NOT NULL,
             position    INTEGER NOT NULL,
             start_time  TEXT,
             playlist    TEXT NOT NULL,
             updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
         );",
    )
    .map_err(|e| format!("DB error: {e}"))
}

/// A trimmed, non-empty name of at most [`MAX_TAB_NAME`] characters.
fn clean_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A tab needs a name".to_string());
    }
    Ok(name.chars().take(MAX_TAB_NAME).collect())
}

/// "HH:MM" or "HH:MM:SS" as "HH:MM:SS".
pub fn parse_start_time(text: &str) -> Result<String, String> {
    let text = text.trim();
    NaiveTime::parse_from_str(text, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(text, "%H:%M"))
        .map(|t| t.format("%H:%M:%S").to_string())
        .map_err(|_| format!("Not a time of day: {text} (use HH:MM or HH:MM:SS)"))
}

impl MediaLibrary {
    /// The open tabs, in order.
    pub fn playlist_tabs(&self) -> Result<Vec<PlaylistTab>, String> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id, name, start_time, playlist FROM playlist_tabs ORDER BY position, rowid")
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, String>(3)?))
            })
            .map_err(|e| format!("DB error: {e}"))?;
        let mut tabs = Vec::new();
        for row in rows {
            let (id, name, start_time, json) = row.map_err(|e| format!("DB error: {e}"))?;
            let (items, duration_ms) = serde_json::from_str::<Playlist>(&json)
                .map(|p| (p.items.len(), p.total_duration_ms()))
                .unwrap_or((0, 0));
            tabs.push(PlaylistTab { id, name, start_time, items, duration_ms });
        }
        Ok(tabs)
    }

    /// The stored list of a tab.
    pub fn tab_playlist(&self, id: &str) -> Result<Playlist, String> {
        let (name, json): (String, String) = self
            .conn()
            .query_row("SELECT name, playlist FROM playlist_tabs WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(|e| format!("DB error: {e}"))?
            .ok_or_else(|| format!("No playlist tab {id}"))?;
        let mut playlist: Playlist = serde_json::from_str(&json).map_err(|e| format!("Bad playlist tab {id}: {e}"))?;
        playlist.id = id.to_string();
        playlist.name = name;
        Ok(playlist)
    }

    /// Store a tab's list (its id and name come from `playlist`), adding the tab at the
    /// end when it is new. Fails when [`MAX_TABS`] are already open.
    pub fn save_tab(&mut self, playlist: &Playlist) -> Result<(), String> {
        let name = clean_name(&playlist.name)?;
        let json = serde_json::to_string(playlist).map_err(|e| e.to_string())?;
        let db = self.conn();
        let exists = db
            .query_row("SELECT 1 FROM playlist_tabs WHERE id = ?1", params![playlist.id], |_| Ok(()))
            .optional()
            .map_err(|e| format!("DB error: {e}"))?
            .is_some();
        if exists {
            db.execute(
                "UPDATE playlist_tabs SET name = ?2, playlist = ?3, updated_at = datetime('now') WHERE id = ?1",
                params![playlist.id, name, json],
            )
        } else {
            let open: i64 = db
                .query_row("SELECT COUNT(*) FROM playlist_tabs", [], |r| r.get(0))
                .map_err(|e| format!("DB error: {e}"))?;
            if open as usize >= MAX_TABS {
                return Err(format!("At most {MAX_TABS} playlist tabs can be open"));
            }
            db.execute(
                "INSERT INTO playlist_tabs (id, name, position, playlist)
                 VALUES (?1, ?2, (SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tabs), ?3)",
                params![playlist.id, name, json],
            )
        }
        .map_err(|e| format!("DB error: {e}"))?;
        Ok(())
    }

    /// A new, empty tab named `name`; returns its list.
    pub fn new_tab(&mut self, name: &str) -> Result<Playlist, String> {
        let playlist = Playlist::new(clean_name(name)?);
        self.save_tab(&playlist)?;
        Ok(playlist)
    }

    pub fn rename_tab(&mut self, id: &str, name: &str) -> Result<(), String> {
        let mut playlist = self.tab_playlist(id)?;
        playlist.name = clean_name(name)?;
        self.save_tab(&playlist)
    }

    /// Set or clear (`None`) when the tab's list is planned to start.
    pub fn set_tab_start(&mut self, id: &str, start: Option<&str>) -> Result<Option<String>, String> {
        let start = start.filter(|s| !s.trim().is_empty()).map(parse_start_time).transpose()?;
        let rows = self
            .conn()
            .execute("UPDATE playlist_tabs SET start_time = ?2 WHERE id = ?1", params![id, start])
            .map_err(|e| format!("DB error: {e}"))?;
        if rows == 0 {
            return Err(format!("No playlist tab {id}"));
        }
        Ok(start)
    }

    /// Close a tab; its list is gone (save it as a playlist first to keep it).
    pub fn close_tab(&mut self, id: &str) -> Result<(), String> {
        let rows = self
            .conn()
            .execute("DELETE FROM playlist_tabs WHERE id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        if rows == 0 {
            return Err(format!("No playlist tab {id}"));
        }
        Ok(())
    }

    /// Put the tabs in this order; ids not given keep their place after them.
    pub fn order_tabs(&mut self, ids: &[String]) -> Result<(), String> {
        let tx = self.conn_mut().transaction().map_err(|e| format!("DB error: {e}"))?;
        let rest: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT id FROM playlist_tabs ORDER BY position, rowid")
                .map_err(|e| format!("DB error: {e}"))?;
            let all = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| format!("DB error: {e}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("DB error: {e}"))?;
            all.into_iter().filter(|id| !ids.contains(id)).collect()
        };
        for (position, id) in ids.iter().chain(rest.iter()).enumerate() {
            tx.execute("UPDATE playlist_tabs SET position = ?2 WHERE id = ?1", params![id, position as i64])
                .map_err(|e| format!("DB error: {e}"))?;
        }
        tx.commit().map_err(|e| format!("DB error: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(name: &str, paths: &[&str]) -> Playlist {
        let mut p = Playlist::new(name.to_string());
        for path in paths {
            p.add_item(path.to_string(), path.to_string(), String::new(), 1000);
        }
        p
    }

    #[test]
    fn tabs_keep_their_lists_order_and_start_time() {
        let mut lib = MediaLibrary::open(":memory:").unwrap();
        let mut a = list("Mañana", &["/a.mp3"]);
        a.items[0].stop_after = true;
        lib.save_tab(&a).unwrap();
        let b = lib.new_tab("  Programa  ").unwrap();
        assert_eq!(b.name, "Programa");

        let tabs = lib.playlist_tabs().unwrap();
        assert_eq!(tabs.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Mañana", "Programa"]);
        assert_eq!((tabs[0].items, tabs[0].duration_ms), (1, 1000));

        let back = lib.tab_playlist(&a.id).unwrap();
        assert!(back.items[0].stop_after, "every item field is kept");

        assert_eq!(lib.set_tab_start(&b.id, Some("10:00")).unwrap().as_deref(), Some("10:00:00"));
        assert!(lib.set_tab_start(&b.id, Some("25:00")).is_err());
        assert_eq!(lib.playlist_tabs().unwrap()[1].start_time.as_deref(), Some("10:00:00"));
        assert_eq!(lib.set_tab_start(&b.id, Some(" ")).unwrap(), None);

        lib.order_tabs(&[b.id.clone()]).unwrap();
        assert_eq!(lib.playlist_tabs().unwrap()[0].id, b.id);

        lib.rename_tab(&b.id, "Tarde").unwrap();
        assert_eq!(lib.tab_playlist(&b.id).unwrap().name, "Tarde");
        assert!(lib.rename_tab(&b.id, "   ").is_err());

        lib.close_tab(&a.id).unwrap();
        assert_eq!(lib.playlist_tabs().unwrap().len(), 1);
        assert!(lib.tab_playlist(&a.id).is_err());
        assert!(lib.close_tab(&a.id).is_err());
    }

    #[test]
    fn at_most_max_tabs() {
        let mut lib = MediaLibrary::open(":memory:").unwrap();
        for i in 0..MAX_TABS {
            lib.new_tab(&format!("T{i}")).unwrap();
        }
        assert!(lib.new_tab("one more").is_err());
    }
}
