use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_mlv(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_mlv(bytes) {
        return Err(RmpegError::InvalidData("missing MLV header".to_string()));
    }
    let Some(rawi) = find_bytes(bytes, b"RAWI") else {
        return Err(RmpegError::InvalidData(
            "missing MLV RAWI block".to_string(),
        ));
    };
    let width = u32::from(read_u16_le(bytes, rawi + 0x10)?);
    let height = u32::from(read_u16_le(bytes, rawi + 0x12)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid MLV dimensions".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "mlv".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "rawvideo",
            width,
            height,
            Some(1001.0 / 60_000.0),
            None,
        )],
    })
}

pub fn looks_like_mlv(bytes: &[u8]) -> bool {
    bytes.len() >= 16 && bytes.starts_with(b"MLVI")
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rawi_dimensions() {
        let mut bytes = b"MLVI".to_vec();
        bytes.resize(0x34, 0);
        bytes.extend_from_slice(b"RAWI");
        bytes.resize(0x34 + 0x18, 0);
        bytes[0x34 + 0x10..0x34 + 0x12].copy_from_slice(&1472_u16.to_le_bytes());
        bytes[0x34 + 0x12..0x34 + 0x14].copy_from_slice(&610_u16.to_le_bytes());

        let doc = parse_mlv(&bytes).expect("mlv");

        assert_eq!(doc.format, "mlv");
        assert_eq!(doc.streams[0].codec_name, "rawvideo");
        assert_eq!(doc.streams[0].height, Some(610));
        assert_eq!(doc.streams[0].duration_seconds, Some(1001.0 / 60_000.0));
    }
}
