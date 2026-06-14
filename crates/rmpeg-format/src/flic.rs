use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_flic(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_flic(bytes) {
        return Err(RmpegError::InvalidData("missing FLIC header".to_string()));
    }
    if bytes.len() < 12 {
        return Err(RmpegError::UnexpectedEof {
            needed: 12,
            remaining: bytes.len(),
        });
    }
    let declared_size = read_u32_le(bytes, 0)? as usize;
    if declared_size != 0 && declared_size > bytes.len() + 16 {
        return Err(RmpegError::InvalidData(
            "FLIC declared size is implausible".to_string(),
        ));
    }
    let width = u32::from(read_u16_le(bytes, 8)?);
    let height = u32::from(read_u16_le(bytes, 10)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "FLIC dimensions must be nonzero".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "flic".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "flic",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_flic(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && matches!(read_u16_le(bytes, 4).ok(), Some(0xaf11 | 0xaf12))
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

fn read_u32_le(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
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
    fn parses_flic_dimensions() {
        let mut bytes = vec![0; 128];
        bytes[0..4].copy_from_slice(&128_u32.to_le_bytes());
        bytes[4..6].copy_from_slice(&0xaf12_u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&320_u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&200_u16.to_le_bytes());

        let doc = parse_flic(&bytes).expect("valid flic");
        assert_eq!(doc.streams[0].width, Some(320));
        assert_eq!(doc.streams[0].height, Some(200));
    }
}
