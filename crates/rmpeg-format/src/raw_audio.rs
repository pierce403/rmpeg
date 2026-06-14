use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_raw_adp_dtk(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.is_empty() {
        return Err(RmpegError::InvalidData("empty ADP stream".to_string()));
    }

    Ok(ProbeDocument {
        format: "adp".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "adpcm_dtk",
            48_000,
            2,
            0,
            bytes.len() as f64 * 7.0 / 8.0 / 48_000.0,
        )],
    })
}

pub fn parse_raw_g722(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.is_empty() {
        return Err(RmpegError::InvalidData("empty G.722 stream".to_string()));
    }

    Ok(ProbeDocument {
        format: "g722".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "adpcm_g722",
            16_000,
            1,
            4,
            bytes.len() as f64 * 2.0 / 16_000.0,
        )],
    })
}

pub fn parse_raw_g723_1(bytes: &[u8]) -> Result<ProbeDocument> {
    validate_g723_1_frames(bytes)?;
    Ok(ProbeDocument {
        format: "g723_1".to_string(),
        streams: vec![StreamMetadata::audio(0, "g723_1", 8_000, 1, 0, 0.0)],
    })
}

fn validate_g723_1_frames(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        return Err(RmpegError::InvalidData("empty G.723.1 stream".to_string()));
    }

    let mut offset = 0;
    let mut frames = 0;
    while offset < bytes.len() {
        let frame_size = g723_1_frame_size(bytes[offset]);
        if offset + frame_size > bytes.len() {
            return Err(RmpegError::InvalidData(
                "truncated G.723.1 frame".to_string(),
            ));
        }
        offset += frame_size;
        frames += 1;
    }

    if frames < 2 {
        return Err(RmpegError::InvalidData(
            "too few G.723.1 frames".to_string(),
        ));
    }
    Ok(())
}

fn g723_1_frame_size(header: u8) -> usize {
    match header & 0b11 {
        0 => 24,
        1 => 20,
        2 => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extension_gated_adp_duration_from_byte_count() {
        let doc = parse_raw_adp_dtk(&vec![0; 32_768]).expect("adp");

        assert_eq!(doc.format, "adp");
        assert_eq!(doc.streams[0].codec_name, "adpcm_dtk");
        assert_eq!(doc.streams[0].sample_rate, Some(48_000));
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].duration_seconds, Some(28_672.0 / 48_000.0));
    }

    #[test]
    fn parses_raw_g722_duration_from_byte_count() {
        let doc = parse_raw_g722(&vec![0; 170_326]).expect("g722");

        assert_eq!(doc.format, "g722");
        assert_eq!(doc.streams[0].codec_name, "adpcm_g722");
        assert_eq!(doc.streams[0].sample_rate, Some(16_000));
        assert_eq!(doc.streams[0].bits_per_sample, Some(4));
        assert_eq!(doc.streams[0].duration_seconds, Some(21.29075));
    }

    #[test]
    fn validates_observed_g723_1_frame_sizes() {
        let mut bytes = Vec::new();
        bytes.extend([0x00; 24]);
        bytes.extend([0x01; 20]);
        bytes.extend([0x02; 4]);
        bytes.extend([0x03; 1]);

        let doc = parse_raw_g723_1(&bytes).expect("g723.1");

        assert_eq!(doc.format, "g723_1");
        assert_eq!(doc.streams[0].codec_name, "g723_1");
        assert_eq!(doc.streams[0].sample_rate, Some(8_000));
    }

    #[test]
    fn rejects_truncated_g723_1_frame() {
        let error = parse_raw_g723_1(&[0x00; 23]).expect_err("truncated");

        assert!(error.to_string().contains("truncated G.723.1 frame"));
    }
}
