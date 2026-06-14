use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn looks_like_ea(bytes: &[u8]) -> bool {
    bytes.starts_with(b"MVhd")
        || bytes.starts_with(b"AVP6")
        || bytes.starts_with(b"MADk")
        || bytes.starts_with(b"SEAD")
        || bytes.starts_with(b"kVGT")
        || bytes.starts_with(b"1SNh")
}

pub fn parse_ea(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.starts_with(b"MADk") {
        return parse_mad(bytes);
    }
    if bytes.starts_with(b"SEAD") || bytes.starts_with(b"kVGT") || bytes.starts_with(b"1SNh") {
        return parse_tgv_or_tgq(bytes);
    }

    let mut streams = Vec::new();
    let mut pos = 0;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = read_u32(bytes, pos + 4)? as usize;
        if size < 8 || pos + size > bytes.len() {
            break;
        }
        let data_start = pos + 8;
        let data_end = pos + size;
        match id {
            b"MVhd" | b"AVhd" => {
                if let Some(stream) =
                    parse_video_header(&bytes[data_start..data_end], streams.len())?
                {
                    streams.push(stream);
                }
            }
            b"SCHl" => streams.push(StreamMetadata {
                index: streams.len(),
                codec_type: "audio".to_string(),
                codec_name: "adpcm_ea_r3".to_string(),
                sample_rate: Some(32_000),
                channels: Some(2),
                bits_per_sample: Some(0),
                duration_seconds: Some(0.0),
                width: None,
                height: None,
                frame_rate: None,
            }),
            _ => {}
        }
        pos += size + (size & 1);
    }

    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "EA file has no supported streams".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "ea".to_string(),
        streams,
    })
}

fn parse_tgv_or_tgq(bytes: &[u8]) -> Result<ProbeDocument> {
    let mut tgv_video = None;
    let mut tgq_video = None;
    let mut sead_audio = None;
    let mut eacs_audio = None;
    let mut pos = 0;

    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        match id {
            b"SEAD" => {
                let size = read_u32(bytes, pos + 4)? as usize;
                if size < 8 || pos + size > bytes.len() {
                    break;
                }
                let data = &bytes[pos + 8..pos + size];
                if data.len() >= 8 {
                    let sample_rate = read_u32(data, 0)?;
                    let channels = read_u32(data, 4)?;
                    if sample_rate != 0 && (1..=8).contains(&channels) {
                        sead_audio = Some((sample_rate, channels as u16));
                    }
                }
                pos += size + (size & 1);
            }
            b"kVGT" => {
                let size = read_u32(bytes, pos + 4)? as usize;
                if size < 16 || pos + size > bytes.len() {
                    break;
                }
                let data = &bytes[pos + 8..pos + size];
                let width = u32::from(read_u16(data, 0)?);
                let height = u32::from(read_u16(data, 2)?);
                if width != 0 && height != 0 {
                    tgv_video = Some((width, height));
                }
                pos += size + (size & 1);
            }
            b"1SNh" => {
                let size = chunk_size_either_endian(bytes, pos + 4)?;
                if size < 16 || pos + size > bytes.len() {
                    break;
                }
                let data = &bytes[pos + 8..pos + size];
                if let Some(audio) = parse_eacs(data)? {
                    eacs_audio = Some(audio);
                }
                pos += size + (size & 1);
            }
            b"TGQs" => {
                let size = read_u32_be(bytes, pos + 4)? as usize;
                if size < 16 || pos + size > bytes.len() {
                    break;
                }
                let width = u32::from(read_u16_be(bytes, pos + 8)?);
                let height = u32::from(read_u16_be(bytes, pos + 10)?);
                if width != 0 && height != 0 {
                    tgq_video = Some((width, height));
                }
                pos += size + (size & 1);
            }
            _ => {
                pos += 1;
            }
        }
    }

    let mut streams = Vec::new();
    if let Some((width, height)) = tgv_video {
        streams.push(StreamMetadata::video(
            streams.len(),
            "tgv",
            width,
            height,
            Some(0.0),
            None,
        ));
    } else if let Some((width, height)) = tgq_video {
        streams.push(StreamMetadata::video(
            streams.len(),
            "tgq",
            width,
            height,
            Some(0.0),
            None,
        ));
    }

    if let Some((sample_rate, channels)) = sead_audio {
        streams.push(StreamMetadata::audio(
            streams.len(),
            "adpcm_ima_ea_sead",
            sample_rate,
            channels,
            4,
            0.0,
        ));
    } else if let Some((sample_rate, channels)) = eacs_audio {
        let (codec, bits) = if tgq_video.is_some() {
            ("pcm_mulaw", 8)
        } else {
            ("adpcm_ima_ea_eacs", 0)
        };
        streams.push(StreamMetadata::audio(
            streams.len(),
            codec,
            sample_rate,
            channels,
            bits,
            0.0,
        ));
    }

    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "EA TGV/TGQ file has no supported streams".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "ea".to_string(),
        streams,
    })
}

fn parse_eacs(data: &[u8]) -> Result<Option<(u32, u16)>> {
    let Some(pos) = find_bytes(data, b"EACS") else {
        return Ok(None);
    };
    if pos + 12 > data.len() {
        return Ok(None);
    }
    let raw = &data[pos + 4..pos + 8];
    let sample_rate_le = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let sample_rate_be = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let sample_rate = if (1..=192_000).contains(&sample_rate_le) {
        sample_rate_le
    } else {
        sample_rate_be
    };
    let channels = data[pos + 8];
    if sample_rate == 0 || sample_rate > 192_000 || channels == 0 || channels > 8 {
        return Ok(None);
    }
    Ok(Some((sample_rate, u16::from(channels))))
}

fn parse_mad(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 24 {
        return Err(RmpegError::UnexpectedEof {
            needed: 24,
            remaining: bytes.len(),
        });
    }
    let width = u32::from(read_u16(bytes, 16)?);
    let height = u32::from(read_u16(bytes, 18)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid EA MAD video metadata".to_string(),
        ));
    }

    let mut streams = vec![StreamMetadata::video(
        0,
        "mad",
        width,
        height,
        Some(0.0),
        None,
    )];
    if let Some(audio) = parse_mad_schl(bytes, streams.len())? {
        streams.push(audio);
    }

    Ok(ProbeDocument {
        format: "ea".to_string(),
        streams,
    })
}

fn parse_mad_schl(bytes: &[u8], index: usize) -> Result<Option<StreamMetadata>> {
    let Some(pos) = find_bytes(bytes, b"SCHl") else {
        return Ok(None);
    };
    if pos + 8 > bytes.len() {
        return Ok(None);
    }
    let size = read_u32(bytes, pos + 4)? as usize;
    let data_start = pos + 8;
    let data_end = data_start
        .checked_add(size.saturating_sub(8))
        .ok_or_else(|| RmpegError::InvalidData("EA MAD SCHl size overflow".to_string()))?
        .min(bytes.len());
    let data = &bytes[data_start..data_end];

    let sample_rate = find_tag(data, 0x84, 3)
        .and_then(|value| value.get(0..3))
        .map(|value| {
            (u32::from(value[0]) << 16) | (u32::from(value[1]) << 8) | u32::from(value[2])
        });
    let channels = find_tag(data, 0x82, 1).and_then(|value| value.first().copied());
    let codec_tag = find_tag(data, 0x85, 3);
    let (codec_name, bits_per_sample) = match codec_tag {
        Some([0x02, b'R', b'S', ..]) => ("adpcm_ea_r1", 0),
        Some([0x03, b'/', b'c', ..]) => ("pcm_s16le_planar", 16),
        _ => return Ok(None),
    };
    let (Some(sample_rate), Some(channels)) = (sample_rate, channels) else {
        return Ok(None);
    };
    if sample_rate == 0 || channels == 0 {
        return Ok(None);
    }

    Ok(Some(StreamMetadata::audio(
        index,
        codec_name,
        sample_rate,
        u16::from(channels),
        bits_per_sample,
        0.0,
    )))
}

fn find_tag(data: &[u8], tag: u8, len: u8) -> Option<&[u8]> {
    data.windows(2)
        .position(|window| window == [tag, len])
        .and_then(|pos| data.get(pos + 2..pos + 2 + usize::from(len)))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_video_header(data: &[u8], index: usize) -> Result<Option<StreamMetadata>> {
    if data.len() < 20 {
        return Ok(None);
    }
    let codec = &data[0..4];
    if codec != b"vp60" && codec != b"vp61" && codec != b"\0\0\0\0" {
        return Ok(None);
    }
    let width = round_up_16(u32::from(read_u16(data, 4)?));
    let height = round_up_16(u32::from(read_u16(data, 6)?));
    let frames = read_u32(data, 8)?;
    let fps_fixed = read_u32(data, 16)?;
    let duration = if fps_fixed == 0 {
        0.0
    } else {
        frames as f64 * 32768.0 / fps_fixed as f64
    };
    Ok(Some(StreamMetadata::video(
        index,
        "vp6",
        width,
        height,
        Some(duration),
        None,
    )))
}

fn round_up_16(value: u32) -> u32 {
    value.saturating_add(15) / 16 * 16
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
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

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
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

fn chunk_size_either_endian(bytes: &[u8], offset: usize) -> Result<usize> {
    let le = read_u32(bytes, offset)? as usize;
    if le >= 8 && offset - 4 + le <= bytes.len() {
        return Ok(le);
    }
    let be = read_u32_be(bytes, offset)? as usize;
    if be >= 8 && offset - 4 + be <= bytes.len() {
        return Ok(be);
    }
    Ok(le)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_video_only_mad_header() {
        let mut bytes = vec![0; 24];
        bytes[0..4].copy_from_slice(b"MADk");
        bytes[16..18].copy_from_slice(&96_u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&96_u16.to_le_bytes());

        let doc = parse_ea(&bytes).expect("mad");

        assert_eq!(doc.format, "ea");
        assert_eq!(doc.streams.len(), 1);
        assert_eq!(doc.streams[0].codec_name, "mad");
        assert_eq!(doc.streams[0].duration_seconds, Some(0.0));
    }

    #[test]
    fn parses_mad_schl_audio_tags() {
        let mut bytes = vec![0; 24];
        bytes[0..4].copy_from_slice(b"MADk");
        bytes[16..18].copy_from_slice(&720_u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&496_u16.to_le_bytes());
        bytes.extend_from_slice(b"SCHl");
        bytes.extend_from_slice(&48_u32.to_le_bytes());
        bytes.extend_from_slice(&[
            0x50, 0x54, 0x00, 0x00, 0x85, 0x03, 0x02, b'R', b'S', 0x82, 0x01, 0x02, 0x84, 0x03,
            0x00, 0xbb, 0x80, 0xff, 0x00, 0x00, 0x00,
        ]);

        let doc = parse_ea(&bytes).expect("mad");

        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[1].codec_name, "adpcm_ea_r1");
        assert_eq!(doc.streams[1].sample_rate, Some(48_000));
        assert_eq!(doc.streams[1].channels, Some(2));
    }

    #[test]
    fn parses_tgv_with_sead_audio() {
        let mut bytes = b"SEAD".to_vec();
        bytes.extend_from_slice(&20_u32.to_le_bytes());
        bytes.extend_from_slice(&22_050_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(b"kVGT");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&320_u16.to_le_bytes());
        bytes.extend_from_slice(&200_u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);

        let doc = parse_ea(&bytes).expect("tgv");

        assert_eq!(doc.streams[0].codec_name, "tgv");
        assert_eq!(doc.streams[1].codec_name, "adpcm_ima_ea_sead");
        assert_eq!(doc.streams[1].bits_per_sample, Some(4));
    }

    #[test]
    fn parses_tgq_with_eacs_audio() {
        let mut bytes = b"1SNh".to_vec();
        bytes.extend_from_slice(&40_u32.to_be_bytes());
        bytes.extend_from_slice(b"EACS");
        bytes.extend_from_slice(&22_050_u32.to_be_bytes());
        bytes.extend_from_slice(&[2, 2, 1, 0]);
        bytes.resize(40, 0);
        bytes.extend_from_slice(b"TGQs");
        bytes.extend_from_slice(&16_u32.to_be_bytes());
        bytes.extend_from_slice(&208_u16.to_be_bytes());
        bytes.extend_from_slice(&112_u16.to_be_bytes());
        bytes.extend_from_slice(&[0; 4]);

        let doc = parse_ea(&bytes).expect("tgq");

        assert_eq!(doc.streams[0].codec_name, "tgq");
        assert_eq!(doc.streams[1].codec_name, "pcm_mulaw");
        assert_eq!(doc.streams[1].bits_per_sample, Some(8));
    }
}
