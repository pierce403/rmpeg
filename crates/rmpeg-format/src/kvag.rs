use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_kvag(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_kvag(bytes) {
        return Err(RmpegError::InvalidData("missing KVAG header".to_string()));
    }

    let data_size = read_u32_le(bytes, 4)?;
    let sample_rate = read_u32_le(bytes, 8)?;
    let channels = u16::from(bytes[12]) + 1;
    if data_size == 0 || sample_rate == 0 || channels == 0 || channels > 2 {
        return Err(RmpegError::InvalidData(
            "invalid KVAG audio metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "kvag".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "adpcm_ima_ssi",
            sample_rate,
            channels,
            4,
            data_size as f64 * 2.0 / f64::from(channels) / sample_rate as f64,
        )],
    })
}

pub fn looks_like_kvag(bytes: &[u8]) -> bool {
    bytes.len() >= 16
        && bytes.starts_with(b"KVAG")
        && matches!(bytes[12], 0 | 1)
        && read_u32_le(bytes, 8).is_ok_and(|rate| (8_000..=96_000).contains(&rate))
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
    fn parses_observed_stereo_kvag_header() {
        let mut bytes = vec![0; 16];
        bytes[0..4].copy_from_slice(b"KVAG");
        bytes[4..8].copy_from_slice(&125_000_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&22_050_u32.to_le_bytes());
        bytes[12] = 1;

        let doc = parse_kvag(&bytes).expect("kvag");

        assert_eq!(doc.format, "kvag");
        assert_eq!(doc.streams[0].codec_name, "adpcm_ima_ssi");
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].bits_per_sample, Some(4));
        assert_eq!(doc.streams[0].duration_seconds, Some(125_000.0 / 22_050.0));
    }

    #[test]
    fn rejects_unreasonable_channel_flag() {
        let mut bytes = vec![0; 16];
        bytes[0..4].copy_from_slice(b"KVAG");
        bytes[8..12].copy_from_slice(&22_050_u32.to_le_bytes());
        bytes[12] = 4;

        assert!(parse_kvag(&bytes).is_err());
    }
}
