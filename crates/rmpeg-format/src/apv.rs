use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_apv(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_apv(bytes) {
        return Err(RmpegError::InvalidData("missing APV header".to_string()));
    }
    let width = u32::from(read_u16_be(bytes, 0x14)?);
    let height = u32::from(read_u16_be(bytes, 0x17)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid APV dimensions".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "apv".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "apv",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_apv(bytes: &[u8]) -> bool {
    bytes.len() >= 0x1b && bytes.get(4..8) == Some(b"aPv1")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_apv_dimensions() {
        let mut bytes = vec![0; 0x1b];
        bytes[4..8].copy_from_slice(b"aPv1");
        bytes[0x14..0x16].copy_from_slice(&320_u16.to_be_bytes());
        bytes[0x17..0x19].copy_from_slice(&180_u16.to_be_bytes());

        let doc = parse_apv(&bytes).expect("apv");

        assert_eq!(doc.format, "apv");
        assert_eq!(doc.streams[0].codec_name, "apv");
        assert_eq!(doc.streams[0].width, Some(320));
        assert_eq!(doc.streams[0].height, Some(180));
    }
}
