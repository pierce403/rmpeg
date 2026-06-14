use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ac3Kind {
    Ac3,
    Eac3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Ac3Info {
    kind: Ac3Kind,
    sample_rate: u32,
    channels: u16,
    bitrate: u32,
}

pub fn parse_raw_ac3_or_eac3(bytes: &[u8]) -> Result<ProbeDocument> {
    parse_raw_ac3_or_eac3_from_sync(bytes, 0)
}

pub fn parse_raw_ac3_or_eac3_scanning(bytes: &[u8]) -> Result<ProbeDocument> {
    let sync = find_sync(bytes)
        .ok_or_else(|| RmpegError::InvalidData("raw AC-3 stream has no sync word".to_string()))?;
    parse_raw_ac3_or_eac3_from_sync(bytes, sync)
}

pub fn looks_like_raw_ac3_or_eac3(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x0b, 0x77]) && parse_info_at(bytes, 0).is_some()
}

fn parse_raw_ac3_or_eac3_from_sync(bytes: &[u8], sync: usize) -> Result<ProbeDocument> {
    let info = parse_info_at(bytes, sync)
        .ok_or_else(|| RmpegError::InvalidData("unsupported raw AC-3/E-AC-3 header".to_string()))?;
    let duration_seconds = bytes.len() as f64 * 8.0 / info.bitrate as f64;
    let (format, codec) = match info.kind {
        Ac3Kind::Ac3 => ("ac3", "ac3"),
        Ac3Kind::Eac3 => ("eac3", "eac3"),
    };
    Ok(ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec,
            info.sample_rate,
            info.channels,
            0,
            duration_seconds,
        )],
    })
}

fn parse_info_at(bytes: &[u8], pos: usize) -> Option<Ac3Info> {
    if bytes.get(pos..pos + 2) != Some(&[0x0b, 0x77]) {
        return None;
    }
    parse_ac3_info(&bytes[pos..]).or_else(|| parse_eac3_info(&bytes[pos..]))
}

fn parse_ac3_info(bytes: &[u8]) -> Option<Ac3Info> {
    if bytes.len() < 7 {
        return None;
    }
    let mut bits = BitReader::new(bytes);
    if bits.read(16)? != 0x0b77 {
        return None;
    }
    bits.skip(16)?;
    let fscod = bits.read(2)? as usize;
    let frmsizecod = bits.read(6)? as usize;
    let bsid = bits.read(5)? as u8;
    if bsid > 10 {
        return None;
    }
    bits.skip(3)?;
    let acmod = bits.read(3)? as u8;
    skip_ac3_mix_levels(&mut bits, acmod)?;
    let lfeon = bits.read(1)? != 0;

    let sample_rate = [48_000, 44_100, 32_000].get(fscod).copied()?;
    let bitrate = ac3_bitrate(frmsizecod)?;
    Some(Ac3Info {
        kind: Ac3Kind::Ac3,
        sample_rate,
        channels: ac3_channel_count(acmod, lfeon)?,
        bitrate,
    })
}

fn parse_eac3_info(bytes: &[u8]) -> Option<Ac3Info> {
    if bytes.len() < 8 {
        return None;
    }
    let mut bits = BitReader::new(bytes);
    if bits.read(16)? != 0x0b77 {
        return None;
    }
    let strmtyp = bits.read(2)? as u8;
    if strmtyp > 2 {
        return None;
    }
    bits.skip(3)?;
    let frame_size = (bits.read(11)? + 1) * 2;
    if frame_size < 6 {
        return None;
    }
    let fscod = bits.read(2)? as usize;
    let (sample_rate, blocks) = if fscod == 3 {
        let fscod2 = bits.read(2)? as usize;
        ([24_000, 22_050, 16_000].get(fscod2).copied()?, 6_u32)
    } else {
        let numblkscod = bits.read(2)? as usize;
        (
            [48_000, 44_100, 32_000].get(fscod).copied()?,
            [1_u32, 2, 3, 6].get(numblkscod).copied()?,
        )
    };
    let acmod = bits.read(3)? as u8;
    let lfeon = bits.read(1)? != 0;
    let bsid = bits.read(5)? as u8;
    if !(11..=16).contains(&bsid) {
        return None;
    }
    let channels = ac3_channel_count(acmod, lfeon)?;
    let bitrate =
        (u64::from(frame_size) * 8 * u64::from(sample_rate) / (u64::from(blocks) * 256)) as u32;
    Some(Ac3Info {
        kind: Ac3Kind::Eac3,
        sample_rate,
        channels,
        bitrate,
    })
}

fn skip_ac3_mix_levels(bits: &mut BitReader<'_>, acmod: u8) -> Option<()> {
    if acmod & 0x01 != 0 && acmod != 1 {
        bits.skip(2)?;
    }
    if acmod & 0x04 != 0 {
        bits.skip(2)?;
    }
    if acmod == 2 {
        bits.skip(2)?;
    }
    Some(())
}

fn ac3_bitrate(frmsizecod: usize) -> Option<u32> {
    [
        32_000, 40_000, 48_000, 56_000, 64_000, 80_000, 96_000, 112_000, 128_000, 160_000, 192_000,
        224_000, 256_000, 320_000, 384_000, 448_000, 512_000, 576_000, 640_000,
    ]
    .get(frmsizecod / 2)
    .copied()
}

fn ac3_channel_count(acmod: u8, lfeon: bool) -> Option<u16> {
    let mut channels = match acmod {
        0 => 2,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 3,
        5 => 4,
        6 => 4,
        7 => 5,
        _ => return None,
    };
    if lfeon {
        channels += 1;
    }
    Some(channels)
}

fn find_sync(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|window| window == [0x0b, 0x77])
}

#[derive(Debug)]
struct BitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit_pos: 0 }
    }

    fn read(&mut self, bits: usize) -> Option<u32> {
        let mut value = 0_u32;
        for _ in 0..bits {
            let byte = *self.bytes.get(self.bit_pos / 8)?;
            value = (value << 1) | u32::from((byte >> (7 - (self.bit_pos % 8))) & 1);
            self.bit_pos += 1;
        }
        Some(value)
    }

    fn skip(&mut self, bits: usize) -> Option<()> {
        self.read(bits).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ac3_header() {
        let bytes = [
            0x0b, 0x77, 0x88, 0xca, 0x1c, 0x40, 0xa0, 0xdf, 0xa0, 0xc0, 0x7f, 0x4a,
        ];
        let info = parse_ac3_info(&bytes).expect("valid ac3");
        assert_eq!(info.kind, Ac3Kind::Ac3);
        assert_eq!(info.sample_rate, 48_000);
        assert_eq!(info.channels, 4);
        assert_eq!(info.bitrate, 384_000);
    }

    #[test]
    fn parses_eac3_header() {
        let bytes = [
            0x0b, 0x77, 0x01, 0xff, 0x3f, 0x86, 0xa0, 0x36, 0x49, 0x23, 0x04, 0x64,
        ];
        let info = parse_eac3_info(&bytes).expect("valid eac3");
        assert_eq!(info.kind, Ac3Kind::Eac3);
        assert_eq!(info.sample_rate, 48_000);
        assert_eq!(info.channels, 6);
        assert_eq!(info.bitrate, 256_000);
    }

    #[test]
    fn path_gated_scan_finds_prefixed_eac3_sync() {
        let bytes = [
            0xaa, 0xbb, 0x0b, 0x77, 0x00, 0xff, 0x34, 0x86, 0xff, 0xf0, 0x47, 0x12, 0x81, 0x00,
        ];
        let doc = parse_raw_ac3_or_eac3_scanning(&bytes).expect("valid prefixed eac3");
        assert_eq!(doc.format, "eac3");
        assert_eq!(doc.streams[0].codec_name, "eac3");
        assert_eq!(doc.streams[0].channels, Some(2));
    }
}
