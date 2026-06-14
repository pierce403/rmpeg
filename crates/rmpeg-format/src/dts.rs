use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy)]
struct DtsCoreInfo {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    bitrate: Option<u32>,
}

pub fn parse_dtshd(bytes: &[u8]) -> Result<ProbeDocument> {
    if !bytes.starts_with(b"DTSHDHDR") {
        return Err(RmpegError::InvalidData("missing DTS-HD header".to_string()));
    }

    let mut aupr = None;
    let mut pos = 0;
    while pos + 16 <= bytes.len() {
        let id = &bytes[pos..pos + 8];
        let size = read_u64_be(bytes, pos + 8)? as usize;
        let data_start = pos + 16;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("DTS-HD chunk size overflow".to_string()))?;
        if data_end > bytes.len() {
            return Err(RmpegError::InvalidData(format!(
                "invalid DTS-HD chunk {} size {}",
                String::from_utf8_lossy(id),
                size
            )));
        }
        if id == b"AUPR-HDR" {
            aupr = Some(&bytes[data_start..data_end]);
            break;
        }
        pos = data_end;
    }

    let aupr = aupr
        .ok_or_else(|| RmpegError::InvalidData("DTS-HD file has no AUPR-HDR chunk".to_string()))?;
    if aupr.len() < 20 {
        return Err(RmpegError::UnexpectedEof {
            needed: 20,
            remaining: aupr.len(),
        });
    }

    let kind = aupr[2];
    let sample_rate = read_u24_be(aupr, 3)?;
    let duration_quanta = u32::from(read_u16_be(aupr, 14)?);
    let channel_mask = read_u32_be(aupr, 16)?;
    let channels = dtshd_channels(channel_mask).ok_or_else(|| {
        RmpegError::InvalidData(format!("unsupported DTS-HD channel mask {channel_mask:#x}"))
    })?;
    let bits_per_sample = match kind {
        4 | 5 if sample_rate == 192_000 && aupr.get(10) == Some(&0x08) => 16,
        4 | 5 => 24,
        _ => 0,
    };
    let duration_seconds = duration_quanta as f64 * 384.0 / sample_rate as f64;
    Ok(audio_document(
        "dtshd",
        sample_rate,
        channels,
        bits_per_sample,
        duration_seconds,
    ))
}

pub fn parse_raw_dts(bytes: &[u8]) -> Result<ProbeDocument> {
    let core = parse_dts_core(bytes).ok_or_else(|| {
        RmpegError::InvalidData("unsupported or unrecognized DTS stream".to_string())
    })?;
    let has_hd_extension = bytes.get(..bytes.len().min(8192)).is_some_and(|head| {
        head.windows(4)
            .any(|window| window == [0x64, 0x58, 0x20, 0x25])
    });
    let (channels, bits_per_sample, duration_seconds) = if has_hd_extension {
        (core.channels.max(8), 24, 0.0)
    } else {
        let bitrate = core.bitrate.ok_or_else(|| {
            RmpegError::InvalidData("raw DTS stream has no known bitrate".to_string())
        })?;
        (
            core.channels,
            core.bits_per_sample,
            bytes.len() as f64 * 8.0 / bitrate as f64,
        )
    };
    Ok(audio_document(
        "dts",
        core.sample_rate,
        channels,
        bits_per_sample,
        duration_seconds,
    ))
}

pub fn parse_mpegts_dts(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_mpegts(bytes) {
        return Err(RmpegError::InvalidData(
            "missing MPEG-TS packet sync".to_string(),
        ));
    }

    let mut first_pts = None;
    let mut last_pts = None;
    let mut first_core = None;
    let mut pos = 0;
    while pos + 188 <= bytes.len() {
        if bytes[pos] != 0x47 {
            return Err(RmpegError::InvalidData(
                "invalid MPEG-TS packet sync".to_string(),
            ));
        }
        let payload_unit_start = bytes[pos + 1] & 0x40 != 0;
        let adaptation_control = (bytes[pos + 3] >> 4) & 0x03;
        let mut payload = pos + 4;
        if matches!(adaptation_control, 2 | 3) {
            let length = usize::from(bytes[payload]);
            payload = payload.checked_add(1 + length).ok_or_else(|| {
                RmpegError::InvalidData("MPEG-TS adaptation overflow".to_string())
            })?;
        }
        if matches!(adaptation_control, 1 | 3) && payload < pos + 188 {
            let data = &bytes[payload..pos + 188];
            if payload_unit_start && data.len() >= 9 && data.starts_with(&[0x00, 0x00, 0x01]) {
                let pts = parse_pes_pts(data);
                let payload_start = 9 + usize::from(data[8]);
                if payload_start <= data.len() {
                    let pes_payload = &data[payload_start..];
                    if let Some(sync) = find_dts_sync(pes_payload) {
                        if first_core.is_none() {
                            first_core = parse_dts_core(&pes_payload[sync..]);
                        }
                        if let Some(pts) = pts {
                            first_pts.get_or_insert(pts);
                            last_pts = Some(pts);
                        }
                    }
                }
            }
        }
        pos += 188;
    }

    let core = first_core.ok_or_else(|| {
        RmpegError::InvalidData("MPEG-TS stream has no DTS PES payload".to_string())
    })?;
    let duration_seconds = match (first_pts, last_pts) {
        (Some(first), Some(last)) if last >= first => (last - first) as f64 / 90_000.0,
        _ => 0.0,
    };
    Ok(audio_document(
        "mpegts",
        core.sample_rate,
        core.channels,
        core.bits_per_sample,
        duration_seconds,
    ))
}

pub fn looks_like_raw_dts(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x7f, 0xfe, 0x80, 0x01])
}

pub fn looks_like_mpegts(bytes: &[u8]) -> bool {
    bytes.len() >= 188 * 3 && (0..3).all(|packet| bytes.get(packet * 188) == Some(&0x47))
}

fn audio_document(
    format: &str,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    duration_seconds: f64,
) -> ProbeDocument {
    ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "dts",
            sample_rate,
            channels,
            bits_per_sample,
            duration_seconds,
        )],
    }
}

fn parse_dts_core(bytes: &[u8]) -> Option<DtsCoreInfo> {
    if !looks_like_raw_dts(bytes) {
        return None;
    }
    let mut bits = DtsBitReader::new(bytes, 32);
    bits.skip(1)?;
    bits.skip(5)?;
    bits.skip(1)?;
    bits.skip(7)?;
    bits.skip(14)?;
    let audio_mode = bits.read(6)? as u8;
    let sample_rate_index = bits.read(4)? as usize;
    let bitrate_index = bits.read(5)? as usize;
    bits.skip(1)?;
    bits.skip(1)?;
    bits.skip(1)?;
    bits.skip(1)?;
    bits.skip(1)?;
    bits.skip(3)?;
    let extension_audio = bits.read(1)? != 0;
    bits.skip(1)?;
    let lfe = bits.read(2)? as u8;

    let mut channels = dts_audio_mode_channels(audio_mode)?;
    if lfe != 0 {
        channels = channels.saturating_add(1);
    }
    if extension_audio {
        channels = channels.saturating_add(1);
    }

    Some(DtsCoreInfo {
        sample_rate: dts_sample_rate(sample_rate_index)?,
        channels,
        bits_per_sample: 0,
        bitrate: dts_bitrate(bitrate_index),
    })
}

fn dtshd_channels(mask: u32) -> Option<u16> {
    match mask & 0xffff_ff00 {
        0x0000_0f00 => Some(6),
        0x0000_1f00 => Some(7),
        0x0008_4b00 => Some(8),
        _ => None,
    }
}

fn dts_audio_mode_channels(mode: u8) -> Option<u16> {
    match mode {
        0 => Some(1),
        1 | 2 => Some(2),
        3..=5 => Some(3),
        6 => Some(4),
        7 | 8 => Some(5),
        9 => Some(5),
        10..=12 => Some(6),
        13 => Some(7),
        14 | 15 => Some(8),
        _ => None,
    }
}

fn dts_sample_rate(index: usize) -> Option<u32> {
    [
        0, 8_000, 16_000, 32_000, 0, 0, 11_025, 22_050, 44_100, 0, 0, 12_000, 24_000, 48_000,
        96_000, 192_000,
    ]
    .get(index)
    .copied()
    .filter(|rate| *rate != 0)
}

fn dts_bitrate(index: usize) -> Option<u32> {
    [
        32_000, 56_000, 64_000, 96_000, 112_000, 128_000, 192_000, 224_000, 256_000, 320_000,
        384_000, 448_000, 512_000, 576_000, 640_000, 768_000, 960_000, 1_024_000, 1_152_000,
        1_280_000, 1_344_000, 1_408_000, 1_411_200, 1_472_000, 1_536_000, 1_920_000, 2_048_000,
        3_072_000, 3_840_000,
    ]
    .get(index)
    .copied()
}

fn parse_pes_pts(data: &[u8]) -> Option<u64> {
    if data.len() < 14 || data[7] & 0x80 == 0 {
        return None;
    }
    let pts = &data[9..14];
    Some(
        (u64::from((pts[0] >> 1) & 0x07) << 30)
            | (u64::from(pts[1]) << 22)
            | (u64::from((pts[2] >> 1) & 0x7f) << 15)
            | (u64::from(pts[3]) << 7)
            | u64::from((pts[4] >> 1) & 0x7f),
    )
}

fn find_dts_sync(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == [0x7f, 0xfe, 0x80, 0x01])
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn read_u24_be(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 3;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok((u32::from(bytes[offset]) << 16)
        | (u32::from(bytes[offset + 1]) << 8)
        | u32::from(bytes[offset + 2]))
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u64_be(bytes: &[u8], offset: usize) -> Result<u64> {
    let end = offset + 8;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u64::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ]))
}

struct DtsBitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
}

impl<'a> DtsBitReader<'a> {
    fn new(bytes: &'a [u8], bit_pos: usize) -> Self {
        Self { bytes, bit_pos }
    }

    fn read(&mut self, count: usize) -> Option<u32> {
        if count > 32 || self.bytes.len() * 8 < self.bit_pos + count {
            return None;
        }
        let mut value = 0_u32;
        for _ in 0..count {
            let byte = self.bytes[self.bit_pos / 8];
            let bit = (byte >> (7 - (self.bit_pos % 8))) & 1;
            value = (value << 1) | u32::from(bit);
            self.bit_pos += 1;
        }
        Some(value)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        if self.bytes.len() * 8 < self.bit_pos + count {
            None
        } else {
            self.bit_pos += count;
            Some(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dtshd_aupr_metadata() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"DTSHDHDR");
        bytes.extend_from_slice(&0_u64.to_be_bytes());
        bytes.extend_from_slice(b"AUPR-HDR");
        bytes.extend_from_slice(&24_u64.to_be_bytes());
        bytes.extend_from_slice(&[
            0x00, 0x00, 0x05, 0x00, 0xbb, 0x80, 0x00, 0x00, 0x00, 0x06, 0x02, 0x00, 0x00, 0x00,
            0x00, 0x08, 0x00, 0x08, 0x4b, 0x04, 0x00, 0x02, 0x00, 0x00,
        ]);

        let doc = parse_dtshd(&bytes).expect("dtshd");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "dtshd");
        assert_eq!(stream.codec_name, "dts");
        assert_eq!(stream.sample_rate, Some(48_000));
        assert_eq!(stream.channels, Some(8));
        assert_eq!(stream.bits_per_sample, Some(24));
        assert_eq!(stream.duration_seconds, Some(0.064));
    }

    #[test]
    fn parses_raw_dts_core_header() {
        let bytes = [
            0x7f, 0xfe, 0x80, 0x01, 0xfc, 0x3c, 0x7d, 0xb2, 0x77, 0x00, 0x1d, 0x3b, 0x40, 0x09,
            0xef, 0x7b,
        ];
        let core = parse_dts_core(&bytes).expect("core");
        assert_eq!(core.sample_rate, 48_000);
        assert_eq!(core.channels, 7);
        assert_eq!(core.bitrate, Some(1_536_000));
    }
}
