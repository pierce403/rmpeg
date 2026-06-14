use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_realmedia(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_realmedia(bytes) {
        return Err(RmpegError::InvalidData(
            "missing RealMedia header".to_string(),
        ));
    }
    if looks_like_realaudio(bytes) {
        return parse_realaudio(bytes);
    }

    let mut streams = Vec::new();
    let mut pos = 0;
    while pos + 10 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = usize::try_from(read_u32_be(bytes, pos + 4)?)
            .map_err(|_| RmpegError::InvalidData("RealMedia chunk is too large".to_string()))?;
        if size < 10 || pos + size > bytes.len() {
            break;
        }
        if id == b"MDPR" {
            if let Some(stream) = parse_mdpr(&bytes[pos + 10..pos + size], streams.len())? {
                streams.push(stream);
            }
        }
        pos += size;
    }

    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "RealMedia file has no supported streams".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "rm".to_string(),
        streams,
    })
}

pub fn looks_like_realmedia(bytes: &[u8]) -> bool {
    bytes.starts_with(b".RMF") || looks_like_realaudio(bytes)
}

fn looks_like_realaudio(bytes: &[u8]) -> bool {
    bytes.starts_with(b".ra\xfd")
}

fn parse_realaudio(bytes: &[u8]) -> Result<ProbeDocument> {
    let (codec_name, sample_rate, duration_seconds) = if let Some(tag) = find_bytes(bytes, b"sipr")
    {
        let data_start = tag + 11;
        if data_start > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_start,
                remaining: bytes.len(),
            });
        }
        let sample_rate = if bytes.windows(2).any(|value| value == [0x3e, 0x80]) {
            16_000
        } else {
            8_000
        };
        (
            "sipr",
            sample_rate,
            (bytes.len() - data_start) as f64 / 2_000.0,
        )
    } else if let Some(tag) = find_bytes(bytes, b"28_8") {
        let data_start = tag + 11;
        if data_start > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_start,
                remaining: bytes.len(),
            });
        }
        let frame_size = read_u32_be(bytes, 24)?;
        if frame_size == 0 {
            return Err(RmpegError::InvalidData(
                "RealAudio frame size is zero".to_string(),
            ));
        }
        let frames = (bytes.len() - data_start) as f64 / frame_size as f64;
        ("ra_288", 8_000, frames * 0.02)
    } else if let Some(tag) = find_bytes(bytes, b"lpcJ") {
        let data_start = tag + 4;
        if data_start > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_start,
                remaining: bytes.len(),
            });
        }
        ("ra_144", 8_000, (bytes.len() - data_start) as f64 / 1_000.0)
    } else {
        return Err(RmpegError::InvalidData(
            "unsupported RealAudio header".to_string(),
        ));
    };

    Ok(ProbeDocument {
        format: "rm".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec_name,
            sample_rate,
            1,
            0,
            duration_seconds,
        )],
    })
}

fn parse_mdpr(data: &[u8], index: usize) -> Result<Option<StreamMetadata>> {
    if data.len() < 31 {
        return Err(RmpegError::UnexpectedEof {
            needed: 31,
            remaining: data.len(),
        });
    }
    let duration_seconds = read_u32_be(data, 26)? as f64 / 1000.0;
    let name_len = usize::from(data[30]);
    let mime_len_pos = 31 + name_len;
    if mime_len_pos >= data.len() {
        return Ok(None);
    }
    let mime_len = usize::from(data[mime_len_pos]);
    let mime_start = mime_len_pos + 1;
    let mime_end = mime_start + mime_len;
    if mime_end + 4 > data.len() {
        return Ok(None);
    }
    let mime = &data[mime_start..mime_end];
    let type_len = usize::try_from(read_u32_be(data, mime_end)?).map_err(|_| {
        RmpegError::InvalidData("RealMedia type-specific data is too large".to_string())
    })?;
    let type_start = mime_end + 4;
    let type_end = type_start + type_len;
    if type_end > data.len() {
        return Ok(None);
    }
    let type_data = &data[type_start..type_end];

    if mime == b"audio/x-pn-realaudio" {
        return parse_audio_type_data(type_data, index, duration_seconds).map(Some);
    }
    if mime == b"video/x-pn-realvideo" {
        return Ok(parse_video_type_data(type_data, index, duration_seconds));
    }
    Ok(None)
}

fn parse_audio_type_data(
    data: &[u8],
    index: usize,
    duration_seconds: f64,
) -> Result<StreamMetadata> {
    let (codec_name, sample_rate, channels) = if data.windows(4).any(|tag| tag == b"sipr") {
        (
            "sipr",
            if data.windows(2).any(|value| value == [0x3e, 0x80]) {
                16_000
            } else {
                8_000
            },
            1,
        )
    } else if let Some(genr_pos) = find_bytes(data, b"genr") {
        let channels = data.get(genr_pos.saturating_sub(1)).copied().unwrap_or(1);
        ("cook", 44_100, u16::from(channels.max(1)))
    } else if data.windows(4).any(|tag| tag == b"28_8") {
        ("ra_288", 8_000, 1)
    } else if data.windows(4).any(|tag| tag == b"lpcJ") {
        ("ra_144", 8_000, 1)
    } else {
        return Err(RmpegError::InvalidData(
            "unsupported RealAudio codec".to_string(),
        ));
    };

    Ok(StreamMetadata::audio(
        index,
        codec_name,
        sample_rate,
        channels,
        0,
        duration_seconds,
    ))
}

fn parse_video_type_data(
    data: &[u8],
    index: usize,
    duration_seconds: f64,
) -> Option<StreamMetadata> {
    let tag = find_bytes(data, b"VIDORV")?;
    if tag + 12 > data.len() {
        return None;
    }
    let codec_name = match &data[tag + 6..tag + 8] {
        b"10" => "rv10",
        b"20" => "rv20",
        b"30" => "rv30",
        b"40" => "rv40",
        _ => return None,
    };
    let width = u32::from(read_u16_be(data, tag + 8).ok()?);
    let height = u32::from(read_u16_be(data, tag + 10).ok()?);
    Some(StreamMetadata::video(
        index,
        codec_name,
        width,
        height,
        Some(duration_seconds),
        None,
    ))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn read_u16_be(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_u32_be(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_realvideo_type_data() {
        let data = b"\0\0\0\0VIDORV20\x01\x40\0\xf0";
        let stream = parse_video_type_data(data, 0, 1.25).expect("valid video");
        assert_eq!(stream.codec_name, "rv20");
        assert_eq!(stream.width, Some(320));
        assert_eq!(stream.height, Some(240));
        assert_eq!(stream.duration_seconds, Some(1.25));
    }

    #[test]
    fn parses_sipr_audio_type_data() {
        let stream =
            parse_audio_type_data(b".ra\xfd\0\0\x3e\x80siprsipr", 0, 2.0).expect("valid audio");
        assert_eq!(stream.codec_name, "sipr");
        assert_eq!(stream.sample_rate, Some(16_000));
    }

    #[test]
    fn parses_old_ra_288_duration_from_frames() {
        let mut bytes = vec![0; 73 + 76];
        bytes[0..4].copy_from_slice(b".ra\xfd");
        bytes[24..28].copy_from_slice(&38_u32.to_be_bytes());
        bytes[62..66].copy_from_slice(b"28_8");

        let doc = parse_realaudio(&bytes).expect("valid old realaudio");
        assert_eq!(doc.streams[0].codec_name, "ra_288");
        assert_eq!(doc.streams[0].duration_seconds, Some(0.04));
    }
}
