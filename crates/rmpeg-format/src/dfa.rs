use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_dfa(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_dfa(bytes) {
        return Err(RmpegError::InvalidData(
            "missing Chronomaster DFA header".to_string(),
        ));
    }

    let frame_count = u32::from(read_u16(bytes, 6)?);
    let width = u32::from(read_u16(bytes, 8)?);
    let height = u32::from(read_u16(bytes, 10)?);
    let milliseconds_per_frame = u32::from(read_u16(bytes, 12)?);
    if frame_count == 0 || width == 0 || height == 0 || milliseconds_per_frame == 0 {
        return Err(RmpegError::InvalidData(
            "invalid Chronomaster DFA stream metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "dfa".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "dfa",
            width,
            height,
            Some(frame_count as f64 * milliseconds_per_frame as f64 / 1000.0),
            None,
        )],
    })
}

pub fn looks_like_dfa(bytes: &[u8]) -> bool {
    bytes.len() >= 14 && bytes.starts_with(b"DFIA") && bytes[4] == 0 && bytes[5] == 0
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
    fn parses_observed_chronomaster_header_metadata() {
        let mut bytes = vec![0; 64];
        bytes[0..4].copy_from_slice(b"DFIA");
        bytes[6..8].copy_from_slice(&160_u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&640_u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&480_u16.to_le_bytes());
        bytes[12..14].copy_from_slice(&128_u16.to_le_bytes());

        let doc = parse_dfa(&bytes).expect("dfa");

        assert_eq!(doc.format, "dfa");
        assert_eq!(doc.streams[0].codec_name, "dfa");
        assert_eq!(doc.streams[0].width, Some(640));
        assert_eq!(doc.streams[0].height, Some(480));
        assert_eq!(doc.streams[0].duration_seconds, Some(20.48));
    }

    #[test]
    fn rejects_zero_dimensions() {
        let mut bytes = vec![0; 14];
        bytes[0..4].copy_from_slice(b"DFIA");
        bytes[6..8].copy_from_slice(&1_u16.to_le_bytes());
        bytes[12..14].copy_from_slice(&100_u16.to_le_bytes());

        assert!(parse_dfa(&bytes).is_err());
    }
}
