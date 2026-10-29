//! Library folders (the folders the library is built from) and a plain file-system
//! browser for the desktop's folder tree.
//!
//! Scanning is split in two so a shared library is never locked while files are
//! read: [`collect_audio`] walks the disk and reads tags with no library at hand,
//! then [`MediaLibrary::insert_scanned`] stores the result in one transaction.

use std::collections::HashSet;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::db::{read_tags, MediaLibrary, Track};

/// Formats the symphonia features enabled in Cargo.toml can decode.
pub const AUDIO_EXTENSIONS: [&str; 6] = ["mp3", "flac", "wav", "ogg", "aac", "m4a"];

/// An audio file found on disk, with its tags, ready to be stored.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration_ms: u64,
}

/// A folder the library is built from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryFolder {
    pub path: String,
    /// Library tracks whose file is inside this folder.
    pub track_count: usize,
}

/// A subfolder in a [`DirListing`], or a drive or root in [`roots`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
}

/// An audio file in a [`DirListing`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    /// The library track for this file, when it is in the library.
    pub track: Option<Track>,
}

/// The subfolders and audio files of one folder, both sorted by name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirListing {
    pub path: String,
    pub parent: Option<String>,
    pub dirs: Vec<DirEntry>,
    pub files: Vec<FileEntry>,
}

pub(super) fn init_schema(db: &Connection) -> Result<(), String> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS library_folders (
            path     TEXT PRIMARY KEY,
            added_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| format!("DB init error: {e}"))
}

/// `path` as the library stores it: an existing directory, separators normalized.
///
/// Rebuilding from components turns "D:/Musica" and "D:\Musica\" into the same
/// string, so a folder added twice is one folder and a rescan never duplicates tracks.
pub fn normalize_folder(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Empty folder path".into());
    }
    let built: PathBuf = Path::new(trimmed).components().collect();
    if !built.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }
    Ok(built.to_string_lossy().into_owned())
}

/// The string every path inside `folder` starts with (the folder plus a separator).
fn prefix_of(folder: &str) -> String {
    if folder.ends_with(MAIN_SEPARATOR) || folder.ends_with('/') {
        folder.to_string()
    } else {
        format!("{folder}{MAIN_SEPARATOR}")
    }
}

/// Bounds of the paths inside `folder`: `lo <= path < hi`. A range on `file_path`
/// lets SQLite use the column's index, unlike `substr(file_path, ...) = prefix`.
fn prefix_range(folder: &str) -> (String, String) {
    let lo = prefix_of(folder);
    // The separator is the last character; the next code point bounds the range
    // (backslash -> ']', slash -> '0').
    let mut hi = lo.clone();
    let last = hi.pop().unwrap_or(MAIN_SEPARATOR);
    hi.push(char::from_u32(last as u32 + 1).unwrap_or(char::MAX));
    (lo, hi)
}

/// Whether `path` is `folder` or inside it.
fn is_within(path: &str, folder: &str) -> bool {
    path == folder || path.starts_with(&prefix_of(folder))
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Every playable audio file under `folder`, subfolders included, with its tags,
/// except those in `skip` (already in the library, see [`MediaLibrary::known_paths`]):
/// reading tags is the slow part on an SD card.
/// Touches no database, so it can run while others use the library.
pub fn collect_audio(folder: &str, skip: &HashSet<String>) -> Result<Vec<ScannedFile>, String> {
    let root = normalize_folder(folder)?;
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::from(root)];

    while let Some(dir) = pending.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                log::warn!("Cannot read directory {}: {e}", dir.display());
                continue;
            }
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            // file_type() does not follow links: linked folders are skipped, so a
            // link back up the tree cannot make the scan loop forever.
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if !hidden_name(&entry.file_name().to_string_lossy()) {
                    pending.push(path);
                }
                continue;
            }
            if kind.is_symlink() && path.is_dir() {
                continue;
            }
            if !is_audio(&path) {
                continue;
            }
            let path_str = path.to_string_lossy().into_owned();
            if skip.contains(&path_str) {
                continue;
            }
            files.push(scanned(&path, path_str));
        }
    }
    Ok(files)
}

/// One file with its tags; a file without a title tag is named after the file.
fn scanned(path: &Path, path_str: String) -> ScannedFile {
    let tags = read_tags(path);
    let title = tags.title.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string()
    });
    ScannedFile {
        path: path_str,
        title,
        artist: tags.artist.unwrap_or_default(),
        album: tags.album.unwrap_or_default(),
        genre: tags.genre.unwrap_or_default(),
        duration_ms: tags.duration_ms,
    }
}

/// One audio file read for the library, without looking at its folder: for a file
/// used before its folder was scanned (dropped on the playlist, chosen in the tree).
/// Touches no database.
pub fn read_audio_file(path: &str) -> Result<ScannedFile, String> {
    // Same separators as a folder scan, so the stored path matches a later rescan.
    let built: PathBuf = Path::new(path.trim()).components().collect();
    if !built.is_file() {
        return Err(format!("Not a file: {path}"));
    }
    if !is_audio(&built) {
        return Err(format!("Not a supported audio file: {path}"));
    }
    let path_str = built.to_string_lossy().into_owned();
    Ok(scanned(&built, path_str))
}

/// The library track of the audio file at `path`, adding the file first when it is
/// not in the library. Holds the lock only to look it up and to store it.
pub fn track_for_file(library: &Mutex<MediaLibrary>, path: &str) -> Result<Track, String> {
    if let Some(track) = library.lock().map_err(|e| e.to_string())?.track_by_path(path)? {
        return Ok(track);
    }
    let file = read_audio_file(path)?;
    let mut library = library.lock().map_err(|e| e.to_string())?;
    library.insert_scanned(std::slice::from_ref(&file))?;
    library
        .track_by_path(&file.path)?
        .ok_or_else(|| format!("Cannot add {path} to the library"))
}

/// The audio files of paths dropped on the playlist, in the order they will be added.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DroppedFiles {
    pub files: Vec<String>,
    /// Dropped paths that are not a folder or a supported audio file.
    pub skipped: usize,
    /// More than the limit were found; the rest were left out.
    pub truncated: bool,
}

/// The audio files of `paths`, as dropped from the system's file manager: a file is
/// kept when it is audio; a folder brings its audio files, subfolders included,
/// each folder's files first and then its subfolders, both by name. Hidden and
/// system entries inside a folder are left out, as in the folder tree. A file
/// dropped twice (on its own and inside its folder) comes once. At most `limit`.
pub fn expand_dropped(paths: &[String], limit: usize) -> DroppedFiles {
    let mut out = DroppedFiles::default();
    let mut seen = HashSet::new();
    let mut push = |out: &mut DroppedFiles, path: PathBuf| {
        if out.files.len() >= limit {
            out.truncated = true;
            return;
        }
        let path = path.to_string_lossy().into_owned();
        if seen.insert(path.clone()) {
            out.files.push(path);
        }
    };
    for dropped in paths {
        // Same separators as a folder scan, so the paths match the library's.
        let built: PathBuf = Path::new(dropped.trim()).components().collect();
        if built.is_dir() {
            // Depth first, keeping the order by name: subfolders go on the stack reversed.
            let mut pending = vec![built];
            while let Some(dir) = pending.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    log::warn!("Cannot read directory {}", dir.display());
                    continue;
                };
                let mut dirs = Vec::new();
                let mut files = Vec::new();
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let Ok(kind) = entry.file_type() else { continue };
                    if hidden_name(&name) || hidden_entry(&entry) {
                        continue;
                    }
                    // file_type() does not follow links: a link back up the tree is not walked.
                    if kind.is_dir() {
                        dirs.push((name.to_lowercase(), entry.path()));
                    } else if is_audio(&entry.path()) && entry.path().is_file() {
                        files.push((name.to_lowercase(), entry.path()));
                    }
                }
                files.sort();
                dirs.sort();
                for (_, file) in files {
                    push(&mut out, file);
                }
                pending.extend(dirs.into_iter().rev().map(|(_, dir)| dir));
            }
        } else if built.is_file() && is_audio(&built) {
            push(&mut out, built);
        } else {
            out.skipped += 1;
        }
    }
    out
}

/// Library tracks for `paths`, in the same order, adding the files not in the
/// library yet. Tags are read with the library unlocked and stored in batches, so
/// others keep using it during a big drop. `progress(done, total)` follows the
/// files being read. A file that cannot be added is left out; returns the tracks
/// and how many were left out.
pub fn tracks_for_files(
    library: &Mutex<MediaLibrary>,
    paths: &[String],
    progress: &mut dyn FnMut(usize, usize),
) -> Result<(Vec<Track>, usize), String> {
    const BATCH: usize = 200;
    let mut found: Vec<Option<Track>> = {
        let library = library.lock().map_err(|e| e.to_string())?;
        paths.iter().map(|path| library.track_by_path(path)).collect::<Result<_, _>>()?
    };
    let missing: Vec<usize> = (0..paths.len()).filter(|&i| found[i].is_none()).collect();
    let total = missing.len();
    progress(0, total);
    for (n, chunk) in missing.chunks(BATCH).enumerate() {
        let mut read = Vec::with_capacity(chunk.len());
        for (k, &i) in chunk.iter().enumerate() {
            match read_audio_file(&paths[i]) {
                Ok(file) => read.push((i, file)),
                Err(e) => log::warn!("Cannot add {}: {e}", paths[i]),
            }
            progress(n * BATCH + k + 1, total);
        }
        let mut library = library.lock().map_err(|e| e.to_string())?;
        let files: Vec<ScannedFile> = read.iter().map(|(_, file)| file.clone()).collect();
        library.insert_scanned(&files)?;
        for (i, file) in read {
            found[i] = library.track_by_path(&file.path)?;
        }
    }
    let left_out = found.iter().filter(|t| t.is_none()).count();
    Ok((found.into_iter().flatten().collect(), left_out))
}

/// Names the folder tree hides: system folders nobody keeps music in.
fn hidden_name(name: &str) -> bool {
    name.starts_with('.')
        || name.eq_ignore_ascii_case("$RECYCLE.BIN")
        || name.eq_ignore_ascii_case("System Volume Information")
        || name.eq_ignore_ascii_case("lost+found")
}

/// Whether Windows marks the entry hidden or system, as Explorer hides it
/// ($WINDOWS.~BT, "Documents and Settings"...). Never on other systems.
fn hidden_entry(entry: &std::fs::DirEntry) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        const SYSTEM: u32 = 0x4;
        // symlink_metadata: a junction's own attributes, not its target's.
        if let Ok(meta) = std::fs::symlink_metadata(entry.path()) {
            return meta.file_attributes() & (HIDDEN | SYSTEM) != 0;
        }
    }
    let _ = entry;
    false
}

/// Where the folder tree starts: the drives on Windows; the root, the home folder
/// and mounted media elsewhere.
pub fn roots() -> Vec<DirEntry> {
    let mut out = Vec::new();
    #[cfg(windows)]
    for letter in b'A'..=b'Z' {
        let path = format!("{}:\\", letter as char);
        if Path::new(&path).is_dir() {
            out.push(DirEntry { name: format!("{}:", letter as char), path });
        }
    }
    #[cfg(not(windows))]
    {
        out.push(DirEntry { name: "/".into(), path: "/".into() });
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            if home.is_dir() {
                out.push(DirEntry { name: "~".into(), path: home.to_string_lossy().into_owned() });
            }
        }
        for media in ["/media", "/mnt", "/Volumes"] {
            if let Ok(entries) = std::fs::read_dir(media) {
                let mut found: Vec<DirEntry> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .map(|p| DirEntry {
                        name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                        path: p.to_string_lossy().into_owned(),
                    })
                    .collect();
                found.sort_by_key(|d| d.name.to_lowercase());
                out.extend(found);
            }
        }
    }
    out
}

/// The subfolders and audio files of `path`, without looking deeper.
/// `FileEntry::track` is left empty; [`MediaLibrary::fill_tracks`] fills it.
pub fn list_dir(path: &str) -> Result<DirListing, String> {
    let dir = normalize_folder(path)?;
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Cannot read {dir}: {e}"))?;
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if hidden_name(&name) || hidden_entry(&entry) {
            continue;
        }
        let full = entry.path();
        if full.is_dir() {
            dirs.push(DirEntry { name, path: full.to_string_lossy().into_owned() });
        } else if is_audio(&full) {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            files.push(FileEntry { name, path: full.to_string_lossy().into_owned(), size, track: None });
        }
    }
    dirs.sort_by_key(|d| d.name.to_lowercase());
    files.sort_by_key(|f| f.name.to_lowercase());
    let parent = Path::new(&dir).parent().map(|p| p.to_string_lossy().into_owned());
    Ok(DirListing { path: dir, parent, dirs, files })
}

impl MediaLibrary {
    /// The library folders, each with how many library tracks it holds.
    pub fn folders(&self) -> Result<Vec<LibraryFolder>, String> {
        let paths = self.folder_paths()?;
        let mut count = self
            .conn()
            .prepare_cached("SELECT COUNT(*) FROM tracks WHERE file_path >= ?1 AND file_path < ?2")
            .map_err(|e| format!("Query error: {e}"))?;
        paths
            .into_iter()
            .map(|path| {
                let (lo, hi) = prefix_range(&path);
                let n: i64 = count
                    .query_row(params![lo, hi], |r| r.get(0))
                    .map_err(|e| format!("Query error: {e}"))?;
                Ok(LibraryFolder { path, track_count: n as usize })
            })
            .collect()
    }

    /// The library folder paths, without counting their tracks.
    pub fn folder_paths(&self) -> Result<Vec<String>, String> {
        self.conn()
            .prepare("SELECT path FROM library_folders ORDER BY path")
            .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
            .map_err(|e| format!("Query error: {e}"))
    }

    /// Whether `path` is a library folder or inside one.
    pub fn is_in_folders(&self, path: &str) -> Result<bool, String> {
        Ok(self.folder_paths()?.iter().any(|f| is_within(path, f)))
    }

    /// Remember `path` as a library folder; returns it as stored. Adding a folder
    /// twice is not an error. Scanning it is the caller's job.
    pub fn add_folder(&mut self, path: &str) -> Result<String, String> {
        let path = normalize_folder(path)?;
        self.conn()
            .execute("INSERT OR IGNORE INTO library_folders (path) VALUES (?1)", params![path])
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(path)
    }

    /// Forget a library folder. With `remove_tracks`, also take out of the library
    /// the tracks inside it that no other library folder holds; returns how many.
    /// All in one transaction: either everything is removed or nothing is.
    pub fn remove_folder(&mut self, path: &str, remove_tracks: bool) -> Result<usize, String> {
        // The folder may be gone from disk, so it is matched as stored, not normalized.
        let all = self.folder_paths()?;
        let Some(stored) = all.iter().find(|p| *p == path || Path::new(p.as_str()) == Path::new(path)).cloned() else {
            return Err(format!("Not a library folder: {path}"));
        };
        let others: Vec<&String> = all.iter().filter(|p| **p != stored).collect();
        let (lo, hi) = prefix_range(&stored);
        let tx = self.conn_mut().transaction().map_err(|e| format!("DB error: {e}"))?;
        tx.execute("DELETE FROM library_folders WHERE path = ?1", params![stored])
            .map_err(|e| format!("DB error: {e}"))?;
        let mut removed = 0;
        if remove_tracks {
            let doomed: Vec<(i64, String)> = tx
                .prepare("SELECT id, file_path FROM tracks WHERE file_path >= ?1 AND file_path < ?2")
                .and_then(|mut stmt| stmt.query_map(params![lo, hi], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
                .map_err(|e| format!("Query error: {e}"))?;
            let mut delete = tx
                .prepare("DELETE FROM tracks WHERE id = ?1")
                .map_err(|e| format!("Delete error: {e}"))?;
            for (id, file) in doomed {
                if !others.iter().any(|o| is_within(&file, o)) {
                    removed += delete.execute(params![id]).map_err(|e| format!("Delete error: {e}"))?;
                }
            }
            drop(delete);
        }
        tx.commit().map_err(|e| format!("DB error: {e}"))?;
        if removed > 0 {
            self.bump_revision();
        }
        Ok(removed)
    }

    /// The library folders worth scanning: those not inside another library folder,
    /// which a scan of the outer one already covers.
    pub fn folders_to_scan(&self) -> Result<Vec<String>, String> {
        let all = self.folder_paths()?;
        Ok(all
            .iter()
            .filter(|p| !all.iter().any(|o| o != *p && is_within(p, o)))
            .cloned()
            .collect())
    }

    /// The files under `folder` already in the library with their tags read, which a
    /// rescan can skip. Tracks stored without a duration are left out so their tags
    /// are read again.
    pub fn known_paths(&self, folder: &str) -> Result<HashSet<String>, String> {
        let (lo, hi) = prefix_range(folder);
        self.conn()
            .prepare("SELECT file_path FROM tracks WHERE file_path >= ?1 AND file_path < ?2 AND duration_ms > 0")
            .and_then(|mut stmt| stmt.query_map(params![lo, hi], |r| r.get(0))?.collect())
            .map_err(|e| format!("Query error: {e}"))
    }

    /// Fill `FileEntry::track` for the files of `listing` that are in the library.
    /// One query per 500 files, not one per file.
    pub fn fill_tracks(&self, listing: &mut DirListing) -> Result<(), String> {
        let mut found = std::collections::HashMap::new();
        for chunk in listing.files.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT id, title, artist, album, genre, duration_ms, file_path, bpm, sample_rate
                 FROM tracks WHERE file_path IN ({marks})"
            );
            let mut stmt = self.conn().prepare(&sql).map_err(|e| format!("Query error: {e}"))?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(chunk.iter().map(|f| f.path.as_str())), |row| {
                    Ok(Track {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        artist: row.get(2)?,
                        album: row.get(3)?,
                        genre: row.get(4)?,
                        duration_ms: row.get(5)?,
                        file_path: row.get(6)?,
                        bpm: row.get(7)?,
                        sample_rate: row.get(8)?,
                    })
                })
                .map_err(|e| format!("Query error: {e}"))?;
            for track in rows {
                let track = track.map_err(|e| format!("Query error: {e}"))?;
                found.insert(track.file_path.clone(), track);
            }
        }
        for file in &mut listing.files {
            file.track = found.remove(&file.path);
        }
        Ok(())
    }
}

/// Scan one folder into a shared library, holding the lock only to store the result.
pub fn scan_into(library: &Mutex<MediaLibrary>, folder: &str) -> Result<usize, String> {
    let folder = normalize_folder(folder)?;
    let known = library.lock().map_err(|e| e.to_string())?.known_paths(&folder)?;
    let files = collect_audio(&folder, &known)?;
    let mut library = library.lock().map_err(|e| e.to_string())?;
    // Removed while the disk was being read: storing now would bring its tracks back.
    if !library.is_in_folders(&folder)? {
        log::info!("{folder} left the library folders during its scan; nothing stored");
        return Ok(0);
    }
    let added = library.insert_scanned(&files)?;
    drop(library);
    log::info!("Scanned {folder}: {} files read, {added} new tracks", files.len());
    Ok(added)
}

/// Rescan every library folder into a shared library (see [`scan_into`]).
/// A folder that fails is logged and skipped; returns the new tracks in total.
pub fn rescan_all(library: &Mutex<MediaLibrary>) -> Result<usize, String> {
    let folders = library.lock().map_err(|e| e.to_string())?.folders_to_scan()?;
    let mut added = 0;
    for folder in folders {
        match scan_into(library, &folder) {
            Ok(n) => added += n,
            Err(e) => log::warn!("Cannot rescan {folder}: {e}"),
        }
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("riars-folders-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"not really audio").unwrap();
    }

    fn library(dir: &Path) -> MediaLibrary {
        MediaLibrary::open(&dir.join("lib.db").to_string_lossy()).unwrap()
    }

    #[test]
    fn lists_folders_and_audio_files_sorted_hiding_system_entries() {
        let dir = temp_dir("list");
        touch(&dir.join("b.mp3"));
        touch(&dir.join("A.FLAC"));
        touch(&dir.join("notes.txt"));
        std::fs::create_dir_all(dir.join("zeta")).unwrap();
        std::fs::create_dir_all(dir.join("Alpha")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let listing = list_dir(&dir.to_string_lossy()).unwrap();
        let dirs: Vec<_> = listing.dirs.iter().map(|d| d.name.as_str()).collect();
        let files: Vec<_> = listing.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(dirs, ["Alpha", "zeta"]);
        assert_eq!(files, ["A.FLAC", "b.mp3"]);
        assert!(listing.parent.is_some());
        assert!(list_dir(&dir.join("missing").to_string_lossy()).is_err());
    }

    #[test]
    fn folders_are_normalized_counted_and_deduplicated() {
        let dir = temp_dir("add");
        let music = dir.join("music");
        touch(&music.join("one.mp3"));
        touch(&music.join("sub").join("two.wav"));
        let mut lib = library(&dir);
        let stored = lib.add_folder(&format!("{}{}", music.to_string_lossy(), MAIN_SEPARATOR)).unwrap();
        assert_eq!(stored, music.to_string_lossy());
        lib.add_folder(&music.to_string_lossy().replace('\\', "/")).unwrap();
        assert_eq!(lib.folders().unwrap().len(), 1);
        assert_eq!(lib.scan_folder(&stored).unwrap(), 2);
        assert_eq!(lib.folders().unwrap()[0].track_count, 2);
        assert!(lib.add_folder(&dir.join("nope").to_string_lossy()).is_err());
    }

    #[test]
    fn nested_folders_are_scanned_once_and_keep_their_tracks() {
        let dir = temp_dir("nested");
        let music = dir.join("music");
        let jingles = music.join("jingles");
        // A sibling whose name starts like the folder must not count as inside it.
        let music2 = dir.join("music2");
        touch(&jingles.join("j.mp3"));
        touch(&music.join("song.mp3"));
        touch(&music2.join("other.mp3"));
        let lib = Mutex::new(library(&dir));
        {
            let mut l = lib.lock().unwrap();
            l.add_folder(&jingles.to_string_lossy()).unwrap();
            l.add_folder(&music.to_string_lossy()).unwrap();
            l.add_folder(&music2.to_string_lossy()).unwrap();
            let scan = l.folders_to_scan().unwrap();
            assert_eq!(scan, [music.to_string_lossy().into_owned(), music2.to_string_lossy().into_owned()]);
        }
        assert_eq!(rescan_all(&lib).unwrap(), 3);
        assert_eq!(rescan_all(&lib).unwrap(), 0);
        let mut l = lib.lock().unwrap();
        // Dropping the outer folder keeps the jingles, which another folder still holds.
        assert_eq!(l.remove_folder(&music.to_string_lossy(), true).unwrap(), 1);
        assert_eq!(l.track_count().unwrap(), 2);
        assert_eq!(l.remove_folder(&music2.to_string_lossy(), false).unwrap(), 0);
        assert_eq!(l.track_count().unwrap(), 2);
        assert!(l.remove_folder(&music.to_string_lossy(), false).is_err());
    }

    #[test]
    fn scans_skip_system_folders_and_unregistered_folders_store_nothing() {
        let dir = temp_dir("skip");
        let music = dir.join("music");
        touch(&music.join("song.mp3"));
        touch(&music.join("$RECYCLE.BIN").join("deleted.mp3"));
        touch(&music.join(".cache").join("hidden.mp3"));
        let lib = Mutex::new(library(&dir));
        // Not (or no longer) a library folder: a scan that finishes stores nothing.
        assert_eq!(scan_into(&lib, &music.to_string_lossy()).unwrap(), 0);
        lib.lock().unwrap().add_folder(&music.to_string_lossy()).unwrap();
        assert_eq!(scan_into(&lib, &music.to_string_lossy()).unwrap(), 1);
    }

    #[test]
    fn prefix_ranges_bound_exactly_the_folder() {
        let s = MAIN_SEPARATOR;
        let (lo, hi) = prefix_range(&format!("{s}music"));
        let inside = format!("{s}music{s}a.mp3");
        let sibling = format!("{s}music2{s}a.mp3");
        assert_eq!(lo, format!("{s}music{s}"));
        assert!(inside >= lo && inside < hi);
        assert!(!(sibling >= lo && sibling < hi));
        let (lo, hi) = prefix_range("/");
        assert_eq!((lo.as_str(), hi.as_str()), ("/", "0"));
        // A backslash separates folders only on Windows.
        #[cfg(windows)]
        {
            let (lo, hi) = prefix_range("D:\\");
            assert_eq!((lo.as_str(), hi.as_str()), ("D:\\", "D:]"));
        }
    }

    #[test]
    fn a_single_file_joins_the_library_once() {
        let dir = temp_dir("single");
        let file = dir.join("loose").join("jingle.mp3");
        touch(&file);
        touch(&dir.join("loose").join("notes.txt"));
        let lib = Mutex::new(library(&dir));
        let track = track_for_file(&lib, &file.to_string_lossy()).unwrap();
        assert_eq!(track.title, "jingle");
        assert_eq!(track_for_file(&lib, &file.to_string_lossy()).unwrap().id, track.id);
        assert_eq!(lib.lock().unwrap().track_count().unwrap(), 1);
        assert!(track_for_file(&lib, &dir.join("loose").join("notes.txt").to_string_lossy()).is_err());
        assert!(track_for_file(&lib, &dir.join("missing.mp3").to_string_lossy()).is_err());
    }

    #[test]
    fn fills_library_tracks_into_a_listing() {
        let dir = temp_dir("fill");
        let music = dir.join("music");
        touch(&music.join("in.mp3"));
        let mut lib = library(&dir);
        lib.scan_folder(&music.to_string_lossy()).unwrap();
        touch(&music.join("new.mp3"));
        let mut listing = list_dir(&music.to_string_lossy()).unwrap();
        lib.fill_tracks(&mut listing).unwrap();
        let tracked: Vec<_> = listing.files.iter().map(|f| (f.name.as_str(), f.track.is_some())).collect();
        assert_eq!(tracked, [("in.mp3", true), ("new.mp3", false)]);
    }

    #[test]
    fn a_drop_expands_folders_by_name_and_skips_what_is_not_audio() {
        let dir = temp_dir("drop");
        let show = dir.join("Show");
        touch(&show.join("b.mp3"));
        touch(&show.join("A.flac"));
        touch(&show.join("cover.jpg"));
        touch(&show.join("Part 2").join("z.wav"));
        touch(&show.join("part 1").join("y.ogg"));
        touch(&show.join(".cache").join("hidden.mp3"));
        touch(&dir.join("jingle.mp3"));
        touch(&dir.join("notes.txt"));
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let dropped = [s(&dir.join("jingle.mp3")), s(&show), s(&dir.join("notes.txt")),
            s(&show.join("b.mp3")), s(&dir.join("missing.mp3"))];
        let out = expand_dropped(&dropped, 100);
        let names: Vec<_> = out.files.iter()
            .map(|f| Path::new(f).strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(names, ["jingle.mp3", "Show/A.flac", "Show/b.mp3", "Show/part 1/y.ogg", "Show/Part 2/z.wav"]);
        assert_eq!((out.skipped, out.truncated), (2, false));

        let out = expand_dropped(&[s(&show)], 2);
        assert_eq!((out.files.len(), out.truncated), (2, true));
    }

    #[test]
    fn dropped_files_join_the_library_in_order() {
        let dir = temp_dir("drop-lib");
        let music = dir.join("music");
        touch(&music.join("known.mp3"));
        touch(&music.join("new.mp3"));
        let lib = Mutex::new(library(&dir));
        let known = track_for_file(&lib, &music.join("known.mp3").to_string_lossy()).unwrap();
        let paths: Vec<String> = ["new.mp3", "known.mp3", "gone.mp3"]
            .iter().map(|n| music.join(n).to_string_lossy().into_owned()).collect();
        let mut seen = Vec::new();
        let (tracks, left_out) = tracks_for_files(&lib, &paths, &mut |done, total| seen.push((done, total))).unwrap();
        let titles: Vec<_> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["new", "known"]);
        assert_eq!(tracks[1].id, known.id);
        assert_eq!(left_out, 1);
        assert_eq!(seen, [(0, 2), (1, 2), (2, 2)]);
        assert_eq!(lib.lock().unwrap().track_count().unwrap(), 2);
    }
}
