//! Completing the metadata a library's tracks lack (title, artist, album, year, genre
//! and cover) from open sources: MusicBrainz, AcoustID and the Cover Art Archive.
//!
//! The network lives here, not in `riars-core`. Nothing is changed by a lookup: it
//! produces proposals, and only the changes the operator accepts are applied, recording
//! where each value came from.

mod catalog;
mod http;
mod matching;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use riars_core::analysis::LibraryAccess;
use riars_core::library::metadata::{fingerprint, write_tags};
use riars_core::{MediaLibrary, MetadataChange, MetadataFields, TrackMetadata};
use serde::{Deserialize, Serialize};

pub use catalog::{Catalog, Online};
pub use http::USER_AGENT;
pub use matching::{identify, Found, Method};

/// Most tracks one lookup takes (about an hour and a half at MusicBrainz's pace).
pub const MAX_TRACKS: usize = 5000;
/// Tracks shorter than this are left out of "every track with gaps": jingles and promos.
pub const MIN_SONG_MS: u64 = 60_000;
/// Library setting holding the AcoustID application key.
const ACOUSTID_KEY: &str = "metadata.acoustid_key";

/// What the lookup found for one track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub track_id: i64,
    pub current: TrackMetadata,
    /// `None`: nothing in the catalogue fits well enough.
    pub found: Option<Found>,
    pub error: Option<String>,
    /// Some of it was applied since.
    #[serde(default)]
    pub applied: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LookupStatus {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    /// Track being looked up now.
    pub current: Option<String>,
    pub results: Vec<Proposal>,
    /// The lookup stopped early (cancelled).
    pub stopped: Option<String>,
    /// Whether the fingerprint step was available (an AcoustID key is set).
    pub fingerprints: bool,
}

/// One lookup at a time per process, so MusicBrainz's limit holds.
#[derive(Default)]
pub struct LookupJob {
    status: Arc<Mutex<LookupStatus>>,
    cancel: Arc<AtomicBool>,
}

/// The process's lookup.
pub fn job() -> &'static LookupJob {
    static JOB: OnceLock<LookupJob> = OnceLock::new();
    JOB.get_or_init(LookupJob::default)
}

impl LookupJob {
    pub fn status(&self) -> LookupStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Stop after the track being looked up now.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Look up `track_ids` (or, when empty, every track missing something that is at
    /// least [`MIN_SONG_MS`] long) on a background thread; returns how many. Fails if a
    /// lookup is running.
    pub fn start(&self, library: Arc<dyn LibraryAccess>, track_ids: Vec<i64>) -> Result<usize, String> {
        self.start_with(library, track_ids, Arc::new(Online))
    }

    pub fn start_with(&self, library: Arc<dyn LibraryAccess>, track_ids: Vec<i64>, catalog: Arc<dyn Catalog>) -> Result<usize, String> {
        if self.status().running {
            return Err("A metadata lookup is already running".to_string());
        }
        let mut tracks = Vec::new();
        let mut key = None;
        library.with_library(&mut |lib| {
            let ids = if track_ids.is_empty() { lib.tracks_missing_metadata(MIN_SONG_MS, MAX_TRACKS)? } else { track_ids.clone() };
            tracks.clear();
            for id in ids.into_iter().take(MAX_TRACKS) {
                if let Some(track) = lib.track_metadata(id)? {
                    tracks.push(track);
                }
            }
            key = acoustid_key(lib)?;
            Ok(())
        })?;
        if tracks.is_empty() {
            return Err("No tracks to look up".to_string());
        }
        let total = tracks.len();
        {
            let mut status = self.status.lock().map_err(|e| e.to_string())?;
            if status.running {
                return Err("A metadata lookup is already running".to_string());
            }
            *status = LookupStatus { running: true, total, fingerprints: key.is_some(), ..Default::default() };
        }
        self.cancel.store(false, Ordering::Relaxed);
        let status = Arc::clone(&self.status);
        let cancel = Arc::clone(&self.cancel);
        let spawned = std::thread::Builder::new().name("riars-metadata".into()).spawn(move || {
            for track in tracks {
                if cancel.load(Ordering::Relaxed) {
                    set(&status, |s| s.stopped = Some("cancelled".into()));
                    break;
                }
                set(&status, |s| s.current = Some(track.file_path.clone()));
                let path = PathBuf::from(&track.file_path);
                let result = identify(catalog.as_ref(), &track, key.as_deref(), &|| fingerprint(&path));
                let proposal = match result {
                    Ok(found) => Proposal { track_id: track.id, current: track, found, error: None, applied: false },
                    Err(error) => {
                        log::warn!("Metadata lookup of {}: {error}", track.file_path);
                        Proposal { track_id: track.id, current: track, found: None, error: Some(error), applied: false }
                    }
                };
                set(&status, |s| {
                    s.results.push(proposal);
                    s.done += 1;
                });
            }
            set(&status, |s| {
                s.running = false;
                s.current = None;
            });
        });
        if let Err(e) = spawned {
            set(&self.status, |s| s.running = false);
            return Err(format!("Cannot start the lookup: {e}"));
        }
        Ok(total)
    }

    fn mark_applied(&self, track_id: i64, now: &TrackMetadata) {
        set(&self.status, |s| {
            if let Some(p) = s.results.iter_mut().find(|p| p.track_id == track_id) {
                p.applied = true;
                p.current = now.clone();
            }
        });
    }
}

fn set(status: &Mutex<LookupStatus>, f: impl FnOnce(&mut LookupStatus)) {
    if let Ok(mut s) = status.lock() {
        f(&mut s);
    }
}

/// The AcoustID key saved in the library settings, if any.
pub fn acoustid_key(lib: &MediaLibrary) -> Result<Option<String>, String> {
    Ok(lib.kv_get(ACOUSTID_KEY)?.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()))
}

/// Save (or, when blank, clear) the AcoustID key.
pub fn set_acoustid_key(lib: &mut MediaLibrary, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.len() > 100 || key.chars().any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_') {
        return Err("An AcoustID key is letters and digits only".to_string());
    }
    lib.kv_set(ACOUSTID_KEY, key)
}

/// One accepted change, as the operator ticked it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApplyItem {
    pub track_id: i64,
    #[serde(default)]
    pub fields: MetadataFields,
    /// Cover to download and keep (only Cover Art Archive addresses are accepted).
    #[serde(default)]
    pub cover_url: Option<String>,
    /// Origin to record, e.g. `musicbrainz:<recording id>`; "manual" when empty.
    #[serde(default)]
    pub source: String,
    /// Also write the values into the audio file's tags.
    #[serde(default)]
    pub write_file: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApplyOutcome {
    pub track_id: i64,
    pub applied: bool,
    pub cover_saved: bool,
    pub wrote_file: bool,
    /// What went wrong, if anything; the other parts may still have been done.
    pub error: Option<String>,
    pub track: Option<TrackMetadata>,
}

/// Where the covers are kept: `riars_covers` next to the library database.
pub fn covers_dir(library_dir: &Path) -> PathBuf {
    library_dir.join("riars_covers")
}

/// Apply the accepted changes. Covers are downloaded and files written outside the
/// library lock; the lock is taken once per track only to store the values.
pub fn apply(library: &dyn LibraryAccess, covers: &Path, items: Vec<ApplyItem>) -> Vec<ApplyOutcome> {
    items.into_iter().take(MAX_TRACKS).map(|item| apply_one(library, covers, item)).collect()
}

fn apply_one(library: &dyn LibraryAccess, covers: &Path, item: ApplyItem) -> ApplyOutcome {
    let mut outcome = ApplyOutcome { track_id: item.track_id, ..Default::default() };
    let mut errors = Vec::new();
    let mut cover: Option<(PathBuf, Vec<u8>)> = None;
    if let Some(url) = item.cover_url.as_deref().filter(|u| !u.is_empty()) {
        match download_cover(covers, url) {
            Ok(saved) => cover = Some(saved),
            Err(e) => errors.push(e),
        }
    }
    let source = match item.source.trim() {
        "" => "manual".to_string(),
        s => s.chars().take(200).collect(),
    };
    let change = MetadataChange {
        track_id: item.track_id,
        fields: item.fields.clone(),
        cover_path: cover.as_ref().map(|(p, _)| p.to_string_lossy().into_owned()),
        source,
    };
    let mut stored = None;
    let result = library.with_library(&mut |lib| {
        stored = Some(lib.apply_metadata(&change)?);
        Ok(())
    });
    match (result, stored) {
        (Ok(()), Some(track)) => {
            outcome.applied = true;
            outcome.cover_saved = cover.is_some();
            job().mark_applied(item.track_id, &track);
            if item.write_file {
                match write_tags(Path::new(&track.file_path), &item.fields, cover.as_ref().map(|(_, b)| b.as_slice())) {
                    Ok(()) => outcome.wrote_file = true,
                    Err(e) => errors.push(e),
                }
            }
            outcome.track = Some(track);
        }
        (Err(e), _) => errors.push(e),
        (Ok(()), None) => errors.push("Nothing stored".into()),
    }
    if !errors.is_empty() {
        outcome.error = Some(errors.join("; "));
    }
    outcome
}

/// Download a cover into `dir` (kept by release group, so an album's tracks share it).
fn download_cover(dir: &Path, url: &str) -> Result<(PathBuf, Vec<u8>), String> {
    let rest = url
        .strip_prefix(catalog::COVER_ART)
        .filter(|r| r.starts_with('/'))
        .ok_or_else(|| "Covers come only from the Cover Art Archive".to_string())?;
    // "/release-group/<id>/front-500" -> "<id>"
    let id: String = rest.split('/').nth(2).unwrap_or("").chars().filter(|c| c.is_ascii_hexdigit() || *c == '-').collect();
    if id.len() < 8 {
        return Err("Not a cover address".to_string());
    }
    for ext in ["jpg", "png"] {
        let existing = dir.join(format!("{id}.{ext}"));
        if let Ok(bytes) = std::fs::read(&existing) {
            return Ok((existing, bytes));
        }
    }
    let (bytes, kind) = match http::get_image(&http::COVER_ART, url) {
        Ok(found) => found,
        Err(http::HttpError::NotFound) => return Err("No cover in the Cover Art Archive".to_string()),
        Err(e) => return Err(format!("Cover: {e}")),
    };
    let ext = if kind.contains("png") || bytes.starts_with(b"\x89PNG") {
        "png"
    } else if kind.contains("jpeg") || bytes.starts_with(&[0xFF, 0xD8]) {
        "jpg"
    } else {
        return Err("The cover is not a JPEG or PNG image".to_string());
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let path = dir.join(format!("{id}.{ext}"));
    std::fs::write(&path, &bytes).map_err(|e| format!("Cannot save the cover: {e}"))?;
    Ok((path, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AcoustMatch, ArtistRef, Credit, Genre, Recording};

    fn library() -> Arc<Mutex<MediaLibrary>> {
        Arc::new(Mutex::new(MediaLibrary::open(":memory:").unwrap()))
    }

    fn add(lib: &Arc<Mutex<MediaLibrary>>, path: &str, title: &str, artist: &str, duration_ms: u64) -> i64 {
        let files = vec![riars_core::library::folders::ScannedFile {
            path: path.into(),
            title: title.into(),
            artist: artist.into(),
            album: String::new(),
            genre: String::new(),
            duration_ms,
        }];
        let mut lib = lib.lock().unwrap();
        lib.insert_scanned(&files).unwrap();
        lib.track_by_path(path).unwrap().unwrap().id
    }

    /// Knows one song; a search for "Boom" fails as if MusicBrainz were down.
    struct OneSong;
    impl Catalog for OneSong {
        fn search(&self, title: &str, _: Option<&str>) -> Result<Vec<Recording>, String> {
            if title == "Boom" {
                return Err("MusicBrainz: Service unavailable".into());
            }
            Ok(vec![Recording {
                id: "r1".into(),
                title: "Roma".into(),
                length: Some(230_000),
                artist_credit: vec![Credit { name: "Amulet".into(), joinphrase: String::new(), artist: ArtistRef::default() }],
                ..Default::default()
            }])
        }
        fn recording(&self, _: &str) -> Result<Recording, String> {
            let mut r = self.search("Roma", None)?.remove(0);
            r.first_release_date = Some("2017".into());
            Ok(r)
        }
        fn release_group_genres(&self, _: &str) -> Result<Vec<Genre>, String> {
            Ok(vec![])
        }
        fn artist_genres(&self, _: &str) -> Result<Vec<Genre>, String> {
            Ok(vec![])
        }
        fn acoustid(&self, _: &str, _: &str, _: u32) -> Result<Vec<AcoustMatch>, String> {
            Ok(vec![])
        }
    }

    fn wait(job: &LookupJob) -> LookupStatus {
        for _ in 0..500 {
            let status = job.status();
            if !status.running {
                return status;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the lookup did not finish");
    }

    #[test]
    fn a_lookup_proposes_and_apply_stores_only_what_was_ticked() {
        let lib = library();
        let roma = add(&lib, "/m/07 Roma.mp3", "Roma", "Amulet", 230_164);
        let boom = add(&lib, "/m/boom.mp3", "Boom", "X", 200_000);
        let jingle = add(&lib, "/m/jingle.wav", "jingle", "", 9_000);
        let job = LookupJob::default();
        let access: Arc<dyn LibraryAccess> = Arc::new(lib.clone());
        assert_eq!(job.start_with(Arc::clone(&access), vec![], Arc::new(OneSong)).unwrap(), 2, "the jingle is left out");
        let status = wait(&job);
        assert_eq!(status.done, 2);
        assert!(!status.fingerprints);
        let found = status.results.iter().find(|p| p.track_id == roma).unwrap().found.clone().unwrap();
        assert_eq!(found.fields.year, Some(2017));
        assert_eq!(found.suggested, vec!["year"]);
        let failed = status.results.iter().find(|p| p.track_id == boom).unwrap();
        assert!(failed.found.is_none());
        assert!(failed.error.as_deref().unwrap().contains("unavailable"));
        assert!(!status.results.iter().any(|p| p.track_id == jingle));
        assert!(job.start_with(Arc::clone(&access), vec![999], Arc::new(OneSong)).is_err(), "no such track");

        // Nothing changed until applied; then only the ticked field.
        assert_eq!(lib.lock().unwrap().track_metadata(roma).unwrap().unwrap().year, None);
        let covers = std::env::temp_dir().join(format!("riars-meta-covers-{}", std::process::id()));
        let outcomes = apply(
            access.as_ref(),
            &covers,
            vec![ApplyItem {
                track_id: roma,
                fields: MetadataFields { year: Some(2017), ..Default::default() },
                source: found.source.clone(),
                ..Default::default()
            }],
        );
        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].applied, "{:?}", outcomes[0].error);
        assert!(!outcomes[0].wrote_file);
        let now = lib.lock().unwrap().track_metadata(roma).unwrap().unwrap();
        assert_eq!(now.year, Some(2017));
        assert_eq!(now.album, "");
        assert_eq!(now.source, "musicbrainz:r1");

        // A cover from anywhere else is refused, and the rest is still applied.
        let outcomes = apply(
            access.as_ref(),
            &covers,
            vec![ApplyItem {
                track_id: roma,
                fields: MetadataFields { genre: Some("Folk".into()), ..Default::default() },
                cover_url: Some("http://127.0.0.1/evil.jpg".into()),
                ..Default::default()
            }],
        );
        assert!(outcomes[0].applied);
        assert!(!outcomes[0].cover_saved);
        assert!(outcomes[0].error.as_deref().unwrap().contains("Cover Art Archive"));
        let now = lib.lock().unwrap().track_metadata(roma).unwrap().unwrap();
        assert_eq!(now.genre, "Folk");
        assert_eq!(now.source, "manual");
        assert!(!apply(access.as_ref(), &covers, vec![ApplyItem { track_id: 999, ..Default::default() }])[0].applied);
    }

    #[test]
    fn the_acoustid_key_is_kept_in_the_library() {
        let lib = library();
        let mut lib = lib.lock().unwrap();
        assert_eq!(acoustid_key(&lib).unwrap(), None);
        set_acoustid_key(&mut lib, " abcDEF123 ").unwrap();
        assert_eq!(acoustid_key(&lib).unwrap().as_deref(), Some("abcDEF123"));
        assert!(set_acoustid_key(&mut lib, "bad key; drop").is_err());
        set_acoustid_key(&mut lib, "").unwrap();
        assert_eq!(acoustid_key(&lib).unwrap(), None);
    }

    /// Real MusicBrainz, to check the queries against the live service:
    /// `cargo test -p riars-meta -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_musicbrainz() {
        let t = TrackMetadata {
            id: 1,
            title: "De Cacería".into(),
            artist: "Def Con Dos".into(),
            album: "Alzheimer".into(),
            genre: String::new(),
            year: None,
            duration_ms: 235_859,
            file_path: "/m/16- De Cacería.mp3".into(),
            source: String::new(),
            cover_path: None,
        };
        let found = identify(&Online, &t, None, &|| Err("no".into())).unwrap().expect("found");
        println!("{found:#?}");
        assert_eq!(found.fields.album.as_deref(), Some("Alzheimer"));
    }
}
