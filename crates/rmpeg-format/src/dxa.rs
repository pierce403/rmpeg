use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_dxa(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_dxa(bytes) {
        return Err(RmpegError::InvalidData("missing DXA header".to_string()));
    }

    let flags = bytes[4];
    let frames = u32::from(read_u16_be(bytes, 5)?);
    let frame_ticks = read_i32_be(bytes, 7)?.unsigned_abs();
    let width = u32::from(read_u16_be(bytes, 11)?);
    let mut height = u32::from(read_u16_be(bytes, 13)?);
    if flags & 0x40 != 0 {
        height /= 2;
    }
    if frames == 0 || frame_ticks == 0 || width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid DXA video metadata".to_string(),
        ));
    }
    let duration = frames as f64 * frame_ticks as f64 / 100_000.0;

    let mut streams = vec![StreamMetadata::video(
        0,
        "dxa",
        width,
        height,
        Some(duration),
        None,
    )];
    if let Some(audio) = parse_embedded_wave(bytes, streams.len(), duration)? {
        streams.push(audio);
    }

    Ok(ProbeDocument {
        format: "dxa".to_string(),
        streams,
    })
}

pub fn looks_like_dxa(bytes: &[u8]) -> bool {
    bytes.len() >= 15 && bytes.starts_with(b"DEXA")
}

fn parse_embedded_wave(
    bytes: &[u8],
    index: usize,
    duration: f64,
) -> Result<Option<StreamMetadata>> {
    if bytes.get(15..19) != Some(b"WAVE") {
        return Ok(None);
    }
    let Some(riff_pos) = find_bytes(bytes, b"RIFF") else {
        return Ok(None);
    };
    if bytes.get(riff_pos + 8..riff_pos + 12) != Some(b"WAVE") {
        return Ok(None);
    }
    let Some(fmt_pos) = find_bytes(&bytes[riff_pos + 12..], b"fmt ") else {
        return Ok(None);
    };
    let fmt_chunk = riff_pos + 12 + fmt_pos;
    let chunk_size = read_u32_le(bytes, fmt_chunk + 4)? as usize;
    let data_start = fmt_chunk + 8;
    if chunk_size < 16 || data_start + 16 > bytes.len() {
        return Ok(None);
    }
    let format_tag = read_u16_le(bytes, data_start)?;
    let channels = read_u16_le(bytes, data_start + 2)?;
    let sample_rate = read_u32_le(bytes, data_start + 4)?;
    let bits_per_sample = read_u16_le(bytes, data_start + 14)?;
    if format_tag != 0x0002 || channels == 0 || sample_rate == 0 {
        return Ok(None);
    }

    Ok(Some(StreamMetadata::audio(
        index,
        "adpcm_ms",
        sample_rate,
        channels,
        bits_per_sample,
        duration,
    )))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
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

fn read_i32_be(bytes: &[u8], offset: usize) -> Result<i32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(i32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_half_height_dxa_header() {
        let mut bytes = b"DEXA".to_vec();
        bytes.extend_from_slice(&[
            0x40, 0x02, 0x7d, 0xff, 0xff, 0xdf, 0x73, 0x02, 0x80, 0x01, 0x90,
        ]);

        let doc = parse_dxa(&bytes).expect("dxa");

        assert_eq!(doc.format, "dxa");
        assert_eq!(doc.streams[0].codec_name, "dxa");
        assert_eq!(doc.streams[0].width, Some(640));
        assert_eq!(doc.streams[0].height, Some(200));
        assert_eq!(
            doc.streams[0].duration_seconds,
            Some(637.0 * 8_333.0 / 100_000.0)
        );
    }

    #[test]
    fn parses_embedded_adpcm_ms_audio() {
        let mut bytes = b"DEXA".to_vec();
        bytes.extend_from_slice(&[
            0x00, 0x01, 0xbc, 0xff, 0xff, 0xd8, 0xf0, 0x02, 0x80, 0x01, 0xe0,
        ]);
        bytes.extend_from_slice(b"WAVE\0\0\0\0RIFF\0\0\0\0WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&0x0002_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&11_025_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&4_u16.to_le_bytes());

        let doc = parse_dxa(&bytes).expect("dxa");

        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[1].codec_name, "adpcm_ms");
        assert_eq!(doc.streams[1].sample_rate, Some(11_025));
        assert_eq!(doc.streams[1].duration_seconds, Some(44.4));
    }
}
