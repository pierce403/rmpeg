use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_apm(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_apm(bytes) {
        return Err(RmpegError::InvalidData("missing APM header".to_string()));
    }
    let channels = u16::from(bytes[2]);
    let sample_rate = read_u32_le(bytes, 4)?;
    let compressed_sample_count = read_u32_le(bytes, 28)?;
    if channels == 0 || sample_rate == 0 || compressed_sample_count == 0 {
        return Err(RmpegError::InvalidData(
            "APM audio metadata must be nonzero".to_string(),
        ));
    }
    let samples = compressed_sample_count as f64 * 2.0 / f64::from(channels);
    Ok(ProbeDocument {
        format: "apm".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "adpcm_ima_apm",
            sample_rate,
            channels,
            4,
            samples / sample_rate as f64,
        )],
    })
}

pub fn looks_like_apm(bytes: &[u8]) -> bool {
    bytes.len() >= 32 && bytes.get(20..24) == Some(b"vs12") && bytes.get(14) == Some(&4)
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
    fn parses_apm_duration_from_sample_count() {
        let mut bytes = vec![0; 32];
        bytes[2] = 1;
        bytes[4..8].copy_from_slice(&11_025_u32.to_le_bytes());
        bytes[14] = 4;
        bytes[20..24].copy_from_slice(b"vs12");
        bytes[28..32].copy_from_slice(&33_102_u32.to_le_bytes());

        let doc = parse_apm(&bytes).expect("valid apm");
        assert_eq!(doc.streams[0].codec_name, "adpcm_ima_apm");
        let duration = doc.streams[0].duration_seconds.unwrap();
        assert!((duration - 6.004897959183674).abs() < 0.000001);
    }
}
