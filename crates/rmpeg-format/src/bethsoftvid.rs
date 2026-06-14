use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_bethsoftvid(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_bethsoftvid(bytes) {
        return Err(RmpegError::InvalidData(
            "missing Bethesda VID signature".to_string(),
        ));
    }
    if bytes.len() < 11 {
        return Err(RmpegError::UnexpectedEof {
            needed: 11,
            remaining: bytes.len(),
        });
    }
    let width = u32::from(read_u16_le(bytes, 7)?);
    let height = u32::from(read_u16_le(bytes, 9)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "Bethesda VID dimensions must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "bethsoftvid".to_string(),
        streams: vec![
            StreamMetadata::audio(0, "pcm_u8", 11_111, 1, 8, 0.0),
            StreamMetadata::video(1, "bethsoftvid", width, height, Some(0.0), None),
        ],
    })
}

pub fn looks_like_bethsoftvid(bytes: &[u8]) -> bool {
    bytes.starts_with(b"VID\0")
}

fn read_u16_le(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bethsoftvid_header() {
        let bytes = b"VID\0\0\0\0\x40\x01\xc8\0".to_vec();
        let doc = parse_bethsoftvid(&bytes).expect("valid bethsoft vid");
        assert_eq!(doc.format, "bethsoftvid");
        assert_eq!(doc.streams[0].codec_name, "pcm_u8");
        assert_eq!(doc.streams[1].width, Some(320));
        assert_eq!(doc.streams[1].height, Some(200));
    }
}
