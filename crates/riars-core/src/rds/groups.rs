//! RDS groups: the station's data turned into the 26-bit blocks an encoder sends.
//! The groups rotate in a fixed order (PS, RadioText, and the RT+ announcements), and
//! `next_group` runs on the audio thread, so it does not allocate.

/// Offset words added to the checkword, by block position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offset {
    A,
    B,
    C,
    Cp,
    D,
}

impl Offset {
    /// The 10-bit offset word: A 0x0FC, B 0x198, C 0x168, C' 0x350, D 0x1B4.
    pub const fn word(self) -> u16 {
        match self {
            Offset::A => 0x0FC,
            Offset::B => 0x198,
            Offset::C => 0x168,
            Offset::Cp => 0x350,
            Offset::D => 0x1B4,
        }
    }
}

/// The 10-bit checkword of a 16-bit information word, before the offset: the remainder
/// of info(x)·x^10 divided by g(x) = x^10+x^8+x^7+x^5+x^4+x^3+1 (0x5B9).
pub const fn checkword(info: u16) -> u16 {
    let mut rem = (info as u32) << 10;
    let poly = 0x5B9u32;
    let mut bit = 25i32;
    while bit >= 10 {
        if rem & (1u32 << bit) != 0 {
            rem ^= poly << (bit - 10);
        }
        bit -= 1;
    }
    (rem & 0x3FF) as u16
}

/// One 26-bit block in the low bits of a u32: info in bits 25..10, checkword XOR offset
/// in bits 9..0. Sent most significant bit first.
pub const fn block(info: u16, offset: Offset) -> u32 {
    ((info as u32) << 10) | (((checkword(info) ^ offset.word()) as u32) & 0x3FF)
}

/// What the groups carry. Copy and fixed-size: it is sent to the audio thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RdsFrame {
    pub pi: u16,
    /// 0..=31; higher values are masked to 5 bits.
    pub pty: u8,
    pub tp: bool,
    pub ta: bool,
    /// Music/Speech flag: true = music.
    pub music: bool,
    /// Decoder identification "stereo" bit (sent in segment 3 of group 0A).
    pub stereo: bool,
    /// PS in the RDS character set, exactly 8 bytes.
    pub ps: [u8; 8],
    /// RadioText in the RDS character set; only the first `rt_len` bytes count.
    pub rt: [u8; 64],
    /// 0..=64; higher values are treated as 64.
    pub rt_len: u8,
    /// RadioText A/B flag. The caller flips it when the text changes.
    pub ab: bool,
    /// RT+ tags: (content type 0..=63, start 0..=63, length 1..=64). `None` sends no RT+.
    pub rt_plus: Option<[(u8, u8, u8); 2]>,
}

impl Default for RdsFrame {
    fn default() -> Self {
        Self {
            pi: 0,
            pty: 0,
            tp: false,
            ta: false,
            music: true,
            stereo: false,
            ps: [b' '; 8],
            rt: [b' '; 64],
            rt_len: 0,
            ab: false,
            rt_plus: None,
        }
    }
}

/// Produces the groups, one after another, in a fixed rotation.
pub struct GroupEncoder {
    frame: RdsFrame,
    /// Position in the 20-group rotation.
    slot: u8,
    /// Next PS segment, 0..=3.
    ps_seg: u8,
    /// Next RadioText segment, 0..nseg.
    rt_seg: u8,
    /// RT+ item toggle bit, flipped when the tags change.
    item_toggle: bool,
}

impl GroupEncoder {
    pub fn new(frame: RdsFrame) -> Self {
        Self { frame, slot: 0, ps_seg: 0, rt_seg: 0, item_toggle: false }
    }

    /// New data. Takes effect from the next group. If the RadioText bytes, rt_len or the
    /// A/B flag changed, the RadioText segments start again from segment 0. If rt_plus
    /// changed, the RT+ item toggle bit flips.
    pub fn set_frame(&mut self, frame: RdsFrame) {
        let old = self.frame;
        if old.rt != frame.rt || old.rt_len != frame.rt_len || old.ab != frame.ab {
            self.rt_seg = 0;
        }
        if old.rt_plus != frame.rt_plus {
            self.item_toggle = !self.item_toggle;
        }
        self.frame = frame;
    }

    pub fn frame(&self) -> &RdsFrame {
        &self.frame
    }

    /// The next group: blocks 1..4 with offsets A, B, C, D.
    pub fn next_group(&mut self) -> [u32; 4] {
        let slot = self.slot;
        self.slot = (self.slot + 1) % 20;
        if slot.is_multiple_of(2) {
            self.group_0a()
        } else if slot == 9 && self.frame.rt_plus.is_some() {
            self.group_11a()
        } else if slot == 19 && self.frame.rt_plus.is_some() {
            self.group_3a()
        } else {
            self.group_2a()
        }
    }

    /// Block 2 for a version A group; `low5` is the group's own five bits.
    fn block2(&self, group_type: u8, low5: u8) -> u32 {
        let info = ((group_type as u16 & 0x0F) << 12)
            | ((self.frame.tp as u16) << 10)
            | (((self.frame.pty & 0x1F) as u16) << 5)
            | (low5 as u16 & 0x1F);
        block(info, Offset::B)
    }

    fn group_0a(&mut self) -> [u32; 4] {
        let seg = self.ps_seg & 0x03;
        self.ps_seg = (self.ps_seg + 1) & 0x03;
        let di = if seg == 3 { self.frame.stereo as u8 } else { 0 };
        let low5 = ((self.frame.ta as u8) << 4)
            | ((self.frame.music as u8) << 3)
            | (di << 2)
            | seg;
        let hi = self.frame.ps.get(2 * seg as usize).copied().unwrap_or(b' ');
        let lo = self.frame.ps.get(2 * seg as usize + 1).copied().unwrap_or(b' ');
        [
            block(self.frame.pi, Offset::A),
            self.block2(0, low5),
            block(0xE0CD, Offset::C),
            block((hi as u16) << 8 | lo as u16, Offset::D),
        ]
    }

    /// Number of RadioText segments: the text plus a carriage return, padded to fours.
    fn nseg(&self) -> u8 {
        let len = self.frame.rt_len.min(64) as u16;
        let with_cr = len + if len < 64 { 1 } else { 0 };
        with_cr.div_ceil(4).clamp(1, 16) as u8
    }

    /// Byte `index` of the padded RadioText (carriage return then spaces).
    fn rt_byte(&self, index: usize) -> u8 {
        let len = self.frame.rt_len.min(64) as usize;
        if index < len {
            self.frame.rt.get(index).copied().unwrap_or(b' ')
        } else if index == len && len < 64 {
            0x0D
        } else {
            b' '
        }
    }

    fn group_2a(&mut self) -> [u32; 4] {
        let nseg = self.nseg();
        let seg = if self.rt_seg >= nseg { 0 } else { self.rt_seg };
        self.rt_seg = if seg + 1 >= nseg { 0 } else { seg + 1 };
        let low5 = ((self.frame.ab as u8) << 4) | (seg & 0x0F);
        let i = 4 * seg as usize;
        let b3 = (self.rt_byte(i) as u16) << 8 | self.rt_byte(i + 1) as u16;
        let b4 = (self.rt_byte(i + 2) as u16) << 8 | self.rt_byte(i + 3) as u16;
        [
            block(self.frame.pi, Offset::A),
            self.block2(2, low5),
            block(b3, Offset::C),
            block(b4, Offset::D),
        ]
    }

    fn group_3a(&self) -> [u32; 4] {
        [
            block(self.frame.pi, Offset::A),
            self.block2(3, 0b10110),
            block(0x0000, Offset::C),
            block(0x4BD7, Offset::D),
        ]
    }

    fn group_11a(&self) -> [u32; 4] {
        let tags = self.frame.rt_plus.unwrap_or_default();
        let (ct1, start1, len1) = tags[0];
        let (ct2, start2, len2) = tags[1];
        let len1m = (len1.saturating_sub(1) & 0x3F) as u16;
        let len2m = (len2.saturating_sub(1) & 0x1F) as u16;
        let low5 = ((self.item_toggle as u8) << 4) | (1 << 3) | ((ct1 >> 3) & 0b111);
        let info3 = (((ct1 & 0b111) as u16) << 13)
            | (((start1 & 0x3F) as u16) << 7)
            | (len1m << 1)
            | (((ct2 >> 5) & 1) as u16);
        let info4 = (((ct2 & 0x1F) as u16) << 11)
            | (((start2 & 0x3F) as u16) << 5)
            | len2m;
        [
            block(self.frame.pi, Offset::A),
            self.block2(11, low5),
            block(info3, Offset::C),
            block(info4, Offset::D),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_frame() -> RdsFrame {
        RdsFrame { pi: 0xE2A1, ps: *b"RENOU   ", ..RdsFrame::default() }
    }

    fn rt_bytes(text: &[u8]) -> [u8; 64] {
        let mut rt = [b' '; 64];
        rt[..text.len()].copy_from_slice(text);
        rt
    }

    fn group_type(group: [u32; 4]) -> u8 {
        ((group[1] >> 10) >> 12) as u8
    }

    fn block_info(word: u32) -> u16 {
        (word >> 10) as u16
    }

    fn next_of_type(encoder: &mut GroupEncoder, kind: u8) -> [u32; 4] {
        for _ in 0..40 {
            let group = encoder.next_group();
            if group_type(group) == kind {
                return group;
            }
        }
        panic!("no group of type {kind} in 40 groups");
    }

    fn rt_segments(encoder: &mut GroupEncoder, groups: usize) -> Vec<(u8, [u8; 4], bool)> {
        let mut out = Vec::new();
        for _ in 0..groups {
            let group = encoder.next_group();
            if group_type(group) != 2 {
                continue;
            }
            let info2 = block_info(group[1]);
            let ab = ((info2 >> 4) & 1) == 1;
            let seg = (info2 & 0x0F) as u8;
            let info3 = block_info(group[2]);
            let info4 = block_info(group[3]);
            out.push((seg, [(info3 >> 8) as u8, info3 as u8, (info4 >> 8) as u8, info4 as u8], ab));
        }
        out
    }

    #[test]
    fn checkword_and_block_match_the_table() {
        let vectors = [
            (0xE2A1, Offset::A, 0x397, 0x38A876B),
            (0x0000, Offset::A, 0x000, 0x00000FC),
            (0x0408, Offset::B, 0x073, 0x01021EB),
            (0xE0CD, Offset::C, 0x081, 0x38335E9),
            (0x5245, Offset::D, 0x287, 0x1491733),
            (0xFFFF, Offset::D, 0x0CD, 0x3FFFD79),
        ];
        for (info, offset, check, word) in vectors {
            assert_eq!(checkword(info), check, "checkword of {info:04X}");
            assert_eq!(block(info, offset), word, "block of {info:04X}");
        }
        assert_eq!(Offset::A.word(), 0x0FC);
        assert_eq!(Offset::B.word(), 0x198);
        assert_eq!(Offset::C.word(), 0x168);
        assert_eq!(Offset::Cp.word(), 0x350);
        assert_eq!(Offset::D.word(), 0x1B4);
    }

    #[test]
    fn rotation_without_rt_plus_alternates_0a_and_2a() {
        let mut encoder = GroupEncoder::new(base_frame());
        let types: Vec<u8> = (0..20).map(|_| group_type(encoder.next_group())).collect();
        assert_eq!(types.iter().filter(|&&t| t == 0).count(), 10);
        assert_eq!(types.iter().filter(|&&t| t == 2).count(), 10);
        for (slot, &kind) in types.iter().enumerate() {
            assert_eq!(kind, if slot % 2 == 0 { 0 } else { 2 }, "slot {slot}");
        }
    }

    #[test]
    fn rotation_with_rt_plus_adds_11a_and_3a() {
        let frame = RdsFrame { rt_plus: Some([(4, 0, 7), (1, 10, 9)]), ..base_frame() };
        let mut encoder = GroupEncoder::new(frame);
        let types: Vec<u8> = (0..20).map(|_| group_type(encoder.next_group())).collect();
        assert_eq!(types.iter().filter(|&&t| t == 0).count(), 10);
        assert_eq!(types.iter().filter(|&&t| t == 2).count(), 8);
        assert_eq!(types[9], 11);
        assert_eq!(types[19], 3);
        for (slot, &kind) in types.iter().enumerate() {
            if slot == 9 || slot == 19 {
                continue;
            }
            assert_eq!(kind, if slot % 2 == 0 { 0 } else { 2 }, "slot {slot}");
        }
    }

    #[test]
    fn group_0a_carries_ps_and_flags() {
        let frame = RdsFrame {
            pi: 0xE2A1,
            pty: 5,
            tp: true,
            ta: true,
            music: false,
            stereo: true,
            ps: *b"RENOU   ",
            ..RdsFrame::default()
        };
        let mut encoder = GroupEncoder::new(frame);
        let mut ps = Vec::new();
        let mut segments = Vec::new();
        let mut di = Vec::new();
        for _ in 0..20 {
            let group = encoder.next_group();
            let info2 = block_info(group[1]);
            if (info2 >> 12) != 0 {
                continue;
            }
            let low = (info2 & 0x1F) as u8;
            let seg = low & 0x03;
            segments.push(seg);
            di.push((low >> 2) & 1);
            let info4 = block_info(group[3]);
            ps.push((info4 >> 8) as u8);
            ps.push(info4 as u8);
            assert_eq!((info2 >> 10) & 1, 1, "TP");
            assert_eq!((info2 >> 5) & 0x1F, 5, "PTY");
            assert_eq!((info2 >> 4) & 1, 1, "TA");
            assert_eq!((info2 >> 3) & 1, 0, "MS");
        }
        assert_eq!(&ps[..8], &b"RENOU   "[..]);
        assert_eq!(segments, vec![0, 1, 2, 3, 0, 1, 2, 3, 0, 1]);
        for (i, &seg) in segments.iter().enumerate() {
            assert_eq!(di[i], if seg == 3 { 1 } else { 0 }, "DI at segment {seg}");
        }
    }

    #[test]
    fn group_2a_sends_text_with_carriage_return_and_padding() {
        let frame = RdsFrame { rt: rt_bytes(b"Hola"), rt_len: 4, ..RdsFrame::default() };
        let mut encoder = GroupEncoder::new(frame);
        let segments = rt_segments(&mut encoder, 20);
        assert_eq!(segments.len(), 10);
        for (i, (seg, bytes, ab)) in segments.iter().enumerate() {
            assert_eq!(*seg, (i % 2) as u8);
            assert!(!*ab);
            if *seg == 0 {
                assert_eq!(bytes, b"Hola");
            } else {
                assert_eq!(bytes, &[0x0D, b' ', b' ', b' ']);
            }
        }
    }

    #[test]
    fn a_full_text_has_sixteen_segments_and_no_carriage_return() {
        let frame = RdsFrame { rt: [b'x'; 64], rt_len: 64, ..RdsFrame::default() };
        let mut encoder = GroupEncoder::new(frame);
        let segments = rt_segments(&mut encoder, 64);
        assert_eq!(segments.len(), 32);
        let mut seen = [false; 16];
        for (seg, bytes, _) in &segments {
            seen[*seg as usize] = true;
            assert_eq!(*bytes, [b'x'; 4]);
        }
        assert!(seen.iter().all(|&s| s));
        assert!(segments.iter().all(|(_, bytes, _)| !bytes.contains(&0x0D)));
    }

    #[test]
    fn empty_text_sends_one_carriage_return_segment() {
        let frame = RdsFrame { rt_len: 0, ..RdsFrame::default() };
        let mut encoder = GroupEncoder::new(frame);
        let segments = rt_segments(&mut encoder, 20);
        assert_eq!(segments.len(), 10);
        for (seg, bytes, _) in &segments {
            assert_eq!(*seg, 0);
            assert_eq!(*bytes, [0x0D, b' ', b' ', b' ']);
        }
    }

    #[test]
    fn the_ab_flag_is_sent_in_group_2a() {
        let frame = RdsFrame { rt: rt_bytes(b"Hola"), rt_len: 4, ab: true, ..RdsFrame::default() };
        let mut encoder = GroupEncoder::new(frame);
        let segments = rt_segments(&mut encoder, 20);
        assert!(segments.iter().all(|(_, _, ab)| *ab));
    }

    #[test]
    fn new_radio_text_restarts_at_segment_zero() {
        let frame = RdsFrame { rt: rt_bytes(b"AAAAAAAA"), rt_len: 8, ..RdsFrame::default() };
        let mut encoder = GroupEncoder::new(frame);
        let first = rt_segments(&mut encoder, 20);
        assert!(first.iter().any(|(seg, _, _)| *seg != 0));
        encoder.set_frame(RdsFrame { rt: rt_bytes(b"BBBBBBBB"), rt_len: 8, ..RdsFrame::default() });
        let group = next_of_type(&mut encoder, 2);
        assert_eq!((block_info(group[1]) & 0x0F) as u8, 0);
    }

    #[test]
    fn changing_rt_plus_flips_the_item_toggle() {
        let first_tags = Some([(4, 0, 7), (1, 10, 9)]);
        let second_tags = Some([(1, 10, 9), (4, 0, 7)]);
        let toggle = |encoder: &mut GroupEncoder| (block_info(next_of_type(encoder, 11)[1]) >> 4) & 1;
        let mut encoder = GroupEncoder::new(RdsFrame { rt_plus: first_tags, ..RdsFrame::default() });
        let first = toggle(&mut encoder);
        encoder.set_frame(RdsFrame { rt_plus: second_tags, ..RdsFrame::default() });
        let second = toggle(&mut encoder);
        assert_ne!(first, second);
        encoder.set_frame(RdsFrame { rt_plus: second_tags, ..RdsFrame::default() });
        let third = toggle(&mut encoder);
        assert_eq!(second, third);
    }

    #[test]
    fn group_11a_packs_rt_plus_tags() {
        let tags = Some([(4, 0, 7), (1, 10, 9)]);
        let mut encoder = GroupEncoder::new(RdsFrame { rt_plus: tags, ..RdsFrame::default() });
        let group = next_of_type(&mut encoder, 11);
        let info2 = block_info(group[1]);
        let info3 = block_info(group[2]);
        let info4 = block_info(group[3]);
        assert_eq!((info2 >> 4) & 1, 0, "item toggle");
        assert_eq!((info2 >> 3) & 1, 1, "item running");
        let ct1 = ((info2 & 0b111) << 3) | ((info3 >> 13) & 0b111);
        assert_eq!(ct1, 4);
        assert_eq!((info3 >> 7) & 0x3F, 0, "start 1");
        assert_eq!((info3 >> 1) & 0x3F, 6, "length 1 marker");
        let ct2 = ((info3 & 1) << 5) | ((info4 >> 11) & 0x1F);
        assert_eq!(ct2, 1);
        assert_eq!((info4 >> 5) & 0x3F, 10, "start 2");
        assert_eq!(info4 & 0x1F, 8, "length 2 marker");
        assert_eq!(info3, 0x800C);
        assert_eq!(info4, 0x0948);
    }

    #[test]
    fn frame_returns_the_current_data() {
        let frame = base_frame();
        let mut encoder = GroupEncoder::new(frame);
        assert_eq!(encoder.frame(), &frame);
        let other = RdsFrame { pi: 0x1234, ..base_frame() };
        encoder.set_frame(other);
        assert_eq!(encoder.frame(), &other);
    }

    #[test]
    fn every_block_has_a_valid_checkword() {
        let frame = RdsFrame {
            pi: 0xE2A1,
            pty: 9,
            tp: true,
            ta: true,
            music: false,
            stereo: true,
            ps: *b"RENOU   ",
            rt: rt_bytes(b"Radio Renou 98.5"),
            rt_len: 16,
            ab: true,
            rt_plus: Some([(4, 0, 7), (1, 10, 9)]),
        };
        let offsets = [Offset::A, Offset::B, Offset::C, Offset::D];
        let mut encoder = GroupEncoder::new(frame);
        for _ in 0..200 {
            let group = encoder.next_group();
            for (i, &word) in group.iter().enumerate() {
                let info = block_info(word);
                assert_eq!(word, block(info, offsets[i]));
                assert_eq!(word & 0x3FF, (checkword(info) ^ offsets[i].word()) as u32);
            }
        }
    }
}
