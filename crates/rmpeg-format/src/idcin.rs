use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_idcin(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_idcin(bytes) {
        return Err(RmpegError::InvalidData("missing Id CIN header".to_string()));
    }
    let width = read_u32_le(bytes, 0)?;
    let height = read_u32_le(bytes, 4)?;
    let sample_rate = read_u32_le(bytes, 8)?;
    let channels = read_u32_le(bytes, 12)?;
    if width == 0 || height == 0 || sample_rate == 0 || channels == 0 || channels > 8 {
        return Err(RmpegError::InvalidData(
            "invalid Id CIN metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "idcin".to_string(),
        streams: vec![
            StreamMetadata::video(0, "idcin", width, height, Some(0.0), None),
            StreamMetadata::audio(1, "pcm_s16le", sample_rate, channels as u16, 16, 0.0),
        ],
    })
}

pub fn looks_like_idcin(bytes: &[u8]) -> bool {
    if bytes.len() < 20 {
        return false;
    }
    let Ok(width) = read_u32_le(bytes, 0) else {
        return false;
    };
    let Ok(height) = read_u32_le(bytes, 4) else {
        return false;
    };
    let Ok(sample_rate) = read_u32_le(bytes, 8) else {
        return false;
    };
    let Ok(channels) = read_u32_le(bytes, 12) else {
        return false;
    };
    matches!(width, 320 | 640)
        && (height == 200 || height == 240 || height == 480)
        && (8_000..=48_000).contains(&sample_rate)
        && (1..=2).contains(&channels)
        && bytes.get(20..24) == Some(&[0x00, 0x44, 0x00, 0x00])
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
    fn parses_observed_idcin_header() {
        let mut bytes = vec![0; 24];
        bytes[0..4].copy_from_slice(&320_u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&240_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&22_050_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&2_u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&[0x00, 0x44, 0x00, 0x00]);

        let doc = parse_idcin(&bytes).expect("idcin");

        assert_eq!(doc.format, "idcin");
        assert_eq!(doc.streams[0].codec_name, "idcin");
        assert_eq!(doc.streams[1].codec_name, "pcm_s16le");
    }
}
