use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_pictor(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_pictor(bytes) {
        return Err(RmpegError::InvalidData("missing Pictor header".to_string()));
    }
    let width = u32::from(read_u16_le(bytes, 2)?);
    let height = u32::from(read_u16_le(bytes, 4)?);

    Ok(ProbeDocument {
        format: "image2".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "pictor",
            width,
            height,
            Some(0.04),
            None,
        )],
    })
}

pub fn looks_like_pictor(bytes: &[u8]) -> bool {
    if bytes.len() < 6 || read_u16_le_lossy(bytes, 0) != Some(0x1234) {
        return false;
    }
    let Some(width) = read_u16_le_lossy(bytes, 2) else {
        return false;
    };
    let Some(height) = read_u16_le_lossy(bytes, 4) else {
        return false;
    };
    width != 0 && height != 0
}

fn read_u16_le_lossy(bytes: &[u8], pos: usize) -> Option<u16> {
    let end = pos.checked_add(2)?;
    if end > bytes.len() {
        return None;
    }
    Some(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_u16_le(bytes: &[u8], pos: usize) -> Result<u16> {
    read_u16_le_lossy(bytes, pos).ok_or(RmpegError::UnexpectedEof {
        needed: pos + 2,
        remaining: bytes.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pictor_dimensions() {
        let mut bytes = vec![0; 16];
        bytes[0..2].copy_from_slice(&0x1234_u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&312_u16.to_le_bytes());
        bytes[4..6].copy_from_slice(&206_u16.to_le_bytes());

        let doc = parse_pictor(&bytes).expect("pictor");

        assert_eq!(doc.format, "image2");
        assert_eq!(doc.streams[0].codec_name, "pictor");
        assert_eq!(doc.streams[0].width, Some(312));
        assert_eq!(doc.streams[0].height, Some(206));
    }
}
