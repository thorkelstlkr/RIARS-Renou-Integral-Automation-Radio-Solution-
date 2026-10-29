//! Deciding which catalogue recording a track is, and what to propose for it.

use riars_core::library::db::fold;
use riars_core::{MetadataFields, TrackMetadata};
use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, Genre, Recording, Release, ReleaseGroup, COVER_ART};

/// Below this, a text match is not proposed at all.
const MIN_CONFIDENCE: f32 = 0.75;
/// AcoustID scores below this are noise.
const MIN_ACOUSTID_SCORE: f32 = 0.7;

/// How the recording was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Title and artist from the tags.
    Tags,
    /// "Artist - Title" read from the file name.
    FileName,
    /// The audio fingerprint, through AcoustID.
    Fingerprint,
}

/// What the catalogue says about a track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Found {
    /// Every value found (`None` where the catalogue has none).
    pub fields: MetadataFields,
    /// Fields worth changing without a second thought: empty now, or a title that is
    /// only the file name. The others are shown, unticked.
    pub suggested: Vec<String>,
    pub recording_id: String,
    pub release_group_id: Option<String>,
    /// Front cover of the release group, if the Cover Art Archive has one.
    pub cover_url: Option<String>,
    pub method: Method,
    /// 0 to 1.
    pub confidence: f32,
    /// What to record as the origin of the values: `musicbrainz:<recording id>`.
    pub source: String,
}

/// Lowercase, no accents, nothing in brackets, no "feat." part, single spaces.
pub(crate) fn normalize(text: &str) -> String {
    let folded = fold(text);
    let mut out = String::with_capacity(folded.len());
    let mut depth = 0usize;
    for c in folded.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    let words: Vec<&str> = out.split_whitespace().collect();
    let cut = words.iter().position(|w| matches!(*w, "feat" | "ft" | "featuring")).unwrap_or(words.len());
    words[..cut].join(" ")
}

/// Dice coefficient of character bigrams: 1 for equal strings, near 0 for unrelated ones.
pub(crate) fn similarity(a: &str, b: &str) -> f32 {
    let (a, b) = (normalize(a), normalize(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let bigrams = |s: &str| {
        let chars: Vec<char> = s.chars().filter(|c| *c != ' ').collect();
        chars.windows(2).map(|w| (w[0], w[1])).collect::<Vec<_>>()
    };
    let (x, mut y) = (bigrams(&a), bigrams(&b));
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    let total = (x.len() + y.len()) as f32;
    let mut common = 0;
    for pair in &x {
        if let Some(i) = y.iter().position(|p| p == pair) {
            y.swap_remove(i);
            common += 1;
        }
    }
    2.0 * common as f32 / total
}

fn duration_score(ours_ms: u64, theirs_ms: Option<u64>) -> f32 {
    match theirs_ms {
        Some(theirs) if ours_ms > 0 => {
            let diff = ours_ms.abs_diff(theirs);
            match diff {
                0..=3_000 => 1.0,
                3_001..=10_000 => 0.7,
                10_001..=30_000 => 0.3,
                _ => 0.0,
            }
        }
        // Nothing to compare: neither for nor against.
        _ => 0.7,
    }
}

/// How well `recording` fits a track called `title` by `artist` (empty if unknown).
pub(crate) fn score(recording: &Recording, title: &str, artist: &str, duration_ms: u64) -> f32 {
    let title_score = similarity(title, &recording.title);
    let duration = duration_score(duration_ms, recording.length);
    if artist.trim().is_empty() {
        // Title alone says little: only an exact title and length are worth proposing.
        return if title_score >= 0.95 && duration >= 1.0 { 0.75 } else { 0.0 };
    }
    let first = recording.artist_credit.first().map(|c| c.name.as_str()).unwrap_or("");
    let artist_score = similarity(artist, &recording.artist()).max(similarity(artist, first));
    if title_score < 0.8 || artist_score < 0.6 {
        return 0.0;
    }
    0.5 * title_score + 0.3 * artist_score + 0.2 * duration
}

/// "01 - Amulet - Roma" -> ("Amulet", "Roma"). A file name with no " - " gives nothing.
pub(crate) fn split_file_name(path: &str) -> Option<(String, String)> {
    // Either separator, whatever the system: a library made on Windows can be served
    // from Linux (a Raspberry Pi), where `Path` would not split at `\`.
    let name = path.rsplit(['/', '\\']).next()?;
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem).replace('_', " ");
    let stem = stem.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, ' ' | '.' | '-'));
    let (artist, title) = stem.split_once(" - ")?;
    let (artist, title) = (artist.trim(), title.trim());
    (!artist.is_empty() && !title.is_empty()).then(|| (artist.to_string(), title.to_string()))
}

fn year_of(date: Option<&str>) -> Option<i32> {
    date.and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()).filter(|y| (1000..=2999).contains(y))
}

/// Rank of a release for "the album this song is on": official studio albums first.
fn release_rank(release: &Release, group: &ReleaseGroup) -> u8 {
    let official = release.status.as_deref() == Some("Official");
    let plain = group.secondary_types.is_empty();
    let base = match group.primary_type.as_deref() {
        Some("Album") if plain => 0,
        Some("EP") if plain => 1,
        Some("Single") if plain => 2,
        Some("Album") => 3,
        _ => 4,
    };
    base * 2 + u8::from(!official)
}

/// The release to take the album from: the one already named in the tags if the
/// recording is on it, else the earliest official studio album.
pub(crate) fn pick_release<'a>(recording: &'a Recording, current_album: &str) -> Option<(&'a Release, &'a ReleaseGroup)> {
    let with_group = || recording.releases.iter().filter_map(|r| r.release_group.as_ref().map(|g| (r, g)));
    if !current_album.trim().is_empty() {
        let named = with_group()
            .filter(|(r, g)| similarity(current_album, &g.title).max(similarity(current_album, &r.title)) >= 0.85)
            .min_by_key(|(r, _)| r.date.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| "9999".into()));
        if named.is_some() {
            return named;
        }
    }
    with_group().min_by_key(|(r, g)| {
        (release_rank(r, g), r.date.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| "9999".into()))
    })
}

/// The most voted genre, "folk rock" -> "Folk Rock".
pub(crate) fn top_genre(genres: &[Genre]) -> Option<String> {
    let best = genres.iter().filter(|g| !g.name.trim().is_empty()).max_by_key(|g| g.count)?;
    Some(
        best.name
            .split(' ')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => String::new(),
                }
            })
            .collect::<Vec<String>>()
            .join(" "),
    )
}

/// Order of equally good candidates, smallest first. The same song is often several
/// recordings (the original, a live take, a re-recording): prefer the one on the album
/// already in the tags, then the better score in steps of 0.05, then the oldest.
fn preference(score: f32, recording: &Recording, album: &str) -> (bool, i32, String) {
    let on_album = !album.trim().is_empty()
        && recording.releases.iter().any(|r| {
            let group = r.release_group.as_ref().map(|g| g.title.as_str()).unwrap_or("");
            similarity(album, &r.title).max(similarity(album, group)) >= 0.85
        });
    let first = recording.first_release_date.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| "9999".into());
    (!on_album, -((score * 20.0).round() as i32), first)
}

/// Fill in a proposal from a chosen recording, asking the catalogue for what it lacks.
fn build(
    catalog: &dyn Catalog,
    track: &TrackMetadata,
    recording_id: &str,
    method: Method,
    confidence: f32,
) -> Result<Found, String> {
    let recording = catalog.recording(recording_id)?;
    let release = pick_release(&recording, &track.album);
    let group = release.map(|(_, g)| g);
    let mut genre = top_genre(&recording.genres);
    if genre.is_none() {
        if let Some(group) = group {
            genre = top_genre(&catalog.release_group_genres(&group.id).unwrap_or_default());
        }
    }
    if genre.is_none() {
        if let Some(credit) = recording.artist_credit.first().filter(|c| !c.artist.id.is_empty()) {
            genre = top_genre(&catalog.artist_genres(&credit.artist.id).unwrap_or_default());
        }
    }
    let year = year_of(recording.first_release_date.as_deref()).or_else(|| release.and_then(|(r, _)| year_of(r.date.as_deref())));
    let artist = recording.artist();
    let fields = MetadataFields {
        title: Some(recording.title.clone()).filter(|t| !t.is_empty()),
        artist: Some(artist).filter(|a| !a.is_empty()),
        album: group.map(|g| g.title.clone()).filter(|a| !a.is_empty()),
        genre,
        year,
    };
    let suggested = suggest(track, &fields);
    Ok(Found {
        suggested,
        recording_id: recording.id.clone(),
        release_group_id: group.map(|g| g.id.clone()),
        cover_url: group.map(|g| format!("{COVER_ART}/release-group/{}/front-500", g.id)),
        method,
        confidence: (confidence * 100.0).round() / 100.0,
        source: format!("musicbrainz:{}", recording.id),
        fields,
    })
}

/// Fields to tick: those empty now (or a title that is just the file name) where the
/// catalogue has something different.
pub(crate) fn suggest(track: &TrackMetadata, found: &MetadataFields) -> Vec<String> {
    let mut out = Vec::new();
    let mut text = |name: &str, current: &str, empty: bool, value: &Option<String>| {
        if let Some(value) = value {
            if empty && value != current {
                out.push(name.to_string());
            }
        }
    };
    text("title", &track.title, track.title.is_empty() || track.title_from_file_name(), &found.title);
    text("artist", &track.artist, track.artist.is_empty(), &found.artist);
    text("album", &track.album, track.album.is_empty(), &found.album);
    text("genre", &track.genre, track.genre.is_empty(), &found.genre);
    if found.year.is_some() && track.year.is_none() {
        out.push("year".to_string());
    }
    out
}

/// Look `track` up: by its tags, then by "Artist - Title" in the file name, then by its
/// fingerprint when an AcoustID key is given (`fingerprint` computes it). `Ok(None)` when
/// nothing fits well enough.
pub fn identify(
    catalog: &dyn Catalog,
    track: &TrackMetadata,
    acoustid_key: Option<&str>,
    fingerprint: &dyn Fn() -> Result<(String, u32), String>,
) -> Result<Option<Found>, String> {
    let mut attempts: Vec<(String, String, Method)> = Vec::new();
    if !track.title.is_empty() && !track.title_from_file_name() {
        attempts.push((track.title.clone(), track.artist.clone(), Method::Tags));
    }
    if let Some((artist, title)) = split_file_name(&track.file_path) {
        if track.artist.is_empty() || track.title_from_file_name() {
            attempts.push((title, artist, Method::FileName));
        }
    }
    for (title, artist, method) in attempts {
        let artist_query = Some(artist.as_str()).filter(|a| !a.trim().is_empty());
        let candidates = catalog.search(&title, artist_query)?;
        let best = candidates
            .iter()
            .map(|r| (score(r, &title, &artist, track.duration_ms), r))
            .filter(|(s, _)| *s >= MIN_CONFIDENCE)
            .min_by_key(|(s, r)| preference(*s, r, &track.album));
        if let Some((confidence, recording)) = best {
            return build(catalog, track, &recording.id, method, confidence).map(Some);
        }
    }
    let Some(key) = acoustid_key.map(str::trim).filter(|k| !k.is_empty()) else {
        return Ok(None);
    };
    let (print, seconds) = fingerprint()?;
    let matches = catalog.acoustid(key, &print, seconds)?;
    let best = matches
        .iter()
        .filter(|m| m.score >= MIN_ACOUSTID_SCORE)
        .flat_map(|m| m.recordings.iter().map(move |r| (m.score, r)))
        // Several recordings can share one sound (album, single, compilation): the one
        // named like the track, if it has a name, else the first.
        .max_by(|a, b| {
            let fit = |r: &Recording| if track.title_from_file_name() { 0.0 } else { similarity(&track.title, &r.title) };
            (a.0 + fit(a.1)).total_cmp(&(b.0 + fit(b.1)))
        });
    match best {
        Some((score, recording)) => build(catalog, track, &recording.id, Method::Fingerprint, score).map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AcoustMatch, ArtistRef, Credit};
    use std::sync::Mutex;

    pub(crate) fn track(title: &str, artist: &str, album: &str, path: &str, duration_ms: u64) -> TrackMetadata {
        TrackMetadata {
            id: 1,
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            genre: String::new(),
            year: None,
            duration_ms,
            file_path: path.into(),
            source: String::new(),
            cover_path: None,
        }
    }

    fn rec(id: &str, title: &str, artist: &str, length: u64) -> Recording {
        Recording {
            id: id.into(),
            title: title.into(),
            length: Some(length),
            artist_credit: vec![Credit { name: artist.into(), joinphrase: String::new(), artist: ArtistRef { id: format!("artist-{artist}"), name: artist.into() } }],
            ..Default::default()
        }
    }

    fn release(id: &str, title: &str, kind: &str, secondary: &[&str], date: &str) -> Release {
        Release {
            id: id.into(),
            title: title.into(),
            status: Some("Official".into()),
            date: Some(date.into()),
            release_group: Some(ReleaseGroup {
                id: format!("rg-{id}"),
                title: title.into(),
                primary_type: Some(kind.into()),
                secondary_types: secondary.iter().map(|s| s.to_string()).collect(),
            }),
        }
    }

    /// A catalogue in memory that counts what it is asked.
    #[derive(Default)]
    pub(crate) struct Fake {
        pub(crate) search: Vec<Recording>,
        pub(crate) details: Vec<Recording>,
        pub(crate) group_genres: Vec<Genre>,
        pub(crate) artist_genres: Vec<Genre>,
        pub(crate) acoustid: Vec<AcoustMatch>,
        pub(crate) calls: Mutex<Vec<String>>,
    }

    impl Catalog for Fake {
        fn search(&self, title: &str, artist: Option<&str>) -> Result<Vec<Recording>, String> {
            self.calls.lock().unwrap().push(format!("search {title} / {}", artist.unwrap_or("-")));
            Ok(self.search.clone())
        }
        fn recording(&self, id: &str) -> Result<Recording, String> {
            self.calls.lock().unwrap().push(format!("recording {id}"));
            self.details.iter().find(|r| r.id == id).cloned().ok_or_else(|| "not found".into())
        }
        fn release_group_genres(&self, id: &str) -> Result<Vec<Genre>, String> {
            self.calls.lock().unwrap().push(format!("group {id}"));
            Ok(self.group_genres.clone())
        }
        fn artist_genres(&self, id: &str) -> Result<Vec<Genre>, String> {
            self.calls.lock().unwrap().push(format!("artist {id}"));
            Ok(self.artist_genres.clone())
        }
        fn acoustid(&self, key: &str, _: &str, duration: u32) -> Result<Vec<AcoustMatch>, String> {
            self.calls.lock().unwrap().push(format!("acoustid {key} {duration}"));
            Ok(self.acoustid.clone())
        }
    }

    fn no_fingerprint() -> Result<(String, u32), String> {
        panic!("the fingerprint should not be needed")
    }

    #[test]
    fn normalizing_ignores_case_accents_brackets_and_guests() {
        assert_eq!(normalize("Ni Permís ni Perdó"), "ni permis ni perdo");
        assert_eq!(normalize("Roma (Remastered 2020) [Live]"), "roma");
        assert_eq!(normalize("Amulet feat. Mar Pujol"), "amulet");
        assert_eq!(normalize("Amulet ft. Carla Collado"), "amulet");
        assert_eq!(similarity("Dímelo Tú", "Dimelo tu"), 1.0);
        assert!(similarity("Estrella", "Estrellas") > 0.85);
        assert!(similarity("Estrella", "Amor") < 0.2);
        assert_eq!(similarity("", "x"), 0.0);
    }

    #[test]
    fn scores_need_title_artist_and_length_to_agree() {
        let r = rec("r", "Petit Viking", "Amulet", 202_000);
        assert!(score(&r, "Petit Viking", "Amulet", 202_109) > 0.95);
        assert!(score(&r, "Petit Viking", "Amulet", 0) >= MIN_CONFIDENCE, "no length to compare is neutral");
        assert_eq!(score(&r, "Petit Viking", "Def Con Dos", 202_109), 0.0, "wrong artist");
        assert_eq!(score(&r, "Estrella", "Amulet", 202_109), 0.0, "wrong title");
        assert!(score(&r, "Petit Viking", "Amulet", 290_000) < 0.9, "far off in length scores lower");
        assert_eq!(score(&r, "Petit Viking", "", 202_109), 0.75, "title alone needs an exact length");
        assert_eq!(score(&r, "Petit Viking", "", 230_000), 0.0);
    }

    #[test]
    fn file_names_split_into_artist_and_title() {
        assert_eq!(split_file_name(r"D:\m\01 - Def Con Dos - De Cacería.mp3"), Some(("Def Con Dos".into(), "De Cacería".into())));
        assert_eq!(split_file_name("/m/Amulet - Roma.wav"), Some(("Amulet".into(), "Roma".into())));
        assert_eq!(split_file_name("/m/Amulet_-_Roma.wav"), Some(("Amulet".into(), "Roma".into())));
        assert_eq!(split_file_name("/m/07 Roma.mp3"), None);
        assert_eq!(split_file_name("/m/TRACK 1-thorkel-58747-RESULT 2.wav"), None);
    }

    #[test]
    fn the_album_is_the_earliest_studio_album_or_the_one_already_named() {
        let mut r = rec("r", "Roma", "Amulet", 230_000);
        r.releases = vec![
            release("comp", "Rock Català 2018", "Album", &["Compilation"], "2018-01-01"),
            release("single", "Roma", "Single", &[], "2016-11-01"),
            release("album", "Petit Viking", "Album", &[], "2017-03-10"),
            release("reissue", "Petit Viking", "Album", &[], "2020-01-01"),
        ];
        assert_eq!(pick_release(&r, "").unwrap().0.id, "album");
        assert_eq!(pick_release(&r, "Rock Catala 2018").unwrap().0.id, "comp");
        assert_eq!(pick_release(&r, "Something else").unwrap().0.id, "album");
        assert!(pick_release(&rec("x", "x", "x", 1), "").is_none());
    }

    #[test]
    fn genres_take_the_most_voted_and_capitalise() {
        let genres = [Genre { name: "folk rock".into(), count: 3 }, Genre { name: "pop".into(), count: 1 }];
        assert_eq!(top_genre(&genres).as_deref(), Some("Folk Rock"));
        assert_eq!(top_genre(&[]), None);
    }

    #[test]
    fn a_tagged_track_is_found_by_text_and_only_its_gaps_are_ticked() {
        let mut details = rec("r1", "Estrella", "Amulet", 279_000);
        details.first_release_date = Some("2017-03-10".into());
        details.releases = vec![release("album", "Petit Viking", "Album", &[], "2017-03-10")];
        let fake = Fake {
            search: vec![rec("r0", "Estrella fugaz", "Otro", 100_000), rec("r1", "Estrella", "Amulet", 279_000)],
            details: vec![details],
            group_genres: vec![],
            artist_genres: vec![Genre { name: "folk metal".into(), count: 2 }],
            ..Default::default()
        };
        let t = track("Estrella", "Amulet", "Petit Viking", r"D:\Musica\Amulet\04 Estrella.mp3", 279_797);
        let found = identify(&fake, &t, None, &no_fingerprint).unwrap().unwrap();
        assert_eq!(found.recording_id, "r1");
        assert_eq!(found.method, Method::Tags);
        assert_eq!(found.fields.album.as_deref(), Some("Petit Viking"));
        assert_eq!(found.fields.year, Some(2017));
        assert_eq!(found.fields.genre.as_deref(), Some("Folk Metal"), "from the artist when the album has none");
        assert_eq!(found.suggested, vec!["genre".to_string(), "year".to_string()]);
        assert_eq!(found.cover_url.as_deref(), Some("https://coverartarchive.org/release-group/rg-album/front-500"));
        assert_eq!(found.source, "musicbrainz:r1");
        let calls = fake.calls.lock().unwrap().clone();
        assert_eq!(calls, vec!["search Estrella / Amulet", "recording r1", "group rg-album", "artist artist-Amulet"]);
    }

    #[test]
    fn nothing_close_enough_is_no_match_and_no_key_means_no_fingerprint() {
        let fake = Fake { search: vec![rec("r0", "Other song", "Amulet", 100_000)], ..Default::default() };
        let t = track("Estrella", "Amulet", "", "/m/04 Estrella.mp3", 279_797);
        assert_eq!(identify(&fake, &t, None, &no_fingerprint).unwrap(), None);
        assert_eq!(identify(&fake, &t, Some("  "), &no_fingerprint).unwrap(), None);
    }

    #[test]
    fn an_untagged_file_is_found_by_its_fingerprint() {
        let mut details = rec("r9", "Roma", "Amulet", 230_000);
        details.artist_credit[0].joinphrase = " feat. ".into();
        details.artist_credit.push(Credit { name: "Mar Pujol".into(), joinphrase: String::new(), artist: ArtistRef::default() });
        details.releases = vec![release("album", "Petit Viking", "Album", &[], "2017-03-10")];
        details.genres = vec![Genre { name: "folk".into(), count: 1 }];
        let fake = Fake {
            details: vec![details],
            acoustid: vec![
                AcoustMatch { score: 0.4, recordings: vec![rec("r-noise", "Noise", "X", 1)] },
                AcoustMatch { score: 0.92, recordings: vec![rec("r9", "Roma", "Amulet", 230_000)] },
            ],
            ..Default::default()
        };
        let t = track("untitled", "", "", "/m/untitled.wav", 230_164);
        let found = identify(&fake, &t, Some("KEY"), &|| Ok(("AQAA".into(), 230))).unwrap().unwrap();
        assert_eq!(found.method, Method::Fingerprint);
        assert_eq!(found.recording_id, "r9");
        assert_eq!(found.confidence, 0.92);
        assert_eq!(found.fields.artist.as_deref(), Some("Amulet feat. Mar Pujol"));
        assert_eq!(found.fields.genre.as_deref(), Some("Folk"));
        assert_eq!(found.suggested, vec!["title", "artist", "album", "genre", "year"]);
        let calls = fake.calls.lock().unwrap().clone();
        assert_eq!(calls, vec!["acoustid KEY 230", "recording r9"], "no text search for a bare file name");
    }

    #[test]
    fn among_equal_recordings_the_one_on_the_named_album_or_the_oldest_wins() {
        let mut live = rec("live", "De cacería", "Def Con Dos", 235_000);
        live.first_release_date = Some("2006".into());
        live.releases = vec![release("live", "6 dementes contra el mundo", "Album", &["Live"], "2006")];
        let mut original = rec("orig", "De cacería", "Def Con Dos", 235_000);
        original.first_release_date = Some("1995".into());
        original.releases = vec![release("orig", "Alzheimer", "Album", &[], "1995")];
        let fake = Fake { search: vec![live.clone(), original.clone()], details: vec![live, original], ..Default::default() };
        let tagged = track("De Cacería", "Def Con Dos", "Alzheimer", "/m/16- De Cacería.mp3", 235_859);
        assert_eq!(identify(&fake, &tagged, None, &no_fingerprint).unwrap().unwrap().recording_id, "orig");
        let no_album = track("De Cacería", "Def Con Dos", "", "/m/16- De Cacería.mp3", 235_859);
        let found = identify(&fake, &no_album, None, &no_fingerprint).unwrap().unwrap();
        assert_eq!(found.recording_id, "orig", "the oldest");
        assert_eq!(found.fields.album.as_deref(), Some("Alzheimer"));
        assert_eq!(found.fields.year, Some(1995));
    }

    #[test]
    fn a_file_name_with_artist_and_title_is_searched() {
        let mut details = rec("r5", "De Cacería", "Def Con Dos", 235_000);
        details.first_release_date = Some("1995".into());
        let fake = Fake { search: vec![rec("r5", "De Cacería", "Def Con Dos", 235_000)], details: vec![details], ..Default::default() };
        let t = track("Def Con Dos - De Cacería", "", "", "/m/Def Con Dos - De Cacería.mp3", 235_859);
        let found = identify(&fake, &t, None, &no_fingerprint).unwrap().unwrap();
        assert_eq!(found.method, Method::FileName);
        assert_eq!(found.fields.year, Some(1995));
        assert!(found.suggested.contains(&"title".to_string()));
        assert!(found.suggested.contains(&"artist".to_string()));
        assert!(!found.suggested.contains(&"album".to_string()), "the recording has no release");
    }
}
