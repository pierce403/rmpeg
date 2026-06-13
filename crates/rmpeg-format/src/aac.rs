use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy)]
struct AdtsFrame {
    frame_len: usize,
    sample_rate: u32,
    channels: u16,
    samples: u32,
}

pub fn parse_adts_aac(bytes: &[u8]) -> Result<ProbeDocument> {
    let mut pos = id3v2_skip(bytes)?;
    let mut first = None;
    let mut samples = 0_u64;

    while pos + 7 <= bytes.len() {
        if bytes.get(pos..pos + 3) == Some(b"ID3") {
            pos += id3v2_skip(&bytes[pos..])?;
            continue;
        }
        let frame = parse_adts_header(&bytes[pos..pos + 7])
            .ok_or_else(|| RmpegError::InvalidData("invalid ADTS AAC frame".to_string()))?;
        if pos + frame.frame_len > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + frame.frame_len,
                remaining: bytes.len(),
            });
        }
        first.get_or_insert(frame);
        samples += u64::from(frame.samples);
        pos += frame.frame_len;
    }

    let first =
        first.ok_or_else(|| RmpegError::InvalidData("no ADTS AAC frames found".to_string()))?;
    let duration_seconds = samples as f64 / first.sample_rate as f64;
    Ok(ProbeDocument {
        format: "aac".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "aac",
            first.sample_rate,
            first.channels,
            0,
            duration_seconds,
        )],
    })
}

pub fn looks_like_adts_aac(bytes: &[u8]) -> bool {
    let Ok(pos) = id3v2_skip(bytes) else {
        return false;
    };
    pos + 7 <= bytes.len() && parse_adts_header(&bytes[pos..pos + 7]).is_some()
}

fn parse_adts_header(header: &[u8]) -> Option<AdtsFrame> {
    if header.len() < 7 {
        return None;
    }
    if header[0] != 0xff || (header[1] & 0xf0) != 0xf0 || (header[1] & 0x06) != 0 {
        return None;
    }
    let sample_rate = sample_rate(usize::from((header[2] & 0x3c) >> 2))?;
    let channel_config = ((header[2] & 0x01) << 2) | ((header[3] & 0xc0) >> 6);
    let channels = channels(channel_config)?;
    let frame_len = ((usize::from(header[3] & 0x03)) << 11)
        | (usize::from(header[4]) << 3)
        | usize::from((header[5] & 0xe0) >> 5);
    let header_len = if header[1] & 0x01 != 0 { 7 } else { 9 };
    if frame_len < header_len {
        return None;
    }
    let samples = (u32::from(header[6] & 0x03) + 1) * 1024;
    Some(AdtsFrame {
        frame_len,
        sample_rate,
        channels,
        samples,
    })
}

fn sample_rate(index: usize) -> Option<u32> {
    [
        96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025,
        8_000, 7_350,
    ]
    .get(index)
    .copied()
}

fn channels(config: u8) -> Option<u16> {
    match config {
        1 => Some(1),
        2 => Some(2),
        3 => Some(3),
        4 => Some(4),
        5 => Some(5),
        6 => Some(6),
        7 => Some(8),
        _ => None,
    }
}

fn id3v2_skip(bytes: &[u8]) -> Result<usize> {
    let mut pos = 0;
    while bytes.get(pos..pos + 3) == Some(b"ID3") {
        if bytes.len().saturating_sub(pos) < 10 {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 10,
                remaining: bytes.len(),
            });
        }
        if bytes[pos + 6..pos + 10].iter().any(|byte| byte & 0x80 != 0) {
            return Err(RmpegError::InvalidData(
                "invalid ID3 synchsafe size".to_string(),
            ));
        }
        let size = ((usize::from(bytes[pos + 6])) << 21)
            | ((usize::from(bytes[pos + 7])) << 14)
            | ((usize::from(bytes[pos + 8])) << 7)
            | usize::from(bytes[pos + 9]);
        let footer = if bytes[pos + 5] & 0x10 != 0 { 10 } else { 0 };
        pos = pos
            .checked_add(10 + size + footer)
            .ok_or_else(|| RmpegError::InvalidData("ID3 size overflow".to_string()))?;
    }
    Ok(pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADTS_LC_44K_STEREO_EMPTY_FRAME: [u8; 7] = [0xff, 0xf1, 0x50, 0x80, 0x00, 0xff, 0xfc];

    #[test]
    fn parses_minimal_adts_frame() {
        let doc = parse_adts_aac(&ADTS_LC_44K_STEREO_EMPTY_FRAME).expect("valid adts");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "aac");
        assert_eq!(stream.codec_name, "aac");
        assert_eq!(stream.sample_rate, Some(44_100));
        assert_eq!(stream.channels, Some(2));
        assert!(
            (stream.duration_seconds.expect("duration") - (1024.0 / 44_100.0)).abs() < f64::EPSILON
        );
    }

    #[test]
    fn skips_id3v2_tag_before_adts() {
        let mut bytes = b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec();
        bytes.extend_from_slice(&ADTS_LC_44K_STEREO_EMPTY_FRAME);

        assert!(looks_like_adts_aac(&bytes));
        parse_adts_aac(&bytes).expect("adts after id3");
    }
}
