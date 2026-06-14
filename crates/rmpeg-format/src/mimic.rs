use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_mimic_cam(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 16 || bytes.get(12..16) != Some(b"ML20") {
        return Err(RmpegError::InvalidData(
            "missing Mimic CAM header".to_string(),
        ));
    }
    let width = u32::from(read_u16_le(bytes, 2)?);
    let height = u32::from(read_u16_le(bytes, 4)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid Mimic dimensions".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "msnwctcp".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "mimic",
            width,
            height,
            Some(0.0),
            None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extension_gated_mimic_cam_dimensions() {
        let mut bytes = vec![0; 16];
        bytes[2..4].copy_from_slice(&320_u16.to_le_bytes());
        bytes[4..6].copy_from_slice(&240_u16.to_le_bytes());
        bytes[12..16].copy_from_slice(b"ML20");

        let doc = parse_mimic_cam(&bytes).expect("mimic");

        assert_eq!(doc.format, "msnwctcp");
        assert_eq!(doc.streams[0].codec_name, "mimic");
        assert_eq!(doc.streams[0].width, Some(320));
    }
}
