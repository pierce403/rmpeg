use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_amv(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_amv(bytes) {
        return Err(RmpegError::InvalidData("missing AMV header".to_string()));
    }
    let width = read_u32_le(bytes, 0x40)?;
    let height = read_u32_le(bytes, 0x44)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid AMV dimensions".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "avi".to_string(),
        streams: vec![
            StreamMetadata::video(0, "amv", width, height, Some(0.0), None),
            StreamMetadata::audio(1, "adpcm_ima_amv", 22_050, 1, 4, 0.0),
        ],
    })
}

pub fn looks_like_amv(bytes: &[u8]) -> bool {
    bytes.len() >= 0x58 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"AMV ")
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
    fn parses_observed_amv_header() {
        let mut bytes = vec![0; 0x58];
        bytes[0..4].copy_from_slice(b"RIFF");
        bytes[8..12].copy_from_slice(b"AMV ");
        bytes[0x40..0x44].copy_from_slice(&160_u32.to_le_bytes());
        bytes[0x44..0x48].copy_from_slice(&120_u32.to_le_bytes());

        let doc = parse_amv(&bytes).expect("amv");

        assert_eq!(doc.format, "avi");
        assert_eq!(doc.streams[0].codec_name, "amv");
        assert_eq!(doc.streams[1].codec_name, "adpcm_ima_amv");
    }
}
