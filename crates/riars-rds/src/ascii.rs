//! Text commands from a template, for encoders with their own command set.
//!
//! Placeholders: `{ps}` (8 characters, padded), `{ps_trim}`, `{rt}`, `{pi}` (4 hex
//! digits), `{pty}`, `{tp}` and `{ms}` (0 or 1), and for RT+ `{rtp_title_start}`,
//! `{rtp_title_len}`, `{rtp_artist_start}`, `{rtp_artist_len}` (0 when there is no tag).
//! Escapes: `\r`, `\n`, `\t` and `\\`.

use riars_core::rds::{charset, RdsCharset, RdsConfig, RdsTexts, RTPLUS_ARTIST, RTPLUS_TITLE};

pub fn render(template: &str, charset: RdsCharset, config: &RdsConfig, texts: &RdsTexts) -> Vec<u8> {
    let tag = |content_type: u8| texts.rt_plus.iter().find(|t| t.content_type == content_type);
    let start = |content_type| tag(content_type).map_or(0, |t| t.start).to_string();
    let len = |content_type| tag(content_type).map_or(0, |t| t.len).to_string();
    let values: [(&str, String); 11] = [
        ("{ps}", texts.ps.clone()),
        ("{ps_trim}", texts.ps.trim().to_string()),
        ("{rt}", texts.rt.clone()),
        ("{pi}", config.pi.to_ascii_uppercase()),
        ("{pty}", config.pty.to_string()),
        ("{tp}", (config.tp as u8).to_string()),
        ("{ms}", (config.music as u8).to_string()),
        ("{rtp_title_start}", start(RTPLUS_TITLE)),
        ("{rtp_title_len}", len(RTPLUS_TITLE)),
        ("{rtp_artist_start}", start(RTPLUS_ARTIST)),
        ("{rtp_artist_len}", len(RTPLUS_ARTIST)),
    ];
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(c) = rest.chars().next() {
        if let Some((key, value)) = values.iter().find(|(key, _)| rest.starts_with(key)) {
            out.extend(encode(value, charset));
            rest = &rest[key.len()..];
            continue;
        }
        if c == '\\' {
            let escaped = match rest[1..].chars().next() {
                Some('r') => Some(b'\r'),
                Some('n') => Some(b'\n'),
                Some('t') => Some(b'\t'),
                Some('\\') => Some(b'\\'),
                _ => None,
            };
            if let Some(byte) = escaped {
                out.push(byte);
                rest = &rest[2..];
                continue;
            }
        }
        out.extend(encode(&rest[..c.len_utf8()], charset));
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn encode(text: &str, charset: RdsCharset) -> Vec<u8> {
    match charset {
        RdsCharset::Rds => charset::encode(text),
        RdsCharset::Ascii => charset::to_ascii(text).into_bytes(),
        RdsCharset::Utf8 => text.as_bytes().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use riars_core::rds::{RtPlusTag, DEFAULT_ASCII_TEMPLATE};

    use super::*;

    fn texts() -> RdsTexts {
        RdsTexts {
            ps: "RENOU   ".into(),
            rt: "Rosalía - Malamente".into(),
            rt_plus: vec![
                RtPlusTag { content_type: RTPLUS_ARTIST, start: 0, len: 7 },
                RtPlusTag { content_type: RTPLUS_TITLE, start: 10, len: 9 },
            ],
        }
    }

    #[test]
    fn default_template() {
        let config = RdsConfig::default();
        let out = render(DEFAULT_ASCII_TEMPLATE, RdsCharset::Utf8, &config, &texts());
        assert_eq!(String::from_utf8(out).unwrap(), "PS=RENOU   \r\nRT=Rosalía - Malamente\r\n");
    }

    #[test]
    fn character_sets() {
        let config = RdsConfig::default();
        assert_eq!(render("{rt}", RdsCharset::Ascii, &config, &texts()), b"Rosalia - Malamente".to_vec());
        let rds = render("{rt}", RdsCharset::Rds, &config, &texts());
        assert_eq!(rds.len(), 19);
        assert_eq!(rds[5], 0x84, "í in the RDS table");
    }

    #[test]
    fn every_placeholder_and_escape() {
        let config = RdsConfig { pi: "c201".into(), pty: 10, tp: true, ..RdsConfig::default() };
        let template = "{pi};{pty};{tp};{ms};[{ps_trim}];{rtp_artist_start},{rtp_artist_len},{rtp_title_start},{rtp_title_len}\\t\\\\{unknown}\\x";
        let out = String::from_utf8(render(template, RdsCharset::Utf8, &config, &texts())).unwrap();
        assert_eq!(out, "C201;10;1;1;[RENOU];0,7,10,9\t\\{unknown}\\x");
    }
}
