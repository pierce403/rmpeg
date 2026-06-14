use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_pict(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 10 {
        return Err(RmpegError::UnexpectedEof {
            needed: 10,
            remaining: bytes.len(),
        });
    }

    let declared_size = usize::from(read_u16_be(bytes, 0)?);
    let top = i32::from(read_i16_be(bytes, 2)?);
    let left = i32::from(read_i16_be(bytes, 4)?);
    let bottom = i32::from(read_i16_be(bytes, 6)?);
    let right = i32::from(read_i16_be(bytes, 8)?);
    if declared_size == 0 || bottom <= top || right <= left {
        return Err(RmpegError::InvalidData(
            "invalid QuickDraw PICT bounds".to_string(),
        ));
    }
    let width = u32::try_from(right - left)
        .map_err(|_| RmpegError::InvalidData("invalid PICT width".to_string()))?;
    let height = u32::try_from(bottom - top)
        .map_err(|_| RmpegError::InvalidData("invalid PICT height".to_string()))?;

    Ok(ProbeDocument {
        format: "image2".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "qdraw",
            width,
            height,
            Some(0.04),
            Some("25/1".to_string()),
        )],
    })
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

fn read_i16_be(bytes: &[u8], offset: usize) -> Result<i16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(i16::from_be_bytes([bytes[offset], bytes[offset + 1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extension_gated_pict_bounds() {
        let mut bytes = vec![0; 32];
        bytes[0..2].copy_from_slice(&0x15fc_u16.to_be_bytes());
        bytes[6..8].copy_from_slice(&64_i16.to_be_bytes());
        bytes[8..10].copy_from_slice(&256_i16.to_be_bytes());

        let doc = parse_pict(&bytes).expect("pict");

        assert_eq!(doc.format, "image2");
        assert_eq!(doc.streams[0].codec_name, "qdraw");
        assert_eq!(doc.streams[0].width, Some(256));
        assert_eq!(doc.streams[0].height, Some(64));
    }

    #[test]
    fn rejects_inverted_bounds() {
        let mut bytes = vec![0; 10];
        bytes[0..2].copy_from_slice(&1_u16.to_be_bytes());
        bytes[2..4].copy_from_slice(&10_i16.to_be_bytes());
        bytes[6..8].copy_from_slice(&5_i16.to_be_bytes());
        bytes[8..10].copy_from_slice(&1_i16.to_be_bytes());

        assert!(parse_pict(&bytes).is_err());
    }
}
