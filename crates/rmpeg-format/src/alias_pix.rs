use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_alias_pix(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 8 {
        return Err(RmpegError::UnexpectedEof {
            needed: 8,
            remaining: bytes.len(),
        });
    }
    if bytes[4..8] != [0, 0, 0, 0] {
        return Err(RmpegError::InvalidData(
            "missing Alias PIX reserved header bytes".to_string(),
        ));
    }
    let width = u32::from(read_u16_be(bytes, 0)?);
    let height = u32::from(read_u16_be(bytes, 2)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "Alias PIX dimensions must be nonzero".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "alias_pix".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "alias_pix",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

fn read_u16_be(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[pos], bytes[pos + 1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alias_pix_dimensions() {
        let mut bytes = vec![0; 8];
        bytes[0..2].copy_from_slice(&201_u16.to_be_bytes());
        bytes[2..4].copy_from_slice(&79_u16.to_be_bytes());

        let doc = parse_alias_pix(&bytes).expect("valid alias pix");
        assert_eq!(doc.format, "alias_pix");
        assert_eq!(doc.streams[0].width, Some(201));
        assert_eq!(doc.streams[0].height, Some(79));
    }
}
