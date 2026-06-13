use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_sgi(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 12 {
        return Err(RmpegError::UnexpectedEof {
            needed: 12,
            remaining: bytes.len(),
        });
    }
    if &bytes[0..2] != b"\x01\xda" {
        return Err(RmpegError::InvalidData("missing SGI signature".to_string()));
    }

    let storage = bytes[2];
    if storage > 1 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported SGI storage mode {storage}"
        )));
    }
    let bytes_per_channel = bytes[3];
    if !matches!(bytes_per_channel, 1 | 2) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported SGI bytes per channel {bytes_per_channel}"
        )));
    }
    let dimensions = read_u16_be(bytes, 4)?;
    if !(1..=3).contains(&dimensions) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported SGI dimension count {dimensions}"
        )));
    }

    let width = u32::from(read_u16_be(bytes, 6)?);
    let height = u32::from(read_u16_be(bytes, 8)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "SGI dimensions must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "sgi_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "sgi",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_sgi(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x01\xda")
}

fn read_u16_be(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[pos], bytes[pos + 1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_sgi(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0; 512];
        bytes[0..2].copy_from_slice(b"\x01\xda");
        bytes[2] = 1;
        bytes[3] = 1;
        bytes[4..6].copy_from_slice(&3_u16.to_be_bytes());
        bytes[6..8].copy_from_slice(&width.to_be_bytes());
        bytes[8..10].copy_from_slice(&height.to_be_bytes());
        bytes[10..12].copy_from_slice(&3_u16.to_be_bytes());
        bytes
    }

    #[test]
    fn parses_sgi_dimensions() {
        let doc = parse_sgi(&minimal_sgi(256, 128)).expect("valid sgi");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "sgi_pipe");
        assert_eq!(stream.codec_name, "sgi");
        assert_eq!(stream.width, Some(256));
        assert_eq!(stream.height, Some(128));
    }

    #[test]
    fn rejects_unknown_storage_mode() {
        let mut bytes = minimal_sgi(1, 1);
        bytes[2] = 2;
        let err = parse_sgi(&bytes).expect_err("bad storage mode");
        assert!(err.to_string().contains("storage mode"));
    }
}
