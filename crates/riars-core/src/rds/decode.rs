//! Test-only RDS decoder: an independent implementation used to check the encoder and
//! the MPX modulator. Nothing here is compiled outside tests.

use std::f64::consts::PI;

/// Offset words that name a block's position in a group.
const OFFSET_A: u16 = 0x0FC;
const OFFSET_B: u16 = 0x198;
const OFFSET_C: u16 = 0x168;
const OFFSET_CP: u16 = 0x350;
const OFFSET_D: u16 = 0x1B4;

/// g(x) = x^10 + x^8 + x^7 + x^5 + x^4 + x^3 + 1, with the x^10 term.
const POLY: u32 = 0x5B9;

/// Remainder of the 26-bit `block` divided by g(x); equals the offset word of a good block.
pub(crate) fn syndrome(block: u32) -> u16 {
    let mut rem = block & 0x03FF_FFFF;
    for bit in (10..26).rev() {
        if rem & (1 << bit) != 0 {
            rem ^= POLY << (bit - 10);
        }
    }
    (rem & 0x3FF) as u16
}

/// Find block sync in a bit stream (one bit per u8, 0 or 1, already differentially decoded)
/// and return every group whose four blocks all have the right syndromes in order
/// (A, B, C or C', D), as the four 16-bit information words.
pub(crate) fn groups_from_bits(bits: &[u8]) -> Vec<[u16; 4]> {
    let mut groups = Vec::new();
    let mut at = 0;
    while at + 104 <= bits.len() {
        let a = window(bits, at);
        if syndrome(a) == OFFSET_A {
            let b = window(bits, at + 26);
            let c = window(bits, at + 52);
            let d = window(bits, at + 78);
            let c_ok = syndrome(c) == OFFSET_C || syndrome(c) == OFFSET_CP;
            if syndrome(b) == OFFSET_B && c_ok && syndrome(d) == OFFSET_D {
                groups.push([
                    (a >> 10) as u16,
                    (b >> 10) as u16,
                    (c >> 10) as u16,
                    (d >> 10) as u16,
                ]);
                at += 104;
                continue;
            }
        }
        at += 1;
    }
    groups
}

/// The 26 bits starting at `start`, MSB first.
fn window(bits: &[u8], start: usize) -> u32 {
    let mut block = 0u32;
    for &bit in &bits[start..start + 26] {
        block = (block << 1) | (bit & 1) as u32;
    }
    block
}

/// What the groups say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Decoded {
    pub pi: Option<u16>,
    pub pty: Option<u8>,
    pub tp: Option<bool>,
    pub ta: Option<bool>,
    pub music: Option<bool>,
    /// Decoder identification bits d3..d0 seen in group 0A (index = segment address).
    pub di: [Option<bool>; 4],
    /// PS bytes as received (raw RDS character set), `None` until that pair is seen.
    pub ps: [Option<u8>; 8],
    /// RadioText bytes by position (raw), up to 64; the text ends at the first 0x0D.
    pub rt: [Option<u8>; 64],
    pub rt_ab: Option<bool>,
    /// RT+ tags from the last group 11A: (content type, start, length) — length is the
    /// length marker + 1.
    pub rt_plus: Option<[(u8, u8, u8); 2]>,
    /// AID announced for group 11A in a group 3A, if any.
    pub oda_11a: Option<u16>,
    pub groups: usize,
}

impl Default for Decoded {
    fn default() -> Self {
        Self {
            pi: None,
            pty: None,
            tp: None,
            ta: None,
            music: None,
            di: [None; 4],
            ps: [None; 8],
            rt: [None; 64],
            rt_ab: None,
            rt_plus: None,
            oda_11a: None,
            groups: 0,
        }
    }
}

impl Decoded {
    /// PS as text if all 8 bytes arrived (bytes as Latin-1 chars; only ASCII is checked in tests).
    pub fn ps_text(&self) -> Option<String> {
        let mut text = String::with_capacity(8);
        for byte in self.ps {
            text.push(char::from(byte?));
        }
        Some(text)
    }

    /// RadioText up to the first 0x0D (or 64 bytes), if every byte before it arrived.
    pub fn rt_text(&self) -> Option<String> {
        let end = self.rt.iter().position(|b| *b == Some(0x0D)).unwrap_or(self.rt.len());
        let mut text = String::with_capacity(end);
        for i in 0..end {
            text.push(char::from(self.rt[i]?));
        }
        Some(text)
    }
}

/// Read the fields of groups 0A, 2A, 3A and 11A (others are counted and skipped).
pub(crate) fn parse(groups: &[[u16; 4]]) -> Decoded {
    let mut decoded = Decoded::default();
    for &group in groups {
        decoded.groups += 1;
        let [pi, block2, block3, block4] = group;
        decoded.pi = Some(pi);
        let ty = (block2 >> 12) & 0x0F;
        let version = (block2 >> 11) & 1;
        decoded.tp = Some((block2 >> 10) & 1 != 0);
        decoded.pty = Some(((block2 >> 5) & 0x1F) as u8);
        let low5 = (block2 & 0x1F) as u8;
        if version != 0 {
            continue;
        }
        match ty {
            0 => {
                let segment = (low5 & 0x03) as usize;
                decoded.ta = Some((low5 >> 4) & 1 != 0);
                decoded.music = Some((low5 >> 3) & 1 != 0);
                decoded.di[segment] = Some((low5 >> 2) & 1 != 0);
                decoded.ps[2 * segment] = Some((block4 >> 8) as u8);
                decoded.ps[2 * segment + 1] = Some(block4 as u8);
            }
            2 => {
                let segment = (low5 & 0x0F) as usize;
                decoded.rt_ab = Some((low5 >> 4) & 1 != 0);
                decoded.rt[4 * segment] = Some((block3 >> 8) as u8);
                decoded.rt[4 * segment + 1] = Some(block3 as u8);
                decoded.rt[4 * segment + 2] = Some((block4 >> 8) as u8);
                decoded.rt[4 * segment + 3] = Some(block4 as u8);
            }
            3 => {
                if low5 == 0b1_0110 {
                    decoded.oda_11a = Some(block4);
                }
            }
            11 => {
                let ct1 = ((low5 & 0x07) << 3) | ((block3 >> 13) & 0x07) as u8;
                let start1 = ((block3 >> 7) & 0x3F) as u8;
                let len1 = ((block3 >> 1) & 0x3F) as u8 + 1;
                let ct2 = (((block3 & 0x01) << 5) as u8) | ((block4 >> 11) & 0x1F) as u8;
                let start2 = ((block4 >> 5) & 0x3F) as u8;
                let len2 = (block4 & 0x1F) as u8 + 1;
                decoded.rt_plus = Some([(ct1, start1, len1), (ct2, start2, len2)]);
            }
            _ => {}
        }
    }
    decoded
}

/// The data bits (after biphase and differential decoding) of the RDS signal inside `mpx`,
/// an MPX baseband sampled at `sample_rate` Hz (at least 128 kHz).
pub(crate) fn demodulate(mpx: &[f32], sample_rate: u32) -> Vec<u8> {
    let n = mpx.len();
    if n == 0 || sample_rate == 0 {
        return Vec::new();
    }
    let fs = sample_rate as f64;

    // Pilot phase: for x = A sin(wt + phi0), the cos correlation gives sin(phi0) and the
    // sin correlation gives cos(phi0).
    let mut cos_part = 0.0f64;
    let mut sin_part = 0.0f64;
    for (i, &x) in mpx.iter().enumerate() {
        let w = 2.0 * PI * 19000.0 * i as f64 / fs;
        let x = x as f64;
        cos_part += x * w.cos();
        sin_part += x * w.sin();
    }
    let phi0 = cos_part.atan2(sin_part);

    // Mix down with the 57 kHz carrier locked to the pilot; try both quadratures and keep
    // the one carrying the data.
    let lowpass_len = ((fs / 10_000.0).round() as usize).max(1);
    let mut baseband = Vec::new();
    let mut best_energy = -1.0f64;
    for carrier_shift in [0.0f64, PI / 2.0] {
        let mut mixed = vec![0.0f64; n];
        for i in 0..n {
            let w = 2.0 * PI * 19000.0 * i as f64 / fs;
            let carrier = (3.0 * (w + phi0) + carrier_shift).sin();
            mixed[i] = mpx[i] as f64 * carrier;
        }
        let mut filtered = mixed;
        for _ in 0..4 {
            filtered = boxcar(&filtered, lowpass_len);
        }
        let energy: f64 = filtered.iter().map(|v| v * v).sum();
        if energy > best_energy {
            best_energy = energy;
            baseband = filtered;
        }
    }

    // Bit timing: try 64 offsets over one bit and keep the one where the two half-bit
    // integrals differ most.
    let spb = fs / 1187.5;
    let half = spb / 2.0;
    let mut best_offset = 0.0f64;
    let mut best_score = -1.0f64;
    for candidate in 0..64 {
        let offset = candidate as f64 * spb / 64.0;
        let bits = ((n as f64 - offset) / spb).floor() as usize;
        if bits == 0 {
            continue;
        }
        let mut score = 0.0f64;
        for k in 0..bits {
            let start = offset + k as f64 * spb;
            let first = sum_between(&baseband, start, start + half);
            let second = sum_between(&baseband, start + half, start + spb);
            score += (first - second).abs();
        }
        let mean = score / bits as f64;
        if mean > best_score {
            best_score = mean;
            best_offset = offset;
        }
    }

    let bits = ((n as f64 - best_offset) / spb).floor() as usize;
    let mut symbols = Vec::with_capacity(bits);
    for k in 0..bits {
        let start = best_offset + k as f64 * spb;
        let first = sum_between(&baseband, start, start + half);
        let second = sum_between(&baseband, start + half, start + spb);
        symbols.push((first - second > 0.0) as u8);
    }
    let mut data = Vec::with_capacity(symbols.len().saturating_sub(1));
    for k in 1..symbols.len() {
        data.push(symbols[k] ^ symbols[k - 1]);
    }
    data
}

/// A moving average with a window of `len` samples; the first samples average what exists.
fn boxcar(x: &[f64], len: usize) -> Vec<f64> {
    let mut out = vec![0.0f64; x.len()];
    let mut sum = 0.0f64;
    for i in 0..x.len() {
        sum += x[i];
        if i >= len {
            sum -= x[i - len];
        }
        let count = if i < len { i + 1 } else { len };
        out[i] = sum / count as f64;
    }
    out
}

/// Sum of the samples whose index is in `[from, to)`.
fn sum_between(x: &[f64], from: f64, to: f64) -> f64 {
    let start = from.max(0.0).floor() as usize;
    let end = (to.max(0.0).floor() as usize).min(x.len());
    if start >= end {
        0.0
    } else {
        x[start..end].iter().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The block built from an information word and its offset word.
    fn build_block(info: u16, offset: u16) -> u32 {
        let shifted = (info as u32) << 10;
        shifted | (syndrome(shifted) ^ offset) as u32
    }

    /// One group as bits, MSB first, blocks A, B, C, D.
    fn group_bits(infos: [u16; 4]) -> Vec<u8> {
        let offsets = [OFFSET_A, OFFSET_B, OFFSET_C, OFFSET_D];
        let mut bits = Vec::with_capacity(104);
        for (info, offset) in infos.into_iter().zip(offsets) {
            let block = build_block(info, offset);
            for i in (0..26).rev() {
                bits.push(((block >> i) & 1) as u8);
            }
        }
        bits
    }

    fn xorshift(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    #[test]
    fn syndrome_matches_the_vectors() {
        let vectors = [
            (0xE2A1u16, OFFSET_A, 0x038A_876Bu32),
            (0x0408, OFFSET_B, 0x0010_21EB),
            (0xE0CD, OFFSET_C, 0x0383_35E9),
            (0x5245, OFFSET_D, 0x0149_1733),
        ];
        for &(info, offset, block) in &vectors {
            assert_eq!(build_block(info, offset), block);
            assert_eq!(syndrome(block), offset);
            for bit in 0..26 {
                assert_ne!(syndrome(block ^ (1 << bit)), offset, "bit {bit}");
            }
        }
    }

    #[test]
    fn groups_are_found_in_a_bit_stream() {
        let first = [0xE2A1, 0x0408, 0xE0CD, 0x5245];
        let second = [0x1234, 0x5678, 0x9ABC, 0xDEF0];

        let mut seed = 0x1234_5678u32;
        let mut bits: Vec<u8> = (0..37).map(|_| (xorshift(&mut seed) & 1) as u8).collect();
        bits.extend(group_bits(first));
        bits.extend([1, 0, 1, 1, 0]);
        bits.extend(group_bits(second));
        assert_eq!(groups_from_bits(&bits), vec![first, second]);

        let mut seed = 0x1234_5678u32;
        let mut broken: Vec<u8> = (0..37).map(|_| (xorshift(&mut seed) & 1) as u8).collect();
        let mut group = group_bits(first);
        group[5] ^= 1;
        broken.extend(group);
        assert!(groups_from_bits(&broken).is_empty());
    }

    #[test]
    fn parse_reads_the_group_fields() {
        let pi = 0x1234;
        let ps = b"RENOU   ";
        let mut groups = Vec::new();
        for segment in 0..4usize {
            let di = (segment == 3) as u16;
            let low5 = (1 << 3) | (di << 2) | segment as u16;
            let block2 = (1 << 10) | (10 << 5) | low5;
            let pair = ((ps[2 * segment] as u16) << 8) | ps[2 * segment + 1] as u16;
            groups.push([pi, block2, 0, pair]);
        }

        let rt = [b'H', b'o', b'l', b'a', 0x0D, b' ', b' ', b' ', b' ', b' ', b' ', b' '];
        for segment in 0..2usize {
            let low5 = (1 << 4) | segment as u16;
            let block2 = (2 << 12) | (1 << 10) | (10 << 5) | low5;
            let block3 = ((rt[4 * segment] as u16) << 8) | rt[4 * segment + 1] as u16;
            let block4 = ((rt[4 * segment + 2] as u16) << 8) | rt[4 * segment + 3] as u16;
            groups.push([pi, block2, block3, block4]);
        }

        groups.push([pi, (3 << 12) | (1 << 10) | (10 << 5) | 0b1_0110, 0, 0x4BD7]);

        let low5 = 0;
        let block3 = (4u16 << 13) | ((7u16 - 1) << 1);
        let block4 = (1u16 << 11) | (10u16 << 5) | (9u16 - 1);
        groups.push([pi, (11 << 12) | (1 << 10) | (10 << 5) | low5, block3, block4]);

        let decoded = parse(&groups);
        assert_eq!(decoded.pi, Some(pi));
        assert_eq!(decoded.tp, Some(true));
        assert_eq!(decoded.pty, Some(10));
        assert_eq!(decoded.ta, Some(false));
        assert_eq!(decoded.music, Some(true));
        assert_eq!(decoded.di, [Some(false), Some(false), Some(false), Some(true)]);
        assert_eq!(decoded.ps_text().as_deref(), Some("RENOU   "));
        assert_eq!(decoded.rt_ab, Some(true));
        assert_eq!(decoded.rt_text().as_deref(), Some("Hola"));
        assert_eq!(decoded.oda_11a, Some(0x4BD7));
        assert_eq!(decoded.rt_plus, Some([(4, 0, 7), (1, 10, 9)]));
        assert_eq!(decoded.groups, 8);
    }

    /// An MPX with the given data bits: differential, biphase, 57 kHz carrier, pilot and a
    /// 1 kHz tone. `delay_s` delays the start of the data.
    fn make_mpx(bits: &[u8], sample_rate: u32, delay_s: f64) -> Vec<f32> {
        let fs = sample_rate as f64;
        let spb = fs / 1187.5;

        let mut symbols = Vec::with_capacity(bits.len());
        let mut previous = 0u8;
        for &bit in bits {
            previous ^= bit;
            symbols.push(previous);
        }

        let delay = (delay_s * fs).round() as usize;
        let data_len = (symbols.len() as f64 * spb).ceil() as usize;
        let total = delay + data_len + spb.ceil() as usize + 4;
        let mut mpx = vec![0.0f32; total];
        for (i, sample) in mpx.iter_mut().enumerate() {
            let t = i as f64 / fs;
            let theta = 2.0 * PI * 19000.0 * t;
            let mut x = 0.09 * theta.sin() + 0.02 * (2.0 * PI * 1000.0 * t).sin();
            if i >= delay {
                let position = (i - delay) as f64 / spb;
                let k = position.floor() as usize;
                if k < symbols.len() {
                    let first_half = (position - k as f64) < 0.5;
                    let level = if (symbols[k] == 1) == first_half { 1.0 } else { -1.0 };
                    x += level * (3.0 * theta).sin();
                }
            }
            *sample = x as f32;
        }
        mpx
    }

    #[test]
    fn demodulate_recovers_the_bits() {
        let pi = 0xE2A1;
        let ps = b"RENOU   ";
        let mut groups = Vec::new();
        for segment in 0..4usize {
            let low5 = (1 << 3) | segment as u16;
            let block2 = (1 << 10) | (10 << 5) | low5;
            let pair = ((ps[2 * segment] as u16) << 8) | ps[2 * segment + 1] as u16;
            groups.push([pi, block2, 0, pair]);
        }

        let mut seed = 0xDEAD_BEEFu32;
        let mut data: Vec<u8> = (0..24).map(|_| (xorshift(&mut seed) & 1) as u8).collect();
        for _ in 0..3 {
            for group in &groups {
                data.extend(group_bits(*group));
            }
        }

        for delay_ms in [0.0f64, 0.3] {
            let mpx = make_mpx(&data, 192_000, delay_ms / 1000.0);
            let decoded = parse(&groups_from_bits(&demodulate(&mpx, 192_000)));
            assert_eq!(decoded.pi, Some(pi), "delay {delay_ms} ms");
            assert_eq!(decoded.ps_text().as_deref(), Some("RENOU   "), "delay {delay_ms} ms");
        }
    }
}
