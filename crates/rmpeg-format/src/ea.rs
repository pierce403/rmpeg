use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn looks_like_ea(bytes: &[u8]) -> bool {
    bytes.starts_with(b"MVhd") || bytes.starts_with(b"AVP6") || bytes.starts_with(b"MADk")
}

pub fn parse_ea(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.starts_with(b"MADk") {
        return parse_mad(bytes);
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
}
