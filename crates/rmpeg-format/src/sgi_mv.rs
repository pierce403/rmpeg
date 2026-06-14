use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_sgi_mv(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_sgi_mv(bytes) {
        return Err(RmpegError::InvalidData(
            "missing SGI Movie header".to_string(),
        ));
    }

    let audio_tracks = value_for_key(bytes, b"__NUM_A_TRACKS")
        .and_then(parse_u32)
        .unwrap_or(0);
    let mut streams = Vec::new();
    if audio_tracks > 0 {
        let sample_rate = value_for_key(bytes, b"SAMPLE_RATE")
            .and_then(parse_f64)
            .map(|value| value.round() as u32)
            .ok_or_else(|| RmpegError::InvalidData("missing SGI MV sample rate".to_string()))?;
        let channels = value_for_key(bytes, b"NUM_CHANNELS")
            .and_then(parse_u32)
            .ok_or_else(|| RmpegError::InvalidData("missing SGI MV channel count".to_string()))?;
        let sample_width = value_for_key(bytes, b"SAMPLE_WIDTH")
            .and_then(parse_u32)
            .unwrap_or(2);
        streams.push(StreamMetadata::audio(
            0,
            "pcm_s16be",
            sample_rate,
            channels as u16,
            (sample_width * 8) as u16,
            0.0,
        ));
    }

    let width_values = values_for_key(bytes, b"WIDTH");
    let height_values = values_for_key(bytes, b"HEIGHT");
    let compression_values = values_for_key(bytes, b"COMPRESSION");
    let dir_counts = values_for_key(bytes, b"__DIR_COUNT");
    let fps_values = values_for_key(bytes, b"FPS");
    let mut width = width_values
        .last()
        .and_then(|value| parse_u32(value))
        .ok_or_else(|| RmpegError::InvalidData("missing SGI MV width".to_string()))?;
    let mut height = height_values
        .last()
        .and_then(|value| parse_u32(value))
        .ok_or_else(|| RmpegError::InvalidData("missing SGI MV height".to_string()))?;
    let compression = compression_values
        .last()
        .copied()
        .ok_or_else(|| RmpegError::InvalidData("missing SGI MV compression".to_string()))?;
    let codec = match compression.trim() {
        "MVC2" => {
            width = width.saturating_sub(2);
            height = height.saturating_sub(2);
            "mvc2"
        }
        "1" => "mvc1",
        "3" => "sgirle",
        _ => {
            return Err(RmpegError::InvalidData(
                "unsupported SGI MV video compression".to_string(),
            ))
        }
    };
    let frames = dir_counts
        .last()
        .and_then(|value| parse_u32(value))
        .ok_or_else(|| RmpegError::InvalidData("missing SGI MV frame count".to_string()))?;
    let fps = fps_values
        .last()
        .and_then(|value| parse_f64(value))
        .ok_or_else(|| RmpegError::InvalidData("missing SGI MV fps".to_string()))?;
    if width == 0 || height == 0 || frames == 0 || fps <= 0.0 {
        return Err(RmpegError::InvalidData(
            "invalid SGI MV video metadata".to_string(),
        ));
    }
    streams.push(StreamMetadata::video(
        streams.len(),
        codec,
        width,
        height,
        Some(frames as f64 / fps),
        None,
    ));

    Ok(ProbeDocument {
        format: "mv".to_string(),
        streams,
    })
}

pub fn looks_like_sgi_mv(bytes: &[u8]) -> bool {
    bytes.len() >= 32
        && bytes.starts_with(b"MOVI")
        && find_bytes(bytes, b"__NUM_I_TRACKS").is_some()
}

fn values_for_key<'a>(bytes: &'a [u8], key: &[u8]) -> Vec<&'a str> {
    let mut values = Vec::new();
    let mut pos = 0;
    while let Some(found) = find_bytes(&bytes[pos..], key) {
        let key_start = pos + found;
        let value_len_pos = key_start + 16;
        let value_start = value_len_pos + 4;
        if value_start > bytes.len() || key_start + key.len() > key_start + 16 {
            pos = key_start + 1;
            continue;
        }
        if !bytes[key_start + key.len()..key_start + 16]
            .iter()
            .all(|byte| *byte == 0 || *byte == b' ')
        {
            pos = key_start + 1;
            continue;
        }
        let Ok(value_len) = read_u32_be(bytes, value_len_pos).map(|value| value as usize) else {
            pos = key_start + 1;
            continue;
        };
        let value_end = value_start.saturating_add(value_len);
        if value_len == 0 || value_end > bytes.len() {
            pos = key_start + 1;
            continue;
        }
        if let Ok(value) = std::str::from_utf8(&bytes[value_start..value_end]) {
            values.push(value.trim_matches(|c| c == '\0' || c == ' ' || c == '\t'));
        }
        pos = value_end;
    }
    values
}

fn value_for_key<'a>(bytes: &'a [u8], key: &[u8]) -> Option<&'a str> {
    values_for_key(bytes, key).into_iter().next()
}

fn parse_u32(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

fn parse_f64(value: &str) -> Option<f64> {
    value.trim().parse().ok()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &[u8], value: &str) -> Vec<u8> {
        let mut out = vec![b' '; 16];
        out[..key.len()].copy_from_slice(key);
        out[key.len()] = 0;
        let mut value_bytes = value.as_bytes().to_vec();
        value_bytes.push(0);
        out.extend_from_slice(&(value_bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&value_bytes);
        out
    }

    #[test]
    fn parses_video_only_sgi_movie() {
        let mut bytes = b"MOVI".to_vec();
        bytes.extend_from_slice(&[0; 20]);
        bytes.extend_from_slice(&entry(b"__NUM_I_TRACKS", "1"));
        bytes.extend_from_slice(&entry(b"__NUM_A_TRACKS", "0"));
        bytes.extend_from_slice(&entry(b"WIDTH", "384"));
        bytes.extend_from_slice(&entry(b"HEIGHT", "288"));
        bytes.extend_from_slice(&entry(b"COMPRESSION", "1"));
        bytes.extend_from_slice(&entry(b"__DIR_COUNT", "80"));
        bytes.extend_from_slice(&entry(b"FPS", "10.000000"));

        let doc = parse_sgi_mv(&bytes).expect("mv");

        assert_eq!(doc.streams.len(), 1);
        assert_eq!(doc.streams[0].codec_name, "mvc1");
        assert_eq!(doc.streams[0].duration_seconds, Some(8.0));
    }

    #[test]
    fn parses_audio_then_mvc2_video() {
        let mut bytes = b"MOVI".to_vec();
        bytes.extend_from_slice(&[0; 20]);
        bytes.extend_from_slice(&entry(b"__NUM_I_TRACKS", "1"));
        bytes.extend_from_slice(&entry(b"__NUM_A_TRACKS", "1"));
        bytes.extend_from_slice(&entry(b"SAMPLE_WIDTH", "2"));
        bytes.extend_from_slice(&entry(b"SAMPLE_RATE", "16000.000000"));
        bytes.extend_from_slice(&entry(b"NUM_CHANNELS", "1"));
        bytes.extend_from_slice(&entry(b"WIDTH", "170"));
        bytes.extend_from_slice(&entry(b"HEIGHT", "190"));
        bytes.extend_from_slice(&entry(b"COMPRESSION", "MVC2"));
        bytes.extend_from_slice(&entry(b"__DIR_COUNT", "225"));
        bytes.extend_from_slice(&entry(b"FPS", "60.000000"));

        let doc = parse_sgi_mv(&bytes).expect("mv");

        assert_eq!(doc.streams[0].codec_name, "pcm_s16be");
        assert_eq!(doc.streams[1].codec_name, "mvc2");
        assert_eq!(doc.streams[1].width, Some(168));
    }
}
