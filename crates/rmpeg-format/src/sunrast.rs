use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_sunrast(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 32 {
        return Err(RmpegError::UnexpectedEof {
            needed: 32,
            remaining: bytes.len(),
        });
    }
    if &bytes[0..4] != b"\x59\xa6\x6a\x95" {
        return Err(RmpegError::InvalidData(
            "missing Sun Raster signature".to_string(),
        ));
    }

    let width = read_u32_be(bytes, 4)?;
    let height = read_u32_be(bytes, 8)?;
    let depth = read_u32_be(bytes, 12)?;
    let raster_type = read_u32_be(bytes, 20)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "Sun Raster dimensions must be nonzero".to_string(),
        ));
    }
    if !matches!(depth, 1 | 4 | 8 | 15 | 16 | 24 | 32) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported Sun Raster depth {depth}"
        )));
    }
    if !matches!(raster_type, 0..=5) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported Sun Raster type {raster_type}"
        )));
    }

    Ok(ProbeDocument {
        format: "sunrast_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "sunrast",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_sunrast(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x59\xa6\x6a\x95")
}

fn read_u32_be(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_sunrast(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0; 32];
        bytes[0..4].copy_from_slice(b"\x59\xa6\x6a\x95");
        bytes[4..8].copy_from_slice(&width.to_be_bytes());
        bytes[8..12].copy_from_slice(&height.to_be_bytes());
        bytes[12..16].copy_from_slice(&24_u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&1_u32.to_be_bytes());
        bytes
    }

    #[test]
    fn parses_sunrast_dimensions() {
        let doc = parse_sunrast(&minimal_sunrast(512, 256)).expect("valid sunrast");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "sunrast_pipe");
        assert_eq!(stream.codec_name, "sunrast");
        assert_eq!(stream.width, Some(512));
        assert_eq!(stream.height, Some(256));
    }

    #[test]
    fn rejects_bad_depth() {
        let mut bytes = minimal_sunrast(1, 1);
        bytes[12..16].copy_from_slice(&3_u32.to_be_bytes());
        let err = parse_sunrast(&bytes).expect_err("bad depth");
        assert!(err.to_string().contains("depth"));
    }
}
