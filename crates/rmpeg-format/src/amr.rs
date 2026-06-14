use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const AMR_NB_MAGIC: &[u8] = b"#!AMR\n";

pub fn parse_amr_nb(bytes: &[u8]) -> Result<ProbeDocument> {
    if !bytes.starts_with(AMR_NB_MAGIC) {
        return Err(RmpegError::InvalidData(
            "missing AMR-NB magic header".to_string(),
        ));
    }

    let mut pos = AMR_NB_MAGIC.len();
    let mut duration_seconds = 0.0;
    let mut frames = 0_u32;
    while pos < bytes.len() {
        let header = bytes[pos];
        let frame_type = (header >> 3) & 0x0f;
        let size = frame_size(frame_type).ok_or_else(|| {
            RmpegError::InvalidData(format!("unsupported AMR-NB frame type {frame_type}"))
        })?;
        let end = pos
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("AMR-NB frame offset overflow".to_string()))?;
        if end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: end,
                remaining: bytes.len(),
            });
        }
        duration_seconds += frame_duration_seconds(frame_type, size)?;
        frames += 1;
        pos = end;
    }

    if frames == 0 {
        return Err(RmpegError::InvalidData(
            "AMR-NB stream has no frames".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "amr".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "amr_nb",
            8_000,
            1,
            0,
            duration_seconds,
        )],
    })
}

fn frame_size(frame_type: u8) -> Option<usize> {
    let payload_size = match frame_type {
        0 => 12,
        1 => 13,
        2 => 15,
        3 => 17,
        4 => 19,
        5 => 20,
        6 => 26,
        7 => 31,
        8 => 5,
        _ => return None,
    };
    Some(payload_size + 1)
}

fn frame_duration_seconds(frame_type: u8, frame_size: usize) -> Result<f64> {
    let bit_rate = match frame_type {
        0 => 5_200.0,
        1 => 5_600.0,
        2 => 6_400.0,
        3 => 7_200.0,
        4 => 8_000.0,
        5 => 8_000.0,
        6 => 10_400.0,
        7 => 12_400.0,
        8 => 2_400.0,
        _ => {
            return Err(RmpegError::InvalidData(format!(
                "unsupported AMR-NB frame type {frame_type}"
            )));
        }
    };
    Ok(frame_size as f64 * 8.0 / bit_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_amr_nb_metadata() {
        let mut bytes = AMR_NB_MAGIC.to_vec();
        for _ in 0..284 {
            bytes.push(6 << 3);
            bytes.extend_from_slice(&[0; 26]);
        }

        let doc = parse_amr_nb(&bytes).expect("valid AMR-NB");
        assert_eq!(doc.format, "amr");
        assert_eq!(doc.streams[0].codec_name, "amr_nb");
        assert_eq!(doc.streams[0].sample_rate, Some(8_000));
        assert_eq!(doc.streams[0].channels, Some(1));
        assert_eq!(doc.streams[0].bits_per_sample, Some(0));
        let duration = doc.streams[0].duration_seconds.expect("duration");
        assert!((duration - 5.898461538461539).abs() < 0.000_000_001);
    }
}
