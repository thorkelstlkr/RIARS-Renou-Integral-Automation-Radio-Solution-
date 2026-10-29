//! HTTP with the manners each service asks for: a User-Agent that says who we are, and
//! no more requests than its limit, counted across the whole process.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;

/// MusicBrainz asks for an application name, version and contact.
pub const USER_AGENT: &str = concat!(
    "RIARS/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/thorkelstlkr/RIARS-Renou-Integral-Automation-Radio-Solution- )"
);

/// Largest JSON answer read (a search of 10 recordings is about 100 KB).
const JSON_LIMIT: u64 = 8 * 1024 * 1024;
/// Largest cover image downloaded.
pub const IMAGE_LIMIT: u64 = 10 * 1024 * 1024;

/// Spaces requests to one service at least `gap` apart, whichever thread sends them.
pub struct Gate {
    gap: Duration,
    next: Mutex<Option<Instant>>,
}

/// MusicBrainz: one request per second per IP; a little more, to stay clear of it.
pub static MUSICBRAINZ: Gate = Gate::new(Duration::from_millis(1100));
/// AcoustID: three requests per second.
pub static ACOUSTID: Gate = Gate::new(Duration::from_millis(340));
/// The Cover Art Archive states no limit; be gentle anyway.
pub static COVER_ART: Gate = Gate::new(Duration::from_millis(250));

impl Gate {
    pub const fn new(gap: Duration) -> Self {
        Self { gap, next: Mutex::new(None) }
    }

    /// Wait for this request's turn. The slot is taken under the lock and the wait
    /// happens outside it, so several threads queue up in order.
    pub fn wait(&self) {
        let now = Instant::now();
        let at = {
            let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
            let at = next.map_or(now, |n| n.max(now));
            *next = Some(at + self.gap);
            at
        };
        if at > now {
            std::thread::sleep(at - now);
        }
    }

    /// After a 503: push the next slot back.
    fn back_off(&self, by: Duration) {
        let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
        *next = Some(Instant::now() + by);
    }
}

#[derive(Debug, PartialEq)]
pub enum HttpError {
    NotFound,
    Other(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::NotFound => write!(f, "not found"),
            HttpError::Other(message) => write!(f, "{message}"),
        }
    }
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into()
    })
}

/// Tries, with the pause before each retry when the service is busy (503) or unreachable.
const RETRIES: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(5)];

/// GET `url` with `query` and read the JSON answer.
pub fn get_json<T: DeserializeOwned>(gate: &Gate, url: &str, query: &[(&str, &str)]) -> Result<T, HttpError> {
    send(gate, || {
        let mut request = agent().get(url).header("Accept", "application/json");
        for (key, value) in query {
            request = request.query(*key, *value);
        }
        request.call()
    })
    .and_then(|mut response| {
        response
            .body_mut()
            .with_config()
            .limit(JSON_LIMIT)
            .read_json()
            .map_err(|e| HttpError::Other(format!("Bad answer from {url}: {e}")))
    })
}

/// POST `form` to `url` and read the JSON answer.
pub fn post_form_json<T: DeserializeOwned>(gate: &Gate, url: &str, form: &[(&str, &str)]) -> Result<T, HttpError> {
    send(gate, || agent().post(url).send_form(form.iter().copied()))
        .and_then(|mut response| {
            response
                .body_mut()
                .with_config()
                .limit(JSON_LIMIT)
                .read_json()
                .map_err(|e| HttpError::Other(format!("Bad answer from {url}: {e}")))
        })
}

/// GET an image: its bytes and content type.
pub fn get_image(gate: &Gate, url: &str) -> Result<(Vec<u8>, String), HttpError> {
    let mut response = send(gate, || agent().get(url).call())?;
    let kind = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = response
        .body_mut()
        .with_config()
        .limit(IMAGE_LIMIT)
        .read_to_vec()
        .map_err(|e| HttpError::Other(format!("Cannot download {url}: {e}")))?;
    Ok((bytes, kind))
}

fn send(
    gate: &Gate,
    call: impl Fn() -> Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<ureq::http::Response<ureq::Body>, HttpError> {
    let mut attempt = 0;
    loop {
        gate.wait();
        let retry = match call() {
            Ok(response) => match response.status().as_u16() {
                200..=299 => return Ok(response),
                404 => return Err(HttpError::NotFound),
                503 | 429 => format!("busy ({})", response.status()),
                status => {
                    let mut response = response;
                    let body = response.body_mut().read_to_string().unwrap_or_default();
                    return Err(HttpError::Other(format!("HTTP {status}: {}", body.chars().take(300).collect::<String>())));
                }
            },
            Err(e) => e.to_string(),
        };
        let Some(pause) = RETRIES.get(attempt) else {
            return Err(HttpError::Other(format!("Service unavailable: {retry}")));
        };
        log::debug!("Metadata lookup: {retry}, retrying in {pause:?}");
        gate.back_off(*pause);
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_spaces_requests_across_threads() {
        static GATE: Gate = Gate::new(Duration::from_millis(60));
        let start = Instant::now();
        let handles: Vec<_> = (0..4).map(|_| std::thread::spawn(|| GATE.wait())).collect();
        for handle in handles {
            handle.join().unwrap();
        }
        // Four requests: the first at once, the others 60 ms apart.
        assert!(start.elapsed() >= Duration::from_millis(175), "{:?}", start.elapsed());
    }

    #[test]
    fn user_agent_names_the_app_and_a_contact() {
        assert!(USER_AGENT.starts_with("RIARS/"));
        assert!(USER_AGENT.contains("https://github.com/"));
    }
}
