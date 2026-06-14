use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_cine(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_cine(bytes) {
        return Err(RmpegError::InvalidData("missing CINE header".to_string()));
    }
    let width = read_u32_le(bytes, 0x30)?;
    let height = read_u32_le(bytes, 0x34)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid CINE dimensions".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "cine".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "rawvideo",
            width,
            height,
            Some(1.0 / 31.0),
            None,
        )],
    })
}

pub fn looks_like_cine(bytes: &[u8]) -> bool {
    bytes.len() >= 0x38 && bytes.starts_with(b"CI,")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_cine_dimensions() {
        let mut bytes = b"CI,".to_vec();
        bytes.resize(0x38, 0);
        bytes[0x30..0x34].copy_from_slice(&1280_u32.to_le_bytes());
        bytes[0x34..0x38].copy_from_slice(&800_u32.to_le_bytes());

        let doc = parse_cine(&bytes).expect("cine");

        assert_eq!(doc.format, "cine");
        assert_eq!(doc.streams[0].codec_name, "rawvideo");
        assert_eq!(doc.streams[0].width, Some(1280));
        assert_eq!(doc.streams[0].duration_seconds, Some(1.0 / 31.0));
    }
}
