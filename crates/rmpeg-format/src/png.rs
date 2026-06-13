use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

pub fn parse_png(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 33 {
        return Err(RmpegError::UnexpectedEof {
            needed: 33,
            remaining: bytes.len(),
        });
    }
    if &bytes[0..8] != PNG_SIGNATURE {
        return Err(RmpegError::InvalidData("missing PNG signature".to_string()));
    }

    let ihdr_len = read_u32_be(bytes, 8)?;
    if ihdr_len != 13 || &bytes[12..16] != b"IHDR" {
        return Err(RmpegError::InvalidData(
            "PNG must start with a 13-byte IHDR chunk".to_string(),
        ));
    }

    let width = read_u32_be(bytes, 16)?;
    let height = read_u32_be(bytes, 20)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "PNG dimensions must be nonzero".to_string(),
        ));
    }

    let (format, codec_name) = if has_animation_control_chunk(bytes) {
        ("apng", "apng")
    } else {
        ("png_pipe", "png")
    };

    Ok(ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::video(
            0,
            codec_name,
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_png(bytes: &[u8]) -> bool {
    bytes.starts_with(PNG_SIGNATURE)
}

fn read_u32_be(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

fn has_animation_control_chunk(bytes: &[u8]) -> bool {
    let mut pos = 8;
    while pos + 8 <= bytes.len() {
        let Ok(chunk_len) = read_u32_be(bytes, pos) else {
            return false;
        };
        let chunk_type = &bytes[pos + 4..pos + 8];
        if chunk_type == b"acTL" {
            return true;
        }
        if chunk_type == b"IDAT" || chunk_type == b"IEND" {
            return false;
        }

        let Some(data_end) = (pos + 8).checked_add(chunk_len as usize) else {
            return false;
        };
        let Some(next_pos) = data_end.checked_add(4) else {
            return false;
        };
        if next_pos > bytes.len() {
            return false;
        }
        pos = next_pos;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(PNG_SIGNATURE);
        bytes.extend_from_slice(&13_u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 2, 0, 0, 0]);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes
    }

    #[test]
    fn parses_png_dimensions() {
        let doc = parse_png(&minimal_png(128, 64)).expect("valid png");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "png_pipe");
        assert_eq!(stream.codec_name, "png");
        assert_eq!(stream.width, Some(128));
        assert_eq!(stream.height, Some(64));
        assert_eq!(stream.duration_seconds, Some(0.0));
    }

    #[test]
    fn rejects_missing_ihdr() {
        let mut bytes = minimal_png(1, 1);
        bytes[12..16].copy_from_slice(b"IDAT");
        let err = parse_png(&bytes).expect_err("missing ihdr");
        assert!(err.to_string().contains("IHDR"));
    }

    #[test]
    fn reports_apng_when_animation_control_chunk_precedes_idat() {
        let mut bytes = minimal_png(128, 64);
        bytes.extend_from_slice(&8_u32.to_be_bytes());
        bytes.extend_from_slice(b"acTL");
        bytes.extend_from_slice(&[0, 0, 0, 2, 0, 0, 0, 0]);
        bytes.extend_from_slice(&0_u32.to_be_bytes());

        let doc = parse_png(&bytes).expect("valid apng");
        assert_eq!(doc.format, "apng");
        assert_eq!(doc.streams[0].codec_name, "apng");
    }
}
