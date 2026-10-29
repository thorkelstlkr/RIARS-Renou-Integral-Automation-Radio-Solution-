//! The outside sources: MusicBrainz (recordings, releases, genres; CC0 data) and
//! AcoustID (recognising a recording by its audio fingerprint).

use serde::Deserialize;

use crate::http::{self, HttpError};

const MUSICBRAINZ: &str = "https://musicbrainz.org/ws/2";
const ACOUSTID: &str = "https://api.acoustid.org/v2/lookup";
pub const COVER_ART: &str = "https://coverartarchive.org";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Recording {
    pub id: String,
    pub title: String,
    /// Milliseconds.
    pub length: Option<u64>,
    #[serde(rename = "artist-credit")]
    pub artist_credit: Vec<Credit>,
    #[serde(rename = "first-release-date")]
    pub first_release_date: Option<String>,
    pub releases: Vec<Release>,
    pub genres: Vec<Genre>,
}

impl Recording {
    /// The credit as printed: "Amulet feat. Mar Pujol".
    pub fn artist(&self) -> String {
        self.artist_credit.iter().map(|c| format!("{}{}", c.name, c.joinphrase)).collect::<String>().trim().to_string()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Credit {
    pub name: String,
    pub joinphrase: String,
    pub artist: ArtistRef,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Release {
    pub id: String,
    pub title: String,
    pub status: Option<String>,
    pub date: Option<String>,
    #[serde(rename = "release-group")]
    pub release_group: Option<ReleaseGroup>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ReleaseGroup {
    pub id: String,
    pub title: String,
    #[serde(rename = "primary-type")]
    pub primary_type: Option<String>,
    #[serde(rename = "secondary-types")]
    pub secondary_types: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Genre {
    pub name: String,
    pub count: u32,
}

/// One AcoustID result: how sure it is, and the recordings that sound like this.
#[derive(Debug, Clone, Default)]
pub struct AcoustMatch {
    pub score: f32,
    pub recordings: Vec<Recording>,
}

/// What the lookup asks of the outside world, so it can be tested without it.
pub trait Catalog: Send + Sync {
    /// Recordings whose title is `title` (and artist `artist`, when given), best first.
    fn search(&self, title: &str, artist: Option<&str>) -> Result<Vec<Recording>, String>;
    /// One recording with its artists, releases and genres.
    fn recording(&self, id: &str) -> Result<Recording, String>;
    fn release_group_genres(&self, id: &str) -> Result<Vec<Genre>, String>;
    fn artist_genres(&self, id: &str) -> Result<Vec<Genre>, String>;
    /// Recordings that sound like `fingerprint`; `duration` is the file's length in seconds.
    fn acoustid(&self, key: &str, fingerprint: &str, duration: u32) -> Result<Vec<AcoustMatch>, String>;
}

/// The real services, over HTTP.
pub struct Online;

/// A Lucene phrase: quotes and backslashes escaped.
fn phrase(text: &str) -> String {
    let escaped: String = text
        .chars()
        .flat_map(|c| match c {
            '"' | '\\' => vec!['\\', c],
            _ => vec![c],
        })
        .collect();
    format!("\"{escaped}\"")
}

pub(crate) fn search_query(title: &str, artist: Option<&str>) -> String {
    match artist.map(str::trim).filter(|a| !a.is_empty()) {
        Some(artist) => format!("recording:{} AND artist:{}", phrase(title.trim()), phrase(artist)),
        None => format!("recording:{}", phrase(title.trim())),
    }
}

fn http_err(e: HttpError) -> String {
    format!("MusicBrainz: {e}")
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SearchAnswer {
    recordings: Vec<Recording>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WithGenres {
    genres: Vec<Genre>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AcoustAnswer {
    status: String,
    error: Option<AcoustError>,
    results: Vec<AcoustResult>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AcoustError {
    message: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AcoustResult {
    score: f32,
    recordings: Vec<AcoustRecording>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AcoustRecording {
    id: String,
    title: String,
    duration: Option<f64>,
    artists: Vec<ArtistRef>,
}

#[cfg(test)]
pub(crate) fn parse_acoustid(json: &str) -> Result<Vec<AcoustMatch>, String> {
    let answer: AcoustAnswer = serde_json::from_str(json).map_err(|e| format!("AcoustID: bad answer: {e}"))?;
    acoust_matches(answer)
}

fn acoust_matches(answer: AcoustAnswer) -> Result<Vec<AcoustMatch>, String> {
    if answer.status != "ok" {
        let message = answer.error.map(|e| e.message).unwrap_or_else(|| answer.status.clone());
        return Err(format!("AcoustID: {message}"));
    }
    Ok(answer
        .results
        .into_iter()
        .map(|result| AcoustMatch {
            score: result.score,
            recordings: result
                .recordings
                .into_iter()
                .filter(|r| !r.id.is_empty())
                .map(|r| Recording {
                    id: r.id,
                    title: r.title,
                    length: r.duration.map(|s| (s * 1000.0) as u64),
                    artist_credit: r
                        .artists
                        .into_iter()
                        .map(|a| Credit { name: a.name.clone(), joinphrase: String::new(), artist: a })
                        .collect(),
                    ..Default::default()
                })
                .collect(),
        })
        .collect())
}

impl Catalog for Online {
    fn search(&self, title: &str, artist: Option<&str>) -> Result<Vec<Recording>, String> {
        let query = search_query(title, artist);
        let answer: SearchAnswer = http::get_json(
            &http::MUSICBRAINZ,
            &format!("{MUSICBRAINZ}/recording"),
            &[("query", &query), ("limit", "15"), ("fmt", "json")],
        )
        .map_err(http_err)?;
        Ok(answer.recordings)
    }

    fn recording(&self, id: &str) -> Result<Recording, String> {
        http::get_json(
            &http::MUSICBRAINZ,
            &format!("{MUSICBRAINZ}/recording/{id}"),
            &[("inc", "artist-credits+releases+release-groups+genres"), ("fmt", "json")],
        )
        .map_err(http_err)
    }

    fn release_group_genres(&self, id: &str) -> Result<Vec<Genre>, String> {
        let answer: WithGenres = http::get_json(
            &http::MUSICBRAINZ,
            &format!("{MUSICBRAINZ}/release-group/{id}"),
            &[("inc", "genres"), ("fmt", "json")],
        )
        .map_err(http_err)?;
        Ok(answer.genres)
    }

    fn artist_genres(&self, id: &str) -> Result<Vec<Genre>, String> {
        let answer: WithGenres = http::get_json(
            &http::MUSICBRAINZ,
            &format!("{MUSICBRAINZ}/artist/{id}"),
            &[("inc", "genres"), ("fmt", "json")],
        )
        .map_err(http_err)?;
        Ok(answer.genres)
    }

    fn acoustid(&self, key: &str, fingerprint: &str, duration: u32) -> Result<Vec<AcoustMatch>, String> {
        let duration = duration.to_string();
        // POST: a two-minute fingerprint is too long for a comfortable URL.
        let answer: AcoustAnswer = http::post_form_json(
            &http::ACOUSTID,
            ACOUSTID,
            &[
                ("client", key),
                ("format", "json"),
                ("meta", "recordings"),
                ("duration", &duration),
                ("fingerprint", fingerprint),
            ],
        )
        .or_else(|e| match e {
            // AcoustID answers a bad key or fingerprint with 400 and a JSON body.
            HttpError::Other(message) if message.starts_with("HTTP 400") => {
                let body = message.split_once(": ").map(|(_, b)| b).unwrap_or("");
                serde_json::from_str(body).map_err(|_| HttpError::Other(message))
            }
            other => Err(other),
        })
        .map_err(|e| format!("AcoustID: {e}"))?;
        acoust_matches(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_escape_quotes() {
        assert_eq!(search_query(" Roma ", Some("Amulet")), r#"recording:"Roma" AND artist:"Amulet""#);
        assert_eq!(search_query(r#"Say "hi" \o/"#, None), r#"recording:"Say \"hi\" \\o/""#);
        assert_eq!(search_query("X", Some("  ")), r#"recording:"X""#);
    }

    #[test]
    fn recordings_parse_from_musicbrainz_json() {
        let json = r#"{"recordings":[{"id":"r1","score":100,"title":"Roma","length":230000,
            "artist-credit":[{"name":"Amulet","joinphrase":" feat. ","artist":{"id":"a1","name":"Amulet"}},
                             {"name":"Mar Pujol","artist":{"id":"a2","name":"Mar Pujol"}}],
            "first-release-date":"2017-03-10",
            "releases":[{"id":"rel1","title":"Petit Viking","status":"Official","date":"2017-03-10",
                "release-group":{"id":"rg1","title":"Petit Viking","primary-type":"Album","secondary-types":[]}}]}]}"#;
        let answer: SearchAnswer = serde_json::from_str(json).unwrap();
        let r = &answer.recordings[0];
        assert_eq!(r.artist(), "Amulet feat. Mar Pujol");
        assert_eq!(r.length, Some(230_000));
        assert_eq!(r.releases[0].release_group.as_ref().unwrap().primary_type.as_deref(), Some("Album"));
    }

    #[test]
    fn acoustid_answers_parse() {
        let ok = r#"{"status":"ok","results":[{"id":"x","score":0.93,"recordings":[
            {"id":"r1","title":"Estrella","duration":279.8,"artists":[{"id":"a1","name":"Amulet"}]},{}]}]}"#;
        let matches = parse_acoustid(ok).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].recordings.len(), 1, "a recording without id is dropped");
        assert_eq!(matches[0].recordings[0].artist(), "Amulet");
        assert_eq!(matches[0].recordings[0].length, Some(279_800));
        let bad = r#"{"status":"error","error":{"code":4,"message":"invalid API key"}}"#;
        assert_eq!(parse_acoustid(bad).unwrap_err(), "AcoustID: invalid API key");
    }
}
