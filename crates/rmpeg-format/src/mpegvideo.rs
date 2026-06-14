use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_mpeg_video(bytes: &[u8]) -> Result<ProbeDocument> {
    let start = sequence_header_start(bytes)
        .ok_or_else(|| RmpegError::InvalidData("missing MPEG video sequence header".to_string()))?;
    parse_mpeg_video_at(bytes, start, "mpegvideo", None)
}

pub fn parse_mpeg_video_payload(
    bytes: &[u8],
    format: &str,
    duration_seconds: Option<f64>,
) -> Result<ProbeDocument> {
    let start = find_sequence_header(bytes).ok_or_else(|| {
        RmpegError::InvalidData("missing MPEG video payload sequence header".to_string())
    })?;
    parse_mpeg_video_at(bytes, start, format, duration_seconds)
}

fn parse_mpeg_video_at(
    bytes: &[u8],
    start: usize,
    format: &str,
    duration_seconds: Option<f64>,
) -> Result<ProbeDocument> {
    let header = parse_sequence_header(bytes, start)?;
    let duration_seconds = duration_seconds
        .or_else(|| {
            header
                .bit_rate
                .map(|bit_rate| bytes.len() as f64 * 8.0 / bit_rate as f64)
        })
        .unwrap_or(0.0);

    Ok(ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "mpeg2video",
            header.width,
            header.height,
            Some(duration_seconds),
            header.frame_rate,
        )],
    })
}

pub fn looks_like_mpeg_video(bytes: &[u8]) -> bool {
    sequence_header_start(bytes).is_some()
}

#[derive(Debug)]
struct SequenceHeader {
    width: u32,
    height: u32,
    bit_rate: Option<u32>,
    frame_rate: Option<String>,
}

fn sequence_header_start(bytes: &[u8]) -> Option<usize> {
    let scan = bytes.len().min(16);
    (0..scan.saturating_sub(3)).find(|&pos| is_sequence_header(bytes, pos))
}

fn find_sequence_header(bytes: &[u8]) -> Option<usize> {
    let scan = bytes.len().min(1_048_576);
    (0..scan.saturating_sub(3)).find(|&pos| is_sequence_header(bytes, pos))
}

fn is_sequence_header(bytes: &[u8], pos: usize) -> bool {
    pos + 4 <= bytes.len()
        && bytes[pos] == 0
        && bytes[pos + 1] == 0
        && bytes[pos + 2] == 1
        && bytes[pos + 3] == 0xb3
}

fn parse_sequence_header(bytes: &[u8], start: usize) -> Result<SequenceHeader> {
    if start + 12 > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: start + 12,
            remaining: bytes.len(),
        });
    }
    let width = (u32::from(bytes[start + 4]) << 4) | u32::from(bytes[start + 5] >> 4);
    let height = (u32::from(bytes[start + 5] & 0x0f) << 8) | u32::from(bytes[start + 6]);
    let frame_rate_code = bytes[start + 7] & 0x0f;
    let bit_rate_value = (u32::from(bytes[start + 8]) << 10)
        | (u32::from(bytes[start + 9]) << 2)
        | u32::from(bytes[start + 10] >> 6);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "MPEG video dimensions must be nonzero".to_string(),
        ));
    }
    Ok(SequenceHeader {
        width,
        height,
        bit_rate: if bit_rate_value == 0 || bit_rate_value == 25_000 || bit_rate_value == 0x3ffff {
            None
        } else {
            bit_rate_value.checked_mul(400)
        },
        frame_rate: frame_rate(frame_rate_code),
    })
}

fn frame_rate(code: u8) -> Option<String> {
    match code {
        1 => Some("24000/1001".to_string()),
        2 => Some("24/1".to_string()),
        3 => Some("25/1".to_string()),
        4 => Some("30000/1001".to_string()),
        5 => Some("30/1".to_string()),
        6 => Some("50/1".to_string()),
        7 => Some("60000/1001".to_string()),
        8 => Some("60/1".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_sequence_header_dimensions_and_duration() {
        let mut bytes = vec![
            0x00, 0x00, 0x01, 0xb3, 0x2d, 0x02, 0x60, 0x33, 0x7a, 0x12, 0x32,
        ];
        bytes.resize(930_186, 0);

        let doc = parse_mpeg_video(&bytes).expect("mpeg video");

        assert_eq!(doc.format, "mpegvideo");
        assert_eq!(doc.streams[0].codec_name, "mpeg2video");
        assert_eq!(doc.streams[0].width, Some(720));
        assert_eq!(doc.streams[0].height, Some(608));
        assert_eq!(doc.streams[0].frame_rate, Some("25/1".to_string()));
        let duration = doc.streams[0].duration_seconds.unwrap();
        assert!((duration - 0.14882976).abs() < 0.000001);
    }
}
