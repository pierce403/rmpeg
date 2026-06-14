use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_sol(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_sol(bytes) {
        return Err(RmpegError::InvalidData("missing SOL header".to_string()));
    }
    let sample_rate = u32::from(read_u16_le(bytes, 6)?);
    let channels = bytes[14];
    if sample_rate == 0 || channels == 0 || channels > 2 {
        return Err(RmpegError::InvalidData(
            "invalid SOL audio metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "sol".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "sol_dpcm",
            sample_rate,
            u16::from(channels),
            0,
            0.0,
        )],
    })
}

pub fn looks_like_sol(bytes: &[u8]) -> bool {
    bytes.len() >= 16 && bytes.get(0..6) == Some(&[0x0d, 0x0c, b'S', b'O', b'L', 0x00])
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_sol_header() {
        let mut bytes = vec![0; 16];
        bytes[0..6].copy_from_slice(&[0x0d, 0x0c, b'S', b'O', b'L', 0x00]);
        bytes[6..8].copy_from_slice(&22_050_u16.to_le_bytes());
        bytes[14] = 2;

        let doc = parse_sol(&bytes).expect("sol");

        assert_eq!(doc.format, "sol");
        assert_eq!(doc.streams[0].codec_name, "sol_dpcm");
        assert_eq!(doc.streams[0].channels, Some(2));
    }
}
