use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_bmp(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 26 {
        return Err(RmpegError::UnexpectedEof {
            needed: 26,
            remaining: bytes.len(),
        });
    }
    if &bytes[0..2] != b"BM" {
        return Err(RmpegError::InvalidData("missing BMP signature".to_string()));
    }

    let dib_size = read_u32_le(bytes, 14)?;
    let (width, height) = match dib_size {
        12 => {
            let width = u32::from(read_u16_le(bytes, 18)?);
            let height = u32::from(read_u16_le(bytes, 20)?);
            (width, height)
        }
        40 | 52 | 56 | 64 | 108 | 124 => {
            let width = read_i32_le(bytes, 18)?;
            let height = read_i32_le(bytes, 22)?;
            if width <= 0 {
                return Err(RmpegError::InvalidData(format!(
                    "invalid BMP width {width}"
                )));
            }
            let height = height
                .checked_abs()
                .ok_or_else(|| RmpegError::InvalidData("invalid BMP height".to_string()))?;
            (width as u32, height as u32)
        }
        _ => {
            return Err(RmpegError::InvalidData(format!(
                "unsupported BMP DIB header size {dib_size}"
            )));
        }
    };

    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "BMP dimensions must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "bmp_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "bmp",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_bmp(bytes: &[u8]) -> bool {
    bytes.starts_with(b"BM")
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

fn read_u32_le(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

fn read_i32_le(bytes: &[u8], pos: usize) -> Result<i32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(i32::from_le_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bmp_with_dib_header(dib: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 14];
        bytes[0..2].copy_from_slice(b"BM");
        bytes.extend_from_slice(dib);
        bytes
    }

    #[test]
    fn parses_bitmap_info_header_dimensions() {
        let mut dib = vec![0; 40];
        dib[0..4].copy_from_slice(&40_u32.to_le_bytes());
        dib[4..8].copy_from_slice(&127_i32.to_le_bytes());
        dib[8..12].copy_from_slice(&64_i32.to_le_bytes());
        let doc = parse_bmp(&bmp_with_dib_header(&dib)).expect("valid bmp");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "bmp_pipe");
        assert_eq!(stream.codec_name, "bmp");
        assert_eq!(stream.width, Some(127));
        assert_eq!(stream.height, Some(64));
    }

    #[test]
    fn parses_os2_core_header_dimensions() {
        let mut dib = vec![0; 12];
        dib[0..4].copy_from_slice(&12_u32.to_le_bytes());
        dib[4..6].copy_from_slice(&300_u16.to_le_bytes());
        dib[6..8].copy_from_slice(&22_u16.to_le_bytes());
        let doc = parse_bmp(&bmp_with_dib_header(&dib)).expect("valid bmp");
        let stream = &doc.streams[0];
        assert_eq!(stream.width, Some(300));
        assert_eq!(stream.height, Some(22));
    }

    #[test]
    fn top_down_height_is_reported_positive() {
        let mut dib = vec![0; 40];
        dib[0..4].copy_from_slice(&40_u32.to_le_bytes());
        dib[4..8].copy_from_slice(&10_i32.to_le_bytes());
        dib[8..12].copy_from_slice(&(-20_i32).to_le_bytes());
        let doc = parse_bmp(&bmp_with_dib_header(&dib)).expect("valid bmp");
        assert_eq!(doc.streams[0].height, Some(20));
    }

    #[test]
    fn rejects_unsupported_header_size() {
        let mut dib = vec![0; 16];
        dib[0..4].copy_from_slice(&16_u32.to_le_bytes());
        let err = parse_bmp(&bmp_with_dib_header(&dib)).expect_err("unsupported header");
        assert!(err.to_string().contains("DIB header size"));
    }
}
