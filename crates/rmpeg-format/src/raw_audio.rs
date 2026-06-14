use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const ADP_DEC_MIN_BYTES: usize = 1_000_000;
const ADP_DEC_MAX_INITIAL_SILENCE_BYTES: usize = 4096;
const ADP_DEC_PAIRED_SAMPLE_WINDOW: usize = 16_384;
const ADP_PCM_MIN_BYTES: usize = 80_000;
const ADP_PCM_PAIRED_SAMPLE_WINDOW: usize = 8192;

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

pub fn parse_raw_adp_dtk_dec(bytes: &[u8]) -> Result<ProbeDocument> {
    validate_duplicated_stereo_pairs(
        bytes,
        ADP_DEC_MIN_BYTES,
        Some(ADP_DEC_MAX_INITIAL_SILENCE_BYTES),
        ADP_DEC_PAIRED_SAMPLE_WINDOW,
        "ADP .dec",
    )?;
    parse_raw_adp_dtk(bytes)
}

pub fn parse_raw_adp_dtk_pcm(bytes: &[u8]) -> Result<ProbeDocument> {
    validate_duplicated_stereo_pairs(
        bytes,
        ADP_PCM_MIN_BYTES,
        None,
        ADP_PCM_PAIRED_SAMPLE_WINDOW,
        "ADP .pcm",
    )?;
    parse_raw_adp_dtk(bytes)
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

pub fn parse_raw_g728(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.is_empty() {
        return Err(RmpegError::InvalidData("empty G.728 stream".to_string()));
    }

    Ok(ProbeDocument {
        format: "g728".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "g728",
            8_000,
            1,
            2,
            bytes.len() as f64 * 8.0 / 16_000.0,
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

fn validate_duplicated_stereo_pairs(
    bytes: &[u8],
    min_bytes: usize,
    max_initial_silence: Option<usize>,
    paired_sample_window: usize,
    label: &str,
) -> Result<()> {
    if bytes.len() < min_bytes {
        return Err(RmpegError::InvalidData(format!(
            "{label} probe window is too small"
        )));
    }

    let mut checked_pairs = 0usize;
    let mut saw_nonzero = false;
    let mut pos = 0usize;
    while pos + 3 < bytes.len() {
        let left = i16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
        let right = i16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        if left != 0 || right != 0 {
            if !saw_nonzero {
                if max_initial_silence.is_some_and(|max| pos > max) {
                    return Err(RmpegError::InvalidData(format!(
                        "{label} initial silence is too long"
                    )));
                }
                saw_nonzero = true;
            }
            if left != right {
                return Err(RmpegError::InvalidData(format!(
                    "{label} paired samples are not duplicated"
                )));
            }
            checked_pairs += 1;
            if checked_pairs >= paired_sample_window {
                return Ok(());
            }
        }
        pos += 4;
    }

    Err(RmpegError::InvalidData(format!(
        "{label} paired sample window was not found"
    )))
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
    fn parses_extension_gated_dec_adp_when_pairs_match_ffprobe_probe_shape() {
        let mut bytes = vec![0; ADP_DEC_MIN_BYTES];
        let start = 128;
        for index in 0..ADP_DEC_PAIRED_SAMPLE_WINDOW {
            let pos = start + index * 4;
            bytes[pos..pos + 2].copy_from_slice(&5_i16.to_le_bytes());
            bytes[pos + 2..pos + 4].copy_from_slice(&5_i16.to_le_bytes());
        }

        let doc = parse_raw_adp_dtk_dec(&bytes).expect("dec adp");

        assert_eq!(doc.format, "adp");
        assert_eq!(doc.streams[0].codec_name, "adpcm_dtk");
        assert_eq!(
            doc.streams[0].duration_seconds,
            Some(ADP_DEC_MIN_BYTES as f64 * 7.0 / 8.0 / 48_000.0)
        );
    }

    #[test]
    fn rejects_extension_gated_dec_adp_when_pairs_differ() {
        let mut bytes = vec![0; ADP_DEC_MIN_BYTES];
        bytes[128..130].copy_from_slice(&5_i16.to_le_bytes());
        bytes[130..132].copy_from_slice(&6_i16.to_le_bytes());

        let error = parse_raw_adp_dtk_dec(&bytes).expect_err("mismatched stereo pair");

        assert!(error.to_string().contains("paired samples"));
    }

    #[test]
    fn rejects_extension_gated_dec_adp_when_signal_starts_too_late() {
        let mut bytes = vec![0; ADP_DEC_MIN_BYTES];
        let start = ADP_DEC_MAX_INITIAL_SILENCE_BYTES + 4;
        bytes[start..start + 2].copy_from_slice(&5_i16.to_le_bytes());
        bytes[start + 2..start + 4].copy_from_slice(&5_i16.to_le_bytes());

        let error = parse_raw_adp_dtk_dec(&bytes).expect_err("late nonzero signal");

        assert!(error.to_string().contains("initial silence"));
    }

    #[test]
    fn parses_extension_gated_pcm_adp_with_late_duplicated_pairs() {
        let mut bytes = vec![0; 473_088];
        let start = 127_260;
        for index in 0..ADP_PCM_PAIRED_SAMPLE_WINDOW {
            let pos = start + index * 4;
            bytes[pos..pos + 2].copy_from_slice(&7_i16.to_le_bytes());
            bytes[pos + 2..pos + 4].copy_from_slice(&7_i16.to_le_bytes());
        }

        let doc = parse_raw_adp_dtk_pcm(&bytes).expect("pcm adp");

        assert_eq!(doc.format, "adp");
        assert_eq!(doc.streams[0].codec_name, "adpcm_dtk");
        assert_eq!(
            doc.streams[0].duration_seconds,
            Some(473_088.0 * 7.0 / 8.0 / 48_000.0)
        );
    }

    #[test]
    fn rejects_extension_gated_pcm_adp_without_enough_duplicated_pairs() {
        let mut bytes = vec![0; ADP_PCM_MIN_BYTES];
        for index in 0..1024 {
            let pos = index * 4;
            bytes[pos..pos + 2].copy_from_slice(&7_i16.to_le_bytes());
            bytes[pos + 2..pos + 4].copy_from_slice(&7_i16.to_le_bytes());
        }

        let error = parse_raw_adp_dtk_pcm(&bytes).expect_err("short paired window");

        assert!(error.to_string().contains("paired sample window"));
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
    fn parses_raw_g728_duration_from_byte_count() {
        let doc = parse_raw_g728(&vec![0; 1600]).expect("g728");

        assert_eq!(doc.format, "g728");
        assert_eq!(doc.streams[0].codec_name, "g728");
        assert_eq!(doc.streams[0].sample_rate, Some(8_000));
        assert_eq!(doc.streams[0].channels, Some(1));
        assert_eq!(doc.streams[0].bits_per_sample, Some(2));
        assert_eq!(doc.streams[0].duration_seconds, Some(0.8));
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
