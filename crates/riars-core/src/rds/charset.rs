//! The RDS character set (EN 50067 Annex E, code table G0) and the conversions an
//! encoder may want: one byte per character in every case, so a text keeps its length.

/// Code table G0, indexed by byte. `'\0'` marks the unused positions.
const G0: [char; 256] = {
    let mut table = ['\0'; 256];
    let rows: [(usize, &str); 14] = [
        (0x20, " !\"#¤%&'()*+,-./"),
        (0x30, "0123456789:;<=>?"),
        (0x40, "@ABCDEFGHIJKLMNO"),
        (0x50, "PQRSTUVWXYZ[\\]―_"),
        (0x60, "‖abcdefghijklmno"),
        (0x70, "pqrstuvwxyz{|}¯"),
        (0x80, "áàéèíìóòúùÑÇŞß¡Ĳ"),
        (0x90, "âäêëîïôöûüñçşğıĳ"),
        (0xA0, "ªα©‰Ğěňőπ€£$←↑→↓"),
        (0xB0, "º¹²³±İńűµ¿÷°¼½¾§"),
        (0xC0, "ÁÀÉÈÍÌÓÒÚÙŘČŠŽĐĿ"),
        (0xD0, "ÂÄÊËÎÏÔÖÛÜřčšžđŀ"),
        (0xE0, "ÃÅÆŒŷÝÕØÞŊŔĆŚŹŦð"),
        (0xF0, "ãåæœŵýõøþŋŕćśźŧ"),
    ];
    let mut r = 0;
    while r < rows.len() {
        let (start, text) = rows[r];
        let bytes = text.as_bytes();
        let (mut i, mut col) = (0, 0);
        // Decode UTF-8 by hand: this runs at compile time.
        while i < bytes.len() {
            let b = bytes[i] as u32;
            let (code, len) = if b < 0x80 {
                (b, 1)
            } else if b < 0xE0 {
                (((b & 0x1F) << 6) | (bytes[i + 1] as u32 & 0x3F), 2)
            } else {
                (((b & 0x0F) << 12) | ((bytes[i + 1] as u32 & 0x3F) << 6) | (bytes[i + 2] as u32 & 0x3F), 3)
            };
            table[start + col] = match char::from_u32(code) {
                Some(c) => c,
                None => '\0',
            };
            i += len;
            col += 1;
        }
        r += 1;
    }
    table
};

/// What a byte of the RDS character set shows, or `None` for an unused position.
pub fn decode(byte: u8) -> Option<char> {
    Some(G0[byte as usize]).filter(|c| *c != '\0')
}

/// The RDS byte for `c`: its own position when the table has it, the closest look-alike
/// otherwise (curly quotes, dashes), `?` when there is none.
pub fn encode_char(c: char) -> u8 {
    let c = match c {
        '‘' | '’' | '´' | '`' => '\'',
        '“' | '”' | '«' | '»' => '"',
        '–' | '—' => '-',
        '·' | '…' => '.',
        '\u{a0}' | '\t' | '\n' | '\r' => ' ',
        other => other,
    };
    G0.iter().position(|&g| g == c && c != '\0').map_or(b'?', |i| i as u8)
}

/// `text` in the RDS character set, one byte per character.
pub fn encode(text: &str) -> Vec<u8> {
    text.chars().map(encode_char).collect()
}

/// `text` with the accents taken off, for encoders that only take plain ASCII.
pub fn to_ascii(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii() && !c.is_ascii_control() {
                return c;
            }
            match c {
                'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ª' => 'a',
                'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
                'é' | 'è' | 'ê' | 'ë' | 'ě' => 'e',
                'É' | 'È' | 'Ê' | 'Ë' => 'E',
                'í' | 'ì' | 'î' | 'ï' | 'ı' => 'i',
                'Í' | 'Ì' | 'Î' | 'Ï' | 'İ' => 'I',
                'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ő' | 'º' => 'o',
                'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ø' => 'O',
                'ú' | 'ù' | 'û' | 'ü' | 'ű' => 'u',
                'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
                'ñ' | 'ń' | 'ň' => 'n',
                'Ñ' => 'N',
                'ç' | 'ć' | 'č' => 'c',
                'Ç' | 'Ć' | 'Č' => 'C',
                'ş' | 'ś' | 'š' => 's',
                'Ş' | 'Ś' | 'Š' => 'S',
                'ß' => 's',
                'ž' | 'ź' => 'z',
                'Ž' | 'Ź' => 'Z',
                'ý' => 'y',
                'Ý' => 'Y',
                'ŀ' => 'l',
                'Ŀ' => 'L',
                'æ' => 'a',
                'Æ' => 'A',
                'œ' => 'o',
                'Œ' => 'O',
                '‘' | '’' | '´' => '\'',
                '“' | '”' | '«' | '»' => '"',
                '–' | '—' => '-',
                '·' | '…' => '.',
                '¡' => '!',
                '¿' => '?',
                '€' => 'E',
                '\u{a0}' | '\t' | '\n' | '\r' => ' ',
                _ => '?',
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_letters_keep_their_codes() {
        assert_eq!(encode("RIARS 1 a-z"), b"RIARS 1 a-z".to_vec());
    }

    #[test]
    fn accents_use_the_rds_table() {
        assert_eq!(encode("ñÑçáé"), vec![0x9A, 0x8A, 0x9B, 0x80, 0x82]);
        assert_eq!(encode("€$"), vec![0xA9, 0xAB]);
        assert_eq!(decode(0x9A), Some('ñ'));
        assert_eq!(decode(0x24), Some('¤'));
        assert_eq!(decode(0x7F), None);
    }

    #[test]
    fn every_used_code_round_trips() {
        for byte in 0x20..=0xFEu8 {
            if let Some(c) = decode(byte) {
                assert_eq!(encode_char(c), byte, "{c:?}");
            }
        }
    }

    #[test]
    fn unknown_characters_keep_the_length() {
        assert_eq!(encode("“Hi”—日本"), b"\"Hi\"-??".to_vec());
        assert_eq!(to_ascii("Canción · Ñandú"), "Cancion . Nandu");
    }
}
