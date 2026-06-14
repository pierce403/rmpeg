use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_ast(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_ast(bytes) {
        return Err(RmpegError::InvalidData("missing AST header".to_string()));
    }

    let channels = read_u16_be(bytes, 12)?;
    let sample_rate = read_u32_be(bytes, 16)?;
    let sample_count = read_u32_be(bytes, 20)?;
    if channels == 0 || sample_rate == 0 || sample_count == 0 {
        return Err(RmpegError::InvalidData(
            "invalid AST stream metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "ast".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "adpcm_afc",
            sample_rate,
            channels,
            0,
            sample_count as f64 / sample_rate as f64,
        )],
    })
}

pub fn looks_like_ast(bytes: &[u8]) -> bool {
    bytes.len() >= 24 && bytes.starts_with(b"STRM")
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

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
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
    fn parses_ast_duration_from_sample_count() {
        let mut bytes = b"STRM".to_vec();
        bytes.resize(24, 0);
        bytes[12..14].copy_from_slice(&2_u16.to_be_bytes());
        bytes[16..20].copy_from_slice(&44_100_u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&2_656_672_u32.to_be_bytes());

        let doc = parse_ast(&bytes).expect("ast");

        assert_eq!(doc.format, "ast");
        assert_eq!(doc.streams[0].codec_name, "adpcm_afc");
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(
            doc.streams[0].duration_seconds,
            Some(2_656_672.0 / 44_100.0)
        );
    }
}
