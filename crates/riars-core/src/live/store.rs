//! SQLite storage for sound banks (cartwall).

use rusqlite::{params, OptionalExtension};

use crate::library::db::MediaLibrary;

use super::model::{CartBank, CartBankSummary, CartButton, CartMode};

const ACTIVE_BANK_KEY: &str = "live_active_bank";
const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub(crate) fn init_live_schema(db: &rusqlite::Connection) -> Result<(), String> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS cart_banks (
            id         INTEGER PRIMARY KEY,
            name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS cart_buttons (
            bank_id   INTEGER NOT NULL REFERENCES cart_banks(id) ON DELETE CASCADE,
            page      INTEGER NOT NULL,
            slot      INTEGER NOT NULL,
            track_id  INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
            file_path TEXT NOT NULL,
            label     TEXT NOT NULL,
            color     TEXT NOT NULL,
            gain_db   REAL NOT NULL,
            mode      TEXT NOT NULL,
            duck      INTEGER NOT NULL,
            PRIMARY KEY (bank_id, page, slot)
        );",
    )
    .map_err(|e| format!("DB error: {e}"))
}

fn mode_text(mode: CartMode) -> &'static str {
    match mode {
        CartMode::Fire => "Fire",
        CartMode::Toggle => "Toggle",
        CartMode::Loop => "Loop",
    }
}

fn mode_from_text(text: &str) -> CartMode {
    match text {
        "Toggle" => CartMode::Toggle,
        "Loop" => CartMode::Loop,
        _ => CartMode::Fire,
    }
}

fn now_text() -> String {
    chrono::Local::now().format(DATETIME_FORMAT).to_string()
}

fn button_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CartButton> {
    let mode: String = row.get(7)?;
    Ok(CartButton {
        page: row.get(0)?,
        slot: row.get(1)?,
        track_id: row.get(2)?,
        file_path: row.get(3)?,
        label: row.get(4)?,
        color: row.get(5)?,
        gain_db: row.get::<_, f64>(6)? as f32,
        mode: mode_from_text(&mode),
        duck: row.get::<_, i64>(8)? != 0,
    })
}

impl MediaLibrary {
    pub fn cart_banks(&self) -> Result<Vec<CartBankSummary>, String> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT b.id, b.name, COUNT(btn.bank_id)
                 FROM cart_banks b
                 LEFT JOIN cart_buttons btn ON btn.bank_id = b.id
                 GROUP BY b.id, b.name
                 ORDER BY b.name COLLATE NOCASE, b.id",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CartBankSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    buttons: row.get::<_, i64>(2)? as usize,
                })
            })
            .map_err(|e| format!("DB error: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("DB error: {e}"))
    }

    pub fn cart_bank(&self, id: i64) -> Result<Option<CartBank>, String> {
        let name: Option<String> = self
            .conn()
            .query_row("SELECT name FROM cart_banks WHERE id = ?1", params![id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| format!("DB error: {e}"))?;
        let Some(name) = name else {
            return Ok(None);
        };
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT page, slot, track_id, file_path, label, color, gain_db, mode, duck
                 FROM cart_buttons WHERE bank_id = ?1 ORDER BY page, slot",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let rows = stmt
            .query_map(params![id], button_from_row)
            .map_err(|e| format!("DB error: {e}"))?;
        let buttons = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(Some(CartBank { id, name, buttons }))
    }

    pub fn save_cart_bank(&mut self, bank: &CartBank) -> Result<i64, String> {
        bank.validate()?;
        let name = bank.name.trim();
        let id = {
            let tx = self
                .conn_mut()
                .transaction()
                .map_err(|e| format!("DB error: {e}"))?;
            let duplicate: Option<i64> = tx
                .query_row(
                    "SELECT id FROM cart_banks WHERE name = ?1 COLLATE NOCASE AND id <> ?2",
                    params![name, bank.id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| format!("DB error: {e}"))?;
            if duplicate.is_some() {
                return Err(format!("A bank named {name} already exists"));
            }
            let updated_at = now_text();
            let id = if bank.id == 0 {
                tx.execute(
                    "INSERT INTO cart_banks (name, updated_at) VALUES (?1, ?2)",
                    params![name, updated_at],
                )
                .map_err(|e| format!("DB error: {e}"))?;
                tx.last_insert_rowid()
            } else {
                let rows = tx
                    .execute(
                        "UPDATE cart_banks SET name = ?1, updated_at = ?2 WHERE id = ?3",
                        params![name, updated_at, bank.id],
                    )
                    .map_err(|e| format!("DB error: {e}"))?;
                if rows == 0 {
                    return Err(format!("No bank {}", bank.id));
                }
                bank.id
            };
            tx.execute("DELETE FROM cart_buttons WHERE bank_id = ?1", params![id])
                .map_err(|e| format!("DB error: {e}"))?;
            for button in &bank.buttons {
                insert_button(&tx, id, button)?;
            }
            tx.commit().map_err(|e| format!("DB error: {e}"))?;
            id
        };
        self.bump_revision();
        Ok(id)
    }

    pub fn delete_cart_bank(&mut self, id: i64) -> Result<bool, String> {
        let rows = self
            .conn()
            .execute("DELETE FROM cart_banks WHERE id = ?1", params![id])
            .map_err(|e| format!("DB error: {e}"))?;
        if rows > 0 {
            self.bump_revision();
        }
        Ok(rows > 0)
    }

    pub fn set_cart_button(&mut self, bank_id: i64, button: &CartButton) -> Result<(), String> {
        button.validate()?;
        let exists = self
            .conn()
            .prepare("SELECT 1 FROM cart_banks WHERE id = ?1")
            .and_then(|mut stmt| stmt.exists(params![bank_id]))
            .map_err(|e| format!("DB error: {e}"))?;
        if !exists {
            return Err(format!("No cart bank {bank_id}"));
        }
        let tx = self
            .conn_mut()
            .transaction()
            .map_err(|e| format!("DB error: {e}"))?;
        insert_button(&tx, bank_id, button)?;
        tx.execute(
            "UPDATE cart_banks SET updated_at = ?1 WHERE id = ?2",
            params![now_text(), bank_id],
        )
        .map_err(|e| format!("DB error: {e}"))?;
        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        Ok(())
    }

    pub fn clear_cart_button(&mut self, bank_id: i64, page: u8, slot: u8) -> Result<bool, String> {
        let rows = self
            .conn()
            .execute(
                "DELETE FROM cart_buttons WHERE bank_id = ?1 AND page = ?2 AND slot = ?3",
                params![bank_id, page, slot],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        if rows > 0 {
            self.conn()
                .execute(
                    "UPDATE cart_banks SET updated_at = ?1 WHERE id = ?2",
                    params![now_text(), bank_id],
                )
                .map_err(|e| format!("DB error: {e}"))?;
            self.bump_revision();
        }
        Ok(rows > 0)
    }

    /// The bank shown in the On air panel; `None` if unset or deleted.
    pub fn active_cart_bank(&self) -> Result<Option<i64>, String> {
        let Some(text) = self.kv_get(ACTIVE_BANK_KEY)? else {
            return Ok(None);
        };
        if text.is_empty() {
            return Ok(None);
        }
        let Ok(id) = text.parse::<i64>() else {
            return Ok(None);
        };
        let exists = self
            .conn()
            .prepare("SELECT 1 FROM cart_banks WHERE id = ?1")
            .and_then(|mut stmt| stmt.exists(params![id]))
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(exists.then_some(id))
    }

    pub fn set_active_cart_bank(&mut self, id: Option<i64>) -> Result<(), String> {
        match id {
            Some(id) => self.kv_set(ACTIVE_BANK_KEY, &id.to_string()),
            None => self.kv_set(ACTIVE_BANK_KEY, ""),
        }
    }
}

fn insert_button(
    conn: &rusqlite::Connection,
    bank_id: i64,
    button: &CartButton,
) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO cart_buttons
            (bank_id, page, slot, track_id, file_path, label, color, gain_db, mode, duck)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            bank_id,
            button.page,
            button.slot,
            button.track_id,
            button.file_path,
            button.label.trim(),
            button.color,
            button.gain_db as f64,
            mode_text(button.mode),
            button.duck as i64
        ],
    )
    .map_err(|e| format!("DB error: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use rusqlite::params;

    use super::*;

    fn remove_db_files(path: &Path) {
        let mut wal = path.as_os_str().to_os_string();
        wal.push("-wal");
        let mut shm = path.as_os_str().to_os_string();
        shm.push("-shm");
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(PathBuf::from(wal));
        let _ = std::fs::remove_file(PathBuf::from(shm));
    }

    fn temp_db(name: &str) -> (MediaLibrary, PathBuf) {
        let path =
            std::env::temp_dir().join(format!("riars-live-{}-{name}.db", std::process::id()));
        remove_db_files(&path);
        let lib = MediaLibrary::open(&path.to_string_lossy()).unwrap();
        (lib, path)
    }

    fn insert_track(lib: &MediaLibrary, title: &str, file_path: &str) -> i64 {
        lib.conn()
            .execute(
                "INSERT INTO tracks (title, artist, album, genre, duration_ms, file_path)
                 VALUES (?1, '', '', '', 0, ?2)",
                params![title, file_path],
            )
            .unwrap();
        lib.conn().last_insert_rowid()
    }

    fn button(page: u8, slot: u8, label: &str) -> CartButton {
        CartButton {
            page,
            slot,
            track_id: None,
            file_path: "/sounds/clap.wav".to_string(),
            label: label.to_string(),
            color: "#ff8800".to_string(),
            gain_db: -3.0,
            mode: CartMode::Fire,
            duck: true,
        }
    }

    fn bank(name: &str, buttons: Vec<CartButton>) -> CartBank {
        CartBank { id: 0, name: name.to_string(), buttons }
    }

    #[test]
    fn save_and_read_bank() {
        let (mut lib, path) = temp_db("roundtrip");
        let original = bank("Breakfast", vec![button(0, 5, "Jingle"), button(1, 2, "Applause")]);
        let id = lib.save_cart_bank(&original).unwrap();
        assert!(id > 0);

        let mut expected = original.clone();
        expected.id = id;
        assert_eq!(lib.cart_bank(id).unwrap(), Some(expected));
        assert!(lib.cart_bank(id + 1).unwrap().is_none());

        let summaries = lib.cart_banks().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, id);
        assert_eq!(summaries[0].name, "Breakfast");
        assert_eq!(summaries[0].buttons, 2);

        drop(lib);
        remove_db_files(&path);
    }

    #[test]
    fn duplicate_name_in_another_case_fails() {
        let (mut lib, path) = temp_db("duplicate");
        let first = lib.save_cart_bank(&bank("Morning", vec![])).unwrap();
        let second = lib.save_cart_bank(&bank("Drive", vec![])).unwrap();

        let renamed = bank("mOrNiNg", vec![]);
        let renamed = CartBank { id: second, ..renamed };
        let err = lib.save_cart_bank(&renamed).unwrap_err();
        assert!(err.contains("A bank named mOrNiNg already exists"));
        assert_eq!(lib.cart_bank(first).unwrap().unwrap().name, "Morning");
        assert_eq!(lib.cart_bank(second).unwrap().unwrap().name, "Drive");

        drop(lib);
        remove_db_files(&path);
    }

    #[test]
    fn saving_again_replaces_all_buttons() {
        let (mut lib, path) = temp_db("replace");
        let id = lib
            .save_cart_bank(&bank("Bed", vec![button(0, 0, "One"), button(0, 1, "Two")]))
            .unwrap();
        let replaced = CartBank { id, name: "Bed".to_string(), buttons: vec![button(2, 3, "Three")] };
        assert_eq!(lib.save_cart_bank(&replaced).unwrap(), id);
        assert_eq!(lib.cart_bank(id).unwrap(), Some(replaced));

        drop(lib);
        remove_db_files(&path);
    }

    #[test]
    fn set_and_clear_one_button() {
        let (mut lib, path) = temp_db("set-clear");
        let id = lib.save_cart_bank(&bank("Bed", vec![])).unwrap();

        let first = button(3, 4, "Sweep");
        lib.set_cart_button(id, &first).unwrap();
        assert_eq!(lib.cart_bank(id).unwrap().unwrap().buttons, vec![first.clone()]);

        let mut second = button(3, 4, "Stinger");
        second.mode = CartMode::Loop;
        lib.set_cart_button(id, &second).unwrap();
        assert_eq!(lib.cart_bank(id).unwrap().unwrap().buttons, vec![second]);

        assert!(lib.clear_cart_button(id, 3, 4).unwrap());
        assert!(lib.cart_bank(id).unwrap().unwrap().buttons.is_empty());
        assert!(!lib.clear_cart_button(id, 3, 4).unwrap());

        assert!(lib.set_cart_button(9_999, &button(0, 0, "Lost")).is_err());

        drop(lib);
        remove_db_files(&path);
    }

    #[test]
    fn deleting_bank_deletes_buttons() {
        let (mut lib, path) = temp_db("delete-bank");
        let id = lib
            .save_cart_bank(&bank("Bed", vec![button(0, 0, "One"), button(1, 1, "Two")]))
            .unwrap();
        assert!(lib.delete_cart_bank(id).unwrap());
        assert!(lib.cart_bank(id).unwrap().is_none());
        let count: i64 = lib
            .conn()
            .query_row("SELECT COUNT(*) FROM cart_buttons WHERE bank_id = ?1", params![id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        assert!(!lib.delete_cart_bank(id).unwrap());

        drop(lib);
        remove_db_files(&path);
    }

    #[test]
    fn deleting_track_keeps_button_without_track() {
        let (mut lib, path) = temp_db("delete-track");
        let track = insert_track(&lib, "Song", "/music/song.mp3");
        let mut with_track = button(0, 0, "Song");
        with_track.track_id = Some(track);
        let id = lib.save_cart_bank(&bank("Bed", vec![with_track])).unwrap();

        assert!(lib.delete_track(track).unwrap());
        let loaded = lib.cart_bank(id).unwrap().unwrap();
        assert_eq!(loaded.buttons.len(), 1);
        assert_eq!(loaded.buttons[0].track_id, None);
        assert_eq!(loaded.buttons[0].label, "Song");

        drop(lib);
        remove_db_files(&path);
    }

    #[test]
    fn active_bank_lifecycle() {
        let (mut lib, path) = temp_db("active");
        assert_eq!(lib.active_cart_bank().unwrap(), None);

        let id = lib.save_cart_bank(&bank("Bed", vec![])).unwrap();
        lib.set_active_cart_bank(Some(id)).unwrap();
        assert_eq!(lib.active_cart_bank().unwrap(), Some(id));

        assert!(lib.delete_cart_bank(id).unwrap());
        assert_eq!(lib.active_cart_bank().unwrap(), None);

        lib.set_active_cart_bank(None).unwrap();
        assert_eq!(lib.active_cart_bank().unwrap(), None);

        drop(lib);
        remove_db_files(&path);
    }
}
