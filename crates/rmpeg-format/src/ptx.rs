use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_ptx(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_ptx(bytes) {
        return Err(RmpegError::InvalidData("missing PTX header".to_string()));
    }
    let width = u32::from(read_u16_le(bytes, 8)?);
    let height = u32::from(read_u16_le(bytes, 10)?);

    Ok(ProbeDocument {
        format: "image2".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "ptx",
            width,
            height,
            Some(0.04),
            None,
        )],
    })
}

pub fn looks_like_ptx(bytes: &[u8]) -> bool {
    if bytes.len() < 44 || read_u32_le_lossy(bytes, 0) != Some(44) {
        return false;
    }
    let Some(width) = read_u16_le_lossy(bytes, 8) else {
        return false;
    };
    let Some(height) = read_u16_le_lossy(bytes, 10) else {
        return false;
    };
    if width == 0 || height == 0 {
        return false;
    }
    let expected = 44usize.saturating_add(usize::from(width) * usize::from(height) * 2);
    bytes.len() >= expected
}

fn read_u16_le_lossy(bytes: &[u8], pos: usize) -> Option<u16> {
    let end = pos.checked_add(2)?;
    if end > bytes.len() {
        return None;
    }
    Some(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_u32_le_lossy(bytes: &[u8], pos: usize) -> Option<u32> {
    let end = pos.checked_add(4)?;
    if end > bytes.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
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
    fn parses_ptx_dimensions() {
        let mut bytes = vec![0; 44 + 32 * 16 * 2];
        bytes[0..4].copy_from_slice(&44_u32.to_le_bytes());
        bytes[8..10].copy_from_slice(&32_u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&16_u16.to_le_bytes());

        let doc = parse_ptx(&bytes).expect("ptx");

        assert_eq!(doc.format, "image2");
        assert_eq!(doc.streams[0].codec_name, "ptx");
        assert_eq!(doc.streams[0].width, Some(32));
        assert_eq!(doc.streams[0].height, Some(16));
    }
}
