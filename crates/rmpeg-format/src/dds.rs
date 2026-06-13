use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_dds(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 128 {
        return Err(RmpegError::UnexpectedEof {
            needed: 128,
            remaining: bytes.len(),
        });
    }
    if &bytes[0..4] != b"DDS " {
        return Err(RmpegError::InvalidData("missing DDS signature".to_string()));
    }
    let header_size = read_u32(bytes, 4)?;
    if header_size != 124 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported DDS header size {header_size}"
        )));
    }
    let height = read_u32(bytes, 12)?;
    let width = read_u32(bytes, 16)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "DDS dimensions must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "dds_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "dds",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

fn read_u32(bytes: &[u8], pos: usize) -> Result<u32> {
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
    fn parses_dds_dimensions() {
        let mut bytes = vec![0; 128];
        bytes[0..4].copy_from_slice(b"DDS ");
        bytes[4..8].copy_from_slice(&124_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&80_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&64_u32.to_le_bytes());

        let doc = parse_dds(&bytes).expect("valid dds");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "dds_pipe");
        assert_eq!(stream.codec_name, "dds");
        assert_eq!(stream.width, Some(64));
        assert_eq!(stream.height, Some(80));
    }

    #[test]
    fn rejects_bad_header_size() {
        let mut bytes = vec![0; 128];
        bytes[0..4].copy_from_slice(b"DDS ");
        bytes[4..8].copy_from_slice(&120_u32.to_le_bytes());
        let err = parse_dds(&bytes).expect_err("bad header");
        assert!(err.to_string().contains("header size"));
    }
}
