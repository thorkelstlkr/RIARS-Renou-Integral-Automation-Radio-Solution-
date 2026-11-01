//! EBU UECP (SPB 490): the frames that set PI, PS, PTY, TP, MS and RadioText on an
//! encoder. A frame is `STA ADD SQC MFL MF CRC STP`; between STA and STP the bytes
//! FD, FE and FF are escaped as FD 00, FD 01 and FD 02.

const STA: u8 = 0xFE;
const STP: u8 = 0xFF;

const MEC_PI: u8 = 0x01;
const MEC_PS: u8 = 0x02;
const MEC_TA_TP: u8 = 0x03;
const MEC_MS: u8 = 0x05;
const MEC_PTY: u8 = 0x07;
const MEC_RT: u8 = 0x0A;

/// Data set 0 (the one on air) and programme service 0 (the main one).
const DSN: u8 = 0;
const PSN: u8 = 0;

/// CRC-CCITT (x^16 + x^12 + x^5 + 1) from FFFF, inverted, over ADD to the end of MF.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    !crc
}

/// One frame carrying `elements` (several message elements back to back, at most 255
/// bytes) to `site` (0–1023) and `encoder` (0–63); 0 and 0 reach every encoder.
pub fn frame(site: u16, encoder: u8, elements: &[u8]) -> Vec<u8> {
    let address = (site & 0x3FF) << 6 | (encoder as u16 & 0x3F);
    let mut body = vec![(address >> 8) as u8, address as u8, 0, elements.len().min(255) as u8];
    body.extend_from_slice(&elements[..elements.len().min(255)]);
    let crc = crc16(&body);
    body.extend_from_slice(&crc.to_be_bytes());
    let mut out = Vec::with_capacity(body.len() + 8);
    out.push(STA);
    for byte in body {
        match byte {
            0xFD..=0xFF => out.extend_from_slice(&[0xFD, byte - 0xFD]),
            _ => out.push(byte),
        }
    }
    out.push(STP);
    out
}

pub fn pi(code: u16) -> Vec<u8> {
    let [hi, lo] = code.to_be_bytes();
    vec![MEC_PI, DSN, PSN, hi, lo]
}

/// `ps` already in the RDS character set; padded or cut to 8 bytes.
pub fn ps(ps: &[u8]) -> Vec<u8> {
    let mut out = vec![MEC_PS, DSN, PSN];
    out.extend((0..8).map(|i| ps.get(i).copied().unwrap_or(b' ')));
    out
}

pub fn ta_tp(tp: bool, ta: bool) -> Vec<u8> {
    vec![MEC_TA_TP, DSN, PSN, (tp as u8) << 1 | ta as u8]
}

pub fn ms(music: bool) -> Vec<u8> {
    vec![MEC_MS, DSN, PSN, music as u8]
}

pub fn pty(pty: u8) -> Vec<u8> {
    vec![MEC_PTY, DSN, PSN, pty & 0x1F]
}

/// RadioText, already in the RDS character set (64 bytes at most). It replaces the
/// encoder's buffer and repeats until the next one; `toggle` flips the A/B flag, which
/// tells radios to clear the old text (set it when the text changes).
pub fn rt(text: &[u8], toggle: bool) -> Vec<u8> {
    let text = &text[..text.len().min(64)];
    // Bits 6-5: 00 = replace the buffer; bits 4-1: 0 = repeat without end; bit 0: A/B.
    let config = toggle as u8;
    let mut out = vec![MEC_RT, DSN, PSN, text.len() as u8 + 1, config];
    out.extend_from_slice(text);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_ccitt_check_value() {
        // CRC-16/CCITT-FALSE of "123456789" is 29B1; UECP sends it inverted.
        assert_eq!(crc16(b"123456789"), !0x29B1);
    }

    #[test]
    fn frame_layout() {
        let out = frame(0, 0, &pi(0xC201));
        let body = [0x00, 0x00, 0x00, 0x05, MEC_PI, 0, 0, 0xC2, 0x01];
        let crc = crc16(&body).to_be_bytes();
        let mut expected = vec![STA];
        expected.extend_from_slice(&body);
        for byte in crc {
            match byte {
                0xFD..=0xFF => expected.extend_from_slice(&[0xFD, byte - 0xFD]),
                _ => expected.push(byte),
            }
        }
        expected.push(STP);
        assert_eq!(out, expected);
    }

    #[test]
    fn reserved_bytes_are_escaped() {
        let out = frame(0, 0, &[0xFD, 0xFE, 0xFF]);
        assert_eq!(&out[5..11], &[0xFD, 0x00, 0xFD, 0x01, 0xFD, 0x02]);
        assert_eq!(out.iter().filter(|&&b| b == STA).count(), 1);
        assert_eq!(out.iter().filter(|&&b| b == STP).count(), 1);
        assert_eq!(*out.last().unwrap(), STP);
    }

    #[test]
    fn addresses_pack_into_two_bytes() {
        let out = frame(1023, 63, &[]);
        assert_eq!(&out[1..5], &[0xFD, 0x02, 0xFD, 0x02], "FFFF, escaped");
        let out = frame(5, 3, &[]);
        assert_eq!(&out[1..3], &[0x01, 0x43]);
    }

    #[test]
    fn message_elements() {
        assert_eq!(ps(b"RIARS"), vec![MEC_PS, 0, 0, b'R', b'I', b'A', b'R', b'S', b' ', b' ', b' ']);
        assert_eq!(ta_tp(true, false), vec![MEC_TA_TP, 0, 0, 0b10]);
        assert_eq!(pty(10), vec![MEC_PTY, 0, 0, 10]);
        assert_eq!(rt(b"Hola", true), vec![MEC_RT, 0, 0, 5, 1, b'H', b'o', b'l', b'a']);
        assert_eq!(rt(&[b'x'; 80], false).len(), 5 + 64);
    }
}
