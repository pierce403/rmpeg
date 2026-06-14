use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_tta(bytes: &[u8]) -> Result<ProbeDocument> {
    if !bytes.starts_with(b"TTA1") {
        return Err(RmpegError::InvalidData("missing TTA1 marker".to_string()));
    }
    if bytes.len() < 18 {
        return Err(RmpegError::UnexpectedEof {
            needed: 18,
            remaining: bytes.len(),
        });
    }

    let format = read_u16_le(bytes, 4)?;
    if format != 1 {
        return Err(RmpegError::Unsupported(format!(
            "unsupported TTA format {format}"
        )));
    }
    let channels = read_u16_le(bytes, 6)?;
    let bits_per_sample = read_u16_le(bytes, 8)?;
    let sample_rate = read_u32_le(bytes, 10)?;
    let total_samples = read_u32_le(bytes, 14)?;
    if channels == 0 || sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "TTA stream has invalid audio metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "tta".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "tta",
            sample_rate,
            channels,
            bits_per_sample,
            total_samples as f64 / sample_rate as f64,
        )],
    })
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
    fn parses_tta_header_metadata() {
        let mut bytes = b"TTA1".to_vec();
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(&44_100_u32.to_le_bytes());
        bytes.extend_from_slice(&44_100_u32.to_le_bytes());

        let doc = parse_tta(&bytes).unwrap();

        assert_eq!(doc.streams[0].codec_name, "tta");
        assert_eq!(doc.streams[0].sample_rate, Some(44_100));
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].duration_seconds, Some(1.0));
    }
}
