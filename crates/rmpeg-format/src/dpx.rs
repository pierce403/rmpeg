use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_dpx(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_dpx(bytes) {
        return Err(RmpegError::InvalidData("missing DPX magic".to_string()));
    }
    if bytes.len() < 780 {
        return Err(RmpegError::UnexpectedEof {
            needed: 780,
            remaining: bytes.len(),
        });
    }
    let big_endian = bytes.starts_with(b"SDPX");
    let width = read_u32(bytes, 772, big_endian)?;
    let height = read_u32(bytes, 776, big_endian)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "DPX dimensions must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "dpx_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "dpx",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_dpx(bytes: &[u8]) -> bool {
    bytes.starts_with(b"SDPX") || bytes.starts_with(b"XPDS")
}

fn read_u32(bytes: &[u8], pos: usize, big_endian: bool) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    let raw = [bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]];
    Ok(if big_endian {
        u32::from_be_bytes(raw)
    } else {
        u32::from_le_bytes(raw)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_big_endian_dpx_dimensions() {
        let mut bytes = vec![0; 780];
        bytes[0..4].copy_from_slice(b"SDPX");
        bytes[772..776].copy_from_slice(&768_u32.to_be_bytes());
        bytes[776..780].copy_from_slice(&512_u32.to_be_bytes());
        let doc = parse_dpx(&bytes).expect("valid dpx");
        assert_eq!(doc.streams[0].width, Some(768));
        assert_eq!(doc.streams[0].height, Some(512));
    }
}
