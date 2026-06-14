use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_anm(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_anm(bytes) {
        return Err(RmpegError::InvalidData("missing ANM header".to_string()));
    }

    let width = u32::from(read_u16_le(bytes, 0x14)?);
    let height = u32::from(read_u16_le(bytes, 0x16)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid ANM dimensions".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "anm".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "anm",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_anm(bytes: &[u8]) -> bool {
    bytes.len() >= 0x18 && bytes.starts_with(b"LPF ") && bytes.get(0x10..0x14) == Some(b"ANIM")
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
    fn parses_lpf_anim_dimensions() {
        let mut bytes = vec![0; 0x18];
        bytes[0..4].copy_from_slice(b"LPF ");
        bytes[0x10..0x14].copy_from_slice(b"ANIM");
        bytes[0x14..0x16].copy_from_slice(&320_u16.to_le_bytes());
        bytes[0x16..0x18].copy_from_slice(&200_u16.to_le_bytes());

        let doc = parse_anm(&bytes).expect("anm");

        assert_eq!(doc.format, "anm");
        assert_eq!(doc.streams[0].codec_name, "anm");
        assert_eq!(doc.streams[0].width, Some(320));
        assert_eq!(doc.streams[0].duration_seconds, Some(0.0));
    }
}
