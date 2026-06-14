use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_qoa(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_qoa(bytes) {
        return Err(RmpegError::InvalidData("missing QOA magic".to_string()));
    }
    if bytes.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: bytes.len(),
        });
    }

    let total_samples = read_u32_be(bytes, 4)?;
    let channels = u16::from(bytes[8]);
    let sample_rate = read_u24_be(bytes, 9)?;
    if total_samples == 0 || channels == 0 || sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "QOA stream metadata must be nonzero".to_string(),
        ));
    }
    let duration_seconds = total_samples as f64 / sample_rate as f64;
    Ok(ProbeDocument {
        format: "qoa".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "qoa",
            sample_rate,
            channels,
            0,
            duration_seconds,
        )],
    })
}

pub fn looks_like_qoa(bytes: &[u8]) -> bool {
    bytes.starts_with(b"qoaf")
}

fn read_u24_be(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 3;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(
        (u32::from(bytes[pos]) << 16)
            | (u32::from(bytes[pos + 1]) << 8)
            | u32::from(bytes[pos + 2]),
    )
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

    #[test]
    fn parses_qoa_header_and_first_frame() {
        let mut bytes = b"qoaf".to_vec();
        bytes.extend_from_slice(&48_000_u32.to_be_bytes());
        bytes.push(2);
        bytes.extend_from_slice(&[0x00, 0xbb, 0x80]);
        bytes.extend_from_slice(&[0x14, 0x00, 0x10, 0x28]);

        let doc = parse_qoa(&bytes).expect("valid qoa");
        assert_eq!(doc.format, "qoa");
        assert_eq!(doc.streams[0].codec_name, "qoa");
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].sample_rate, Some(48_000));
        assert_eq!(doc.streams[0].duration_seconds, Some(1.0));
    }
}
