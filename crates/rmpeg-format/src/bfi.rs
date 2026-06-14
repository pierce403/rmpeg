use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_bfi(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_bfi(bytes) {
        return Err(RmpegError::InvalidData("missing BFI header".to_string()));
    }
    let frames = read_u32_le(bytes, 0x0c)?;
    let fps = read_u32_le(bytes, 0x1c)?;
    let width = read_u32_le(bytes, 0x2c)?;
    let height = read_u32_le(bytes, 0x30)?;
    if frames == 0 || fps == 0 || width == 0 || height == 0 {
        return Err(RmpegError::InvalidData("invalid BFI metadata".to_string()));
    }
    let duration = frames as f64 / fps as f64;

    Ok(ProbeDocument {
        format: "bfi".to_string(),
        streams: vec![
            StreamMetadata::video(0, "bfi", width, height, Some(duration), None),
            StreamMetadata::audio(1, "pcm_u8", 11_025, 1, 8, 0.0),
        ],
    })
}

pub fn looks_like_bfi(bytes: &[u8]) -> bool {
    bytes.len() >= 0x34 && bytes.starts_with(b"BF&I")
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
    fn parses_observed_bfi_header() {
        let mut bytes = vec![0; 0x34];
        bytes[0..4].copy_from_slice(b"BF&I");
        bytes[0x0c..0x10].copy_from_slice(&57_u32.to_le_bytes());
        bytes[0x1c..0x20].copy_from_slice(&9_u32.to_le_bytes());
        bytes[0x2c..0x30].copy_from_slice(&320_u32.to_le_bytes());
        bytes[0x30..0x34].copy_from_slice(&140_u32.to_le_bytes());

        let doc = parse_bfi(&bytes).expect("bfi");

        assert_eq!(doc.streams[0].codec_name, "bfi");
        assert_eq!(doc.streams[0].duration_seconds, Some(57.0 / 9.0));
        assert_eq!(doc.streams[1].codec_name, "pcm_u8");
    }
}
