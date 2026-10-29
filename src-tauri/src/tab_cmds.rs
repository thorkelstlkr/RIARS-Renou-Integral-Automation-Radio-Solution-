//! Playlist tabs: several lists open for editing, one of them on air. The logic is in
//! `riars_core::playlist::tabs`; these commands hand it the app's engine and library.

use riars_core::playlist::playlist::Playlist;
use riars_core::playlist::tabs::{self, CopyTo, Tabs};
use tauri::State;

use crate::AppState;

/// Run `edit` on the list on air (`tab` absent or its id) or on a tab's stored list.
pub(crate) fn edit_list<R>(
    state: &AppState,
    tab: Option<&str>,
    edit: impl FnOnce(&mut Playlist) -> R,
) -> Result<(R, Playlist), String> {
    tabs::edit_list(&state.engine, &state.library, tab, edit)
}

#[tauri::command]
pub fn playlist_tabs(state: State<AppState>) -> Result<Tabs, String> {
    tabs::list(&state.engine, &state.library)
}

/// A tab's list (the live one for the tab on air).
#[tauri::command]
pub fn playlist_tab_get(id: String, state: State<AppState>) -> Result<Playlist, String> {
    Ok(edit_list(&state, Some(&id), |_| ())?.1)
}

/// Open a new tab: empty, or a copy of the saved playlist `from_saved`.
#[tauri::command]
pub fn playlist_tab_new(name: String, from_saved: Option<String>, state: State<AppState>) -> Result<Playlist, String> {
    tabs::open(&state.library, &name, from_saved.as_deref())
}

#[tauri::command]
pub fn playlist_tab_close(id: String, state: State<AppState>) -> Result<Tabs, String> {
    tabs::close(&state.engine, &state.library, &id)
}

#[tauri::command]
pub fn playlist_tab_rename(id: String, name: String, state: State<AppState>) -> Result<Tabs, String> {
    tabs::rename(&state.engine, &state.library, &id, &name)
}

/// Set or clear (empty `start`) when a tab's list is planned to start, "HH:MM[:SS]".
#[tauri::command]
pub fn playlist_tab_start(id: String, start: Option<String>, state: State<AppState>) -> Result<Tabs, String> {
    tabs::set_start(&state.engine, &state.library, &id, start.as_deref())
}

#[tauri::command]
pub fn playlist_tab_order(ids: Vec<String>, state: State<AppState>) -> Result<Tabs, String> {
    tabs::order(&state.engine, &state.library, &ids)
}

/// Put a tab's list on air after the track playing.
#[tauri::command]
pub fn playlist_tab_on_air(id: String, state: State<AppState>) -> Result<Tabs, String> {
    tabs::put_on_air(&state.engine, &state.library, &id)
}

/// Copy (or move, with `remove`) items between lists; `None` is the list on air.
#[tauri::command]
pub fn playlist_tab_copy(
    ids: Vec<String>,
    from: Option<String>,
    to: Option<String>,
    position: CopyTo,
    remove: bool,
    state: State<AppState>,
) -> Result<usize, String> {
    tabs::copy_items(&state.engine, &state.library, &ids, from.as_deref(), to.as_deref(), position, remove)
}
