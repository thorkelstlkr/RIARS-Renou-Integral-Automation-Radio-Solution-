//! Completing a track's metadata from an outside source. The lookup itself needs the
//! network and lives in `riars-meta`; here are the parts that do not: what a track has
//! now, applying the changes the operator accepted (recording where they came from),
//! writing them into the file, and the audio fingerprint AcoustID identifies it by.

use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::db::{search_key, MediaLibrary};

/// Seconds of audio in a fingerprint, as `fpcalc` does by default.
const FINGERPRINT_SECONDS: u64 = 120;

/// A track's metadata as stored now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: Option<i32>,
    pub duration_ms: u64,
    pub file_path: String,
    /// Where the last accepted change came from (`musicbrainz:<recording id>`, ...);
    /// empty for tags read from the file.
    pub source: String,
    /// Cover image saved next to the library database.
    pub cover_path: Option<String>,
}

impl TrackMetadata {
    /// The title is only the file name: the file had no title tag.
    pub fn title_from_file_name(&self) -> bool {
        Path::new(&self.file_path)
            .file_stem()
            .is_some_and(|stem| stem.to_string_lossy() == self.title)
    }
}

/// Values to set; `None` leaves a field as it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetadataFields {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
}

impl MetadataFields {
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.artist.is_none() && self.album.is_none() && self.genre.is_none() && self.year.is_none()
    }

    /// Blank strings become `None`, and a year outside 1000..=2999 is dropped.
    pub fn cleaned(mut self) -> Self {
        for slot in [&mut self.title, &mut self.artist, &mut self.album, &mut self.genre] {
            *slot = slot.take().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        }
        self.year = self.year.filter(|y| (1000..=2999).contains(y));
        self
    }
}

/// One change the operator accepted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetadataChange {
    pub track_id: i64,
    pub fields: MetadataFields,
    /// Cover image already saved on disk.
    #[serde(default)]
    pub cover_path: Option<String>,
    /// Where the values came from, e.g. `musicbrainz:<recording id>`.
    pub source: String,
}

pub(crate) fn init_schema(db: &rusqlite::Connection) -> Result<(), String> {
    let columns = [
        // Also added by the automation schema; whichever runs first creates it.
        ("year", "INTEGER"),
        ("meta_source", "TEXT NOT NULL DEFAULT ''"),
        ("meta_updated_at", "TEXT"),
        ("cover_path", "TEXT"),
    ];
    for (name, kind) in columns {
        let exists = db
            .prepare("SELECT 1 FROM pragma_table_info('tracks') WHERE name = ?1")
            .and_then(|mut stmt| stmt.exists(params![name]))
            .map_err(|e| format!("DB error: {e}"))?;
        if !exists {
            db.execute_batch(&format!("ALTER TABLE tracks ADD COLUMN {name} {kind}"))
                .map_err(|e| format!("DB error: {e}"))?;
        }
    }
    Ok(())
}

impl MediaLibrary {
    pub fn track_metadata(&self, id: i64) -> Result<Option<TrackMetadata>, String> {
        self.conn()
            .query_row(
                "SELECT id, title, artist, album, genre, year, duration_ms, file_path, meta_source, cover_path
                 FROM tracks WHERE id = ?1",
                params![id],
                |row| {
                    Ok(TrackMetadata {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        artist: row.get(2)?,
                        album: row.get(3)?,
                        genre: row.get(4)?,
                        year: row.get(5)?,
                        duration_ms: row.get::<_, i64>(6)?.max(0) as u64,
                        file_path: row.get(7)?,
                        source: row.get(8)?,
                        cover_path: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("DB error: {e}"))
    }

    /// The metadata of the library tracks at these paths; paths not in the library are
    /// left out. For the playlist's album, year and genre columns.
    pub fn metadata_for_paths(&self, paths: &[String]) -> Result<Vec<TrackMetadata>, String> {
        let mut found = Vec::new();
        for path in paths {
            if let Some(track) = self.track_by_path(path)? {
                found.extend(self.track_metadata(track.id)?);
            }
        }
        Ok(found)
    }

    /// Tracks missing an artist, album, genre or year, longest first, leaving out those
    /// shorter than `min_duration_ms` (jingles and promos, which no catalogue lists).
    pub fn tracks_missing_metadata(&self, min_duration_ms: u64, limit: usize) -> Result<Vec<i64>, String> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id FROM tracks
                 WHERE (artist = '' OR album = '' OR genre = '' OR year IS NULL)
                   AND duration_ms >= ?1
                 ORDER BY artist, title LIMIT ?2",
            )
            .map_err(|e| format!("DB error: {e}"))?;
        let ids = stmt
            .query_map(params![min_duration_ms as i64, limit.min(i64::MAX as usize) as i64], |row| row.get(0))
            .map_err(|e| format!("DB error: {e}"))?
            .collect::<Result<Vec<i64>, _>>()
            .map_err(|e| format!("DB error: {e}"))?;
        Ok(ids)
    }

    /// Store an accepted change: only the fields it sets, plus where they came from.
    pub fn apply_metadata(&mut self, change: &MetadataChange) -> Result<TrackMetadata, String> {
        let current = self
            .track_metadata(change.track_id)?
            .ok_or_else(|| format!("No track with id {}", change.track_id))?;
        let fields = change.fields.clone().cleaned();
        if fields.is_empty() && change.cover_path.is_none() {
            return Ok(current);
        }
        let title = fields.title.unwrap_or(current.title);
        let artist = fields.artist.unwrap_or(current.artist);
        let album = fields.album.unwrap_or(current.album);
        let genre = fields.genre.unwrap_or(current.genre);
        let year = fields.year.or(current.year);
        let cover = change.cover_path.clone().or(current.cover_path);
        let key = search_key(&[&title, &artist, &album, &genre]);
        let now = chrono::Local::now().naive_local().format("%Y-%m-%d %H:%M:%S").to_string();
        self.conn()
            .execute(
                "UPDATE tracks SET title = ?1, artist = ?2, album = ?3, genre = ?4, year = ?5, cover_path = ?6,
                                   meta_source = ?7, meta_updated_at = ?8, search_key = ?9
                 WHERE id = ?10",
                params![title, artist, album, genre, year, cover, change.source, now, key, change.track_id],
            )
            .map_err(|e| format!("DB error: {e}"))?;
        self.bump_revision();
        self.track_metadata(change.track_id)?
            .ok_or_else(|| format!("No track with id {}", change.track_id))
    }
}

/// Write `fields` (and `cover`, an image file's bytes) into the audio file's own tags.
///
/// Works on a copy next to the file and replaces the original only once the copy has
/// been written and reads back, so a failure never leaves a damaged file behind.
pub fn write_tags(path: &Path, fields: &MetadataFields, cover: Option<&[u8]>) -> Result<(), String> {
    use lofty::config::WriteOptions;
    use lofty::picture::{Picture, PictureType};
    use lofty::prelude::*;
    use lofty::tag::Tag;

    let fields = fields.clone().cleaned();
    if fields.is_empty() && cover.is_none() {
        return Ok(());
    }
    let picture = match cover {
        Some(bytes) => {
            let mut picture =
                Picture::from_reader(&mut &bytes[..]).map_err(|e| format!("Cover is not an image: {e}"))?;
            picture.set_pic_type(PictureType::CoverFront);
            Some(picture)
        }
        None => None,
    };

    let temp = temp_copy_path(path);
    std::fs::copy(path, &temp).map_err(|e| format!("Cannot copy {}: {e}", path.display()))?;
    let result = (|| {
        let mut tagged = lofty::read_from_path(&temp).map_err(|e| format!("Cannot read the tags of {}: {e}", path.display()))?;
        if tagged.primary_tag().is_none() {
            let kind = tagged.primary_tag_type();
            tagged.insert_tag(Tag::new(kind));
        }
        let tag = tagged.primary_tag_mut().ok_or("The file has no tag to write")?;
        if let Some(v) = &fields.title {
            tag.set_title(v.clone());
        }
        if let Some(v) = &fields.artist {
            tag.set_artist(v.clone());
        }
        if let Some(v) = &fields.album {
            tag.set_album(v.clone());
        }
        if let Some(v) = &fields.genre {
            tag.set_genre(v.clone());
        }
        if let Some(v) = fields.year {
            tag.set_year(v as u32);
        }
        if let Some(picture) = picture {
            tag.remove_picture_type(PictureType::CoverFront);
            tag.push_picture(picture);
        }
        tagged
            .save_to_path(&temp, WriteOptions::default())
            .map_err(|e| format!("Cannot write the tags of {}: {e}", path.display()))?;
        lofty::read_from_path(&temp).map_err(|e| format!("The written file does not read back: {e}"))?;
        std::fs::rename(&temp, path).map_err(|e| format!("Cannot replace {} (is it playing?): {e}", path.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// `song.mp3` -> `song.riars-tags.mp3`, keeping the extension so the format is known.
fn temp_copy_path(path: &Path) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = match path.extension() {
        Some(ext) => format!("{stem}.riars-tags.{}", ext.to_string_lossy()),
        None => format!("{stem}.riars-tags"),
    };
    path.with_file_name(name)
}

/// A Chromaprint fingerprint of the first two minutes, as AcoustID expects it
/// (compressed, URL-safe base64 without padding), and the file's length in seconds.
pub fn fingerprint(path: &Path) -> Result<(String, u32), String> {
    use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("Unsupported audio {}: {e}", path.display()))?;
    let mut format = probed.format;
    let track = format.default_track().ok_or("No audio track")?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.ok_or("Unknown sample rate")?;
    let total_frames = track.codec_params.n_frames;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Cannot decode {}: {e}", path.display()))?;

    let config = Configuration::preset_test2();
    let mut printer = Fingerprinter::new(&config);
    let mut started = false;
    let mut buffer: Option<SampleBuffer<i16>> = None;
    let mut frames_read: u64 = 0;
    let limit = FINGERPRINT_SECONDS * rate as u64;
    while frames_read < limit {
        let Ok(packet) = format.next_packet() else { break };
        if packet.track_id() != track_id {
            continue;
        }
        let Ok(decoded) = decoder.decode(&packet) else { continue };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        if !started {
            printer
                .start(rate, channels as u32)
                .map_err(|e| format!("Cannot fingerprint {}: {e:?}", path.display()))?;
            started = true;
        }
        let buf = buffer.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        if buf.capacity() < decoded.capacity() * channels {
            *buf = SampleBuffer::new(decoded.capacity() as u64, spec);
        }
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();
        let wanted = ((limit - frames_read) as usize).saturating_mul(channels).min(samples.len());
        printer.consume(&samples[..wanted]);
        frames_read += (samples.len() / channels) as u64;
    }
    if !started {
        return Err(format!("No audio decoded from {}", path.display()));
    }
    printer.finish();
    let raw = printer.fingerprint();
    if raw.is_empty() {
        return Err(format!("{} is too short to identify", path.display()));
    }
    let compressed = FingerprintCompressor::from(&config).compress(raw);
    // The length of the whole file; without it in the header, count what was decoded.
    let seconds = match total_frames {
        Some(frames) if rate > 0 => frames / rate as u64,
        _ => {
            while let Ok(packet) = format.next_packet() {
                if packet.track_id() == track_id {
                    frames_read += packet.dur;
                }
            }
            frames_read / rate.max(1) as u64
        }
    };
    Ok((base64_url(&compressed), seconds.min(u32::MAX as u64) as u32))
}

/// URL-safe base64 without padding, the alphabet Chromaprint uses.
fn base64_url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("riars-metadata-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 16-bit stereo WAV of a few tones, `seconds` long.
    fn write_wav(path: &Path, seconds: u32) {
        let rate = 44_100u32;
        let frames = rate * seconds;
        let mut data = Vec::with_capacity(frames as usize * 4);
        for i in 0..frames {
            let t = i as f32 / rate as f32;
            let freq = [440.0, 554.4, 659.3, 880.0][(t as usize / 2) % 4];
            let v = ((t * freq * std::f32::consts::TAU).sin() * 12_000.0) as i16;
            data.extend_from_slice(&v.to_le_bytes());
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(path, wav).unwrap();
    }

    fn insert(lib: &MediaLibrary, title: &str, artist: &str, duration_ms: i64, path: &str) -> i64 {
        lib.conn()
            .execute(
                "INSERT INTO tracks (title, artist, album, genre, duration_ms, file_path) VALUES (?1, ?2, '', '', ?3, ?4)",
                params![title, artist, duration_ms, path],
            )
            .unwrap();
        lib.conn().last_insert_rowid()
    }

    #[test]
    fn apply_sets_only_the_given_fields_and_records_the_source() {
        let mut lib = MediaLibrary::open(":memory:").unwrap();
        let id = insert(&lib, "Roma", "Amulet", 230_000, "/m/07 Roma.mp3");
        let before = lib.revision();
        let change = MetadataChange {
            track_id: id,
            fields: MetadataFields {
                album: Some("Petit Viking".into()),
                genre: Some("  ".into()),
                year: Some(2017),
                ..Default::default()
            },
            cover_path: Some("/covers/x.jpg".into()),
            source: "musicbrainz:abc".into(),
        };
        let after = lib.apply_metadata(&change).unwrap();
        assert_eq!(after.title, "Roma");
        assert_eq!(after.artist, "Amulet");
        assert_eq!(after.album, "Petit Viking");
        assert_eq!(after.genre, "", "a blank value changes nothing");
        assert_eq!(after.year, Some(2017));
        assert_eq!(after.source, "musicbrainz:abc");
        assert_eq!(after.cover_path.as_deref(), Some("/covers/x.jpg"));
        assert!(lib.revision() > before);
        assert_eq!(lib.query("petit viking", 10, 0).unwrap().total, 1, "the search sees the new album");
        assert!(lib.apply_metadata(&MetadataChange { track_id: 999, ..Default::default() }).is_err());
    }

    #[test]
    fn gaps_leave_out_short_tracks_and_complete_ones() {
        let mut lib = MediaLibrary::open(":memory:").unwrap();
        let song = insert(&lib, "Song", "", 200_000, "/m/song.mp3");
        insert(&lib, "Jingle", "", 8_000, "/m/jingle.wav");
        let full = insert(&lib, "Full", "A", 200_000, "/m/full.mp3");
        lib.apply_metadata(&MetadataChange {
            track_id: full,
            fields: MetadataFields { album: Some("B".into()), genre: Some("Rock".into()), year: Some(1999), ..Default::default() },
            cover_path: None,
            source: "test".into(),
        })
        .unwrap();
        assert_eq!(lib.tracks_missing_metadata(60_000, 100).unwrap(), vec![song]);
        assert_eq!(lib.tracks_missing_metadata(0, 100).unwrap().len(), 2);
        let meta = lib.track_metadata(song).unwrap().unwrap();
        assert!(!meta.title_from_file_name());
        let jingle = lib.track_by_path("/m/jingle.wav").unwrap().unwrap();
        let jingle = lib.track_metadata(jingle.id).unwrap().unwrap();
        assert!(!jingle.title_from_file_name(), "title is 'Jingle', the stem is 'jingle'");
    }

    #[test]
    fn metadata_by_path_skips_files_not_in_the_library() {
        let mut lib = MediaLibrary::open(":memory:").unwrap();
        let id = insert(&lib, "Roma", "Amulet", 230_000, "/m/07 Roma.mp3");
        lib.apply_metadata(&MetadataChange {
            track_id: id,
            fields: MetadataFields { album: Some("Petit Viking".into()), year: Some(2017), ..Default::default() },
            cover_path: None,
            source: "test".into(),
        })
        .unwrap();
        let paths = vec!["/m/elsewhere.mp3".to_string(), "/m/07 Roma.mp3".to_string()];
        let found = lib.metadata_for_paths(&paths).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_path, "/m/07 Roma.mp3");
        assert_eq!(found[0].album, "Petit Viking");
        assert_eq!(found[0].year, Some(2017));
    }

    #[test]
    fn tags_are_written_into_the_file_and_read_back() {
        let dir = temp_dir("write");
        let path = dir.join("untagged.wav");
        write_wav(&path, 2);
        let fields = MetadataFields {
            title: Some("Estrella".into()),
            artist: Some("Amulet".into()),
            album: Some("Petit Viking".into()),
            genre: Some("Folk".into()),
            year: Some(2017),
        };
        write_tags(&path, &fields, None).unwrap();
        let tags = super::super::db::read_tags(&path);
        assert_eq!(tags.title.as_deref(), Some("Estrella"));
        assert_eq!(tags.artist.as_deref(), Some("Amulet"));
        assert_eq!(tags.album.as_deref(), Some("Petit Viking"));
        assert_eq!(tags.genre.as_deref(), Some("Folk"));
        assert!(tags.duration_ms >= 1900, "the audio is still there");
        assert!(!temp_copy_path(&path).exists(), "no copy left behind");
        assert!(write_tags(&path, &fields, Some(b"not an image")).is_err());
        assert!(!temp_copy_path(&path).exists(), "no copy left behind after a failure");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fingerprint_of_a_wav() {
        let dir = temp_dir("fp");
        let path = dir.join("tones.wav");
        write_wav(&path, 20);
        let (fp, seconds) = fingerprint(&path).unwrap();
        assert_eq!(seconds, 20);
        assert!(fp.len() > 50, "fingerprint too short: {fp}");
        assert!(fp.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        // Algorithm 1 (fpcalc's default) is the first byte of the header: 'AQ' in base64.
        assert!(fp.starts_with("AQ"), "{fp}");
        assert!(fingerprint(&dir.join("missing.wav")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64_url(b""), "");
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert_eq!(base64_url(&[0xfb, 0xff]), "-_8");
    }
}
