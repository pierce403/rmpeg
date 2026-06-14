use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_siff(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_siff(bytes) {
        return Err(RmpegError::InvalidData(
            "missing SIFF VBV1 header".to_string(),
        ));
    }

    let width = u32::from(read_u16_le(bytes, 0x16)?);
    let height = u32::from(read_u16_le(bytes, 0x18)?);
    let frames = u32::from(read_u16_le(bytes, 0x1e)?);
    let bits_per_sample = read_u16_le(bytes, 0x20)?;
    let sample_rate = u32::from(read_u16_le(bytes, 0x22)?);
    if width == 0 || height == 0 || frames == 0 || sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "invalid SIFF stream metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "siff".to_string(),
        streams: vec![
            StreamMetadata::video(0, "vb", width, height, Some(frames as f64 / 12.0), None),
            StreamMetadata::audio(1, "pcm_u8", sample_rate, 1, bits_per_sample, 0.0),
        ],
    })
}

pub fn looks_like_siff(bytes: &[u8]) -> bool {
    bytes.len() >= 0x38
        && bytes.starts_with(b"SIFF")
        && bytes.get(8..12) == Some(b"VBV1")
        && bytes.get(12..16) == Some(b"VBHD")
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
    fn parses_observed_siff_vbv1_header() {
        let mut bytes = vec![0; 0x38];
        bytes[0..4].copy_from_slice(b"SIFF");
        bytes[8..12].copy_from_slice(b"VBV1");
        bytes[12..16].copy_from_slice(b"VBHD");
        bytes[0x16..0x18].copy_from_slice(&320_u16.to_le_bytes());
        bytes[0x18..0x1a].copy_from_slice(&240_u16.to_le_bytes());
        bytes[0x1e..0x20].copy_from_slice(&100_u16.to_le_bytes());
        bytes[0x20..0x22].copy_from_slice(&8_u16.to_le_bytes());
        bytes[0x22..0x24].copy_from_slice(&22_050_u16.to_le_bytes());

        let doc = parse_siff(&bytes).expect("siff");

        assert_eq!(doc.format, "siff");
        assert_eq!(doc.streams[0].codec_name, "vb");
        assert_eq!(doc.streams[0].duration_seconds, Some(100.0 / 12.0));
        assert_eq!(doc.streams[1].codec_name, "pcm_u8");
        assert_eq!(doc.streams[1].sample_rate, Some(22_050));
    }
}
