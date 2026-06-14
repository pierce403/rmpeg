use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_westwood_aud(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 12 {
        return Err(RmpegError::UnexpectedEof {
            needed: 12,
            remaining: bytes.len(),
        });
    }
    let sample_rate = u32::from(read_u16_le(bytes, 0)?);
    let data_size = usize::try_from(read_u32_le(bytes, 2)?)
        .map_err(|_| RmpegError::InvalidData("Westwood AUD data size is too large".to_string()))?;
    if sample_rate == 0 || 12 + data_size > bytes.len() {
        return Err(RmpegError::InvalidData(
            "invalid Westwood AUD metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "wsaud".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "adpcm_ima_ws",
            sample_rate,
            1,
            4,
            data_size as f64 * 2.0 / sample_rate as f64,
        )],
    })
}

fn read_u16_le(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_u32_le(bytes: &[u8], pos: usize) -> Result<u32> {
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
    fn parses_extension_gated_westwood_aud() {
        let mut bytes = vec![0; 12];
        bytes[0..2].copy_from_slice(&22_050_u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&5_304_u16.to_le_bytes());
        bytes.extend_from_slice(&vec![0; 5_304]);

        let doc = parse_westwood_aud(&bytes).expect("aud");

        assert_eq!(doc.format, "wsaud");
        assert_eq!(doc.streams[0].codec_name, "adpcm_ima_ws");
        assert_eq!(doc.streams[0].duration_seconds, Some(10_608.0 / 22_050.0));
    }
}
