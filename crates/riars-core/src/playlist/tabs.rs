//! Playlist tabs across the engine and the library: several lists open for editing,
//! one of them on air (the engine's list, same id as its tab). Shared by the desktop
//! app, the REST API and the MCP server. The engine and the library are never locked
//! at the same time.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::audio::engine::AudioEngine;
use crate::library::db::MediaLibrary;
use crate::library::tabs::{PlaylistTab, MAX_TAB_NAME};
use crate::playlist::playlist::{Playlist, PlaylistItem};

/// The open tabs and which one is on air.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tabs {
    pub tabs: Vec<PlaylistTab>,
    pub on_air: String,
}

/// Where copied items go.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum CopyTo {
    #[default]
    End,
    /// Right after the item on air (at the top of a list that is not on air).
    Next,
}

fn lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    mutex.lock().map_err(|e| e.to_string())
}

/// Run `edit` on a list: the one on air (`tab` absent, or the id of the tab on air), or
/// a tab's stored list, saved back afterwards. Returns its result and the list.
pub fn edit_list<R>(
    engine: &Mutex<AudioEngine>,
    library: &Mutex<MediaLibrary>,
    tab: Option<&str>,
    edit: impl FnOnce(&mut Playlist) -> R,
) -> Result<(R, Playlist), String> {
    {
        let mut engine = lock(engine)?;
        if tab.map_or(true, |id| id == engine.playlist().id) {
            let result = edit(engine.playlist_mut());
            return Ok((result, engine.playlist().clone()));
        }
    }
    let mut library = lock(library)?;
    let mut playlist = library.tab_playlist(tab.unwrap_or_default())?;
    let result = edit(&mut playlist);
    library.save_tab(&playlist)?;
    Ok((result, playlist))
}

/// The open tabs. The list on air always has one (created on first use); its item
/// count and length are the live list's, from the item on air on.
pub fn list(engine: &Mutex<AudioEngine>, library: &Mutex<MediaLibrary>) -> Result<Tabs, String> {
    let on_air = lock(engine)?.playlist().clone();
    let mut library = lock(library)?;
    let mut tabs = library.playlist_tabs()?;
    if !tabs.iter().any(|t| t.id == on_air.id) {
        library.save_tab(&on_air)?;
        tabs = library.playlist_tabs()?;
    }
    if let Some(tab) = tabs.iter_mut().find(|t| t.id == on_air.id) {
        let left = on_air.items.iter().skip(on_air.current_index);
        tab.items = left.clone().count();
        tab.duration_ms = left.map(|i| i.duration_ms).sum();
    }
    Ok(Tabs { tabs, on_air: on_air.id })
}

/// Open a new tab: empty, or a copy of the saved playlist `from_saved`.
pub fn open(library: &Mutex<MediaLibrary>, name: &str, from_saved: Option<&str>) -> Result<Playlist, String> {
    let mut library = lock(library)?;
    match from_saved {
        None => library.new_tab(name),
        Some(saved) => {
            let mut playlist = library.load_playlist(saved)?;
            playlist.id = Playlist::new(String::new()).id;
            library.save_tab(&playlist)?;
            Ok(playlist)
        }
    }
}

/// Close a tab; the one on air can't be closed.
pub fn close(engine: &Mutex<AudioEngine>, library: &Mutex<MediaLibrary>, id: &str) -> Result<Tabs, String> {
    if lock(engine)?.playlist().id == id {
        return Err("The list on air cannot be closed".to_string());
    }
    lock(library)?.close_tab(id)?;
    list(engine, library)
}

pub fn rename(engine: &Mutex<AudioEngine>, library: &Mutex<MediaLibrary>, id: &str, name: &str) -> Result<Tabs, String> {
    list(engine, library)?; // the tab on air is stored
    lock(library)?.rename_tab(id, name)?;
    {
        let mut engine = lock(engine)?;
        if engine.playlist().id == id {
            engine.playlist_mut().name = name.trim().chars().take(MAX_TAB_NAME).collect();
        }
    }
    list(engine, library)
}

/// Set or clear (empty or `None`) when a tab's list is planned to start, "HH:MM[:SS]".
pub fn set_start(engine: &Mutex<AudioEngine>, library: &Mutex<MediaLibrary>, id: &str, start: Option<&str>) -> Result<Tabs, String> {
    list(engine, library)?;
    lock(library)?.set_tab_start(id, start)?;
    list(engine, library)
}

pub fn order(engine: &Mutex<AudioEngine>, library: &Mutex<MediaLibrary>, ids: &[String]) -> Result<Tabs, String> {
    lock(library)?.order_tabs(ids)?;
    list(engine, library)
}

/// Put a tab's list on air: the track playing ends, then this list follows (see
/// [`AudioEngine::take_over`]). The list that was on air keeps what it had left to play,
/// in its own tab.
pub fn put_on_air(engine: &Mutex<AudioEngine>, library: &Mutex<MediaLibrary>, id: &str) -> Result<Tabs, String> {
    list(engine, library)?;
    let incoming = lock(library)?.tab_playlist(id)?;
    let leaving = {
        let mut engine = lock(engine)?;
        if engine.playlist().id == id {
            None
        } else {
            Some(engine.take_over(incoming))
        }
    };
    if let Some(leaving) = leaving {
        lock(library)?.save_tab(&leaving)?;
    }
    list(engine, library)
}

/// Copy (or move, with `remove`) the items `ids` of list `from` into list `to` (`None`
/// is the list on air). Copies get new ids, so the same item can sit in several lists.
/// Returns how many were copied.
pub fn copy_items(
    engine: &Mutex<AudioEngine>,
    library: &Mutex<MediaLibrary>,
    ids: &[String],
    from: Option<&str>,
    to: Option<&str>,
    position: CopyTo,
    remove: bool,
) -> Result<usize, String> {
    let (items, _) = edit_list(engine, library, from, |list| {
        list.items.iter().filter(|item| ids.contains(&item.id)).cloned().collect::<Vec<PlaylistItem>>()
    })?;
    if items.is_empty() {
        return Ok(0);
    }
    let copies: Vec<PlaylistItem> = items
        .into_iter()
        .map(|item| PlaylistItem { id: PlaylistItem::new(String::new(), String::new(), String::new(), 0).id, ..item })
        .collect();
    let count = copies.len();
    edit_list(engine, library, to, |list| {
        let at = match position {
            CopyTo::End => list.len(),
            CopyTo::Next => list.next_position(),
        };
        list.insert_items(at, copies);
    })?;
    if remove {
        edit_list(engine, library, from, |list| list.remove_ids(ids))?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Mutex<AudioEngine>, Mutex<MediaLibrary>) {
        let mut engine = AudioEngine::new_headless().unwrap();
        for i in 0..3 {
            engine.playlist_mut().add_item(format!("{i}.wav"), format!("t{i}"), String::new(), 1000);
        }
        (Mutex::new(engine), Mutex::new(MediaLibrary::open(":memory:").unwrap()))
    }

    fn titles(list: &Playlist) -> Vec<String> {
        list.items.iter().map(|i| i.title.clone()).collect()
    }

    #[test]
    fn tabs_edit_copy_and_go_on_air() {
        let (engine, library) = setup();
        let tabs = list(&engine, &library).unwrap();
        assert_eq!(tabs.tabs.len(), 1, "the list on air gets a tab");
        let main = tabs.on_air.clone();
        assert_eq!(tabs.tabs[0].items, 3);

        let programme = open(&library, "Programa", None).unwrap();
        let (_, list_b) = edit_list(&engine, &library, Some(&programme.id), |l| {
            l.add_item("p.wav".into(), "p".into(), String::new(), 500)
        })
        .unwrap();
        assert_eq!(titles(&list_b), ["p"]);
        assert_eq!(lock(&engine).unwrap().playlist().len(), 3, "editing a tab leaves the air alone");

        // Copy t1 from the air to the programme; ids differ.
        let t1 = lock(&engine).unwrap().playlist().items[1].id.clone();
        let n = copy_items(&engine, &library, &[t1.clone()], None, Some(&programme.id), CopyTo::End, false).unwrap();
        assert_eq!(n, 1);
        let stored = lock(&library).unwrap().tab_playlist(&programme.id).unwrap();
        assert_eq!(titles(&stored), ["p", "t1"]);
        assert_ne!(stored.items[1].id, t1);

        // Stopped: the programme goes on air, the main list keeps all three.
        let tabs = put_on_air(&engine, &library, &programme.id).unwrap();
        assert_eq!(tabs.on_air, programme.id);
        assert_eq!(titles(lock(&engine).unwrap().playlist()), ["p", "t1"]);
        assert_eq!(titles(&lock(&library).unwrap().tab_playlist(&main).unwrap()), ["t0", "t1", "t2"]);
        assert!(close(&engine, &library, &programme.id).is_err(), "the list on air stays open");

        rename(&engine, &library, &programme.id, "Tarde").unwrap();
        assert_eq!(lock(&engine).unwrap().playlist().name, "Tarde");
        close(&engine, &library, &main).unwrap();
        assert_eq!(list(&engine, &library).unwrap().tabs.len(), 1);
    }
}
