use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_vmd(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: bytes.len(),
        });
    }
    let width = u32::from(read_u16_le(bytes, 12)?);
    let height = u32::from(read_u16_le(bytes, 14)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "VMD dimensions must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "vmd".to_string(),
        streams: vec![
            StreamMetadata::video(0, "vmdvideo", width, height, Some(0.0), None),
            StreamMetadata::audio(1, "vmdaudio", 22_050, 1, 0, 0.0),
        ],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vmd_stream_metadata() {
        let mut bytes = vec![0; 16];
        bytes[12..14].copy_from_slice(&320_u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&240_u16.to_le_bytes());
        let doc = parse_vmd(&bytes).expect("valid vmd");
        assert_eq!(doc.format, "vmd");
        assert_eq!(doc.streams[0].codec_name, "vmdvideo");
        assert_eq!(doc.streams[1].codec_name, "vmdaudio");
    }
}
