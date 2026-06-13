use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_exr(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 8 {
        return Err(RmpegError::UnexpectedEof {
            needed: 8,
            remaining: bytes.len(),
        });
    }
    if !looks_like_exr(bytes) {
        return Err(RmpegError::InvalidData(
            "missing OpenEXR signature".to_string(),
        ));
    }
    if bytes[4] != 2 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported OpenEXR version {}",
            bytes[4]
        )));
    }

    let dimensions = parse_dimensions(bytes)?;
    Ok(ProbeDocument {
        format: "exr_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "exr",
            dimensions.width,
            dimensions.height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_exr(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && &bytes[0..4] == b"\x76\x2f\x31\x01"
}

fn parse_dimensions(bytes: &[u8]) -> Result<Dimensions> {
    let mut pos = 8;
    let mut data_window = None;
    let mut display_window = None;
    while pos < bytes.len() {
        let name_end = find_nul(bytes, pos)?;
        if name_end == pos {
            break;
        }
        let name = &bytes[pos..name_end];
        pos = name_end + 1;

        let type_end = find_nul(bytes, pos)?;
        let attr_type = &bytes[pos..type_end];
        pos = type_end + 1;

        let value_size = read_u32_le(bytes, pos)? as usize;
        pos += 4;
        let value_end = pos
            .checked_add(value_size)
            .ok_or_else(|| RmpegError::InvalidData("OpenEXR attribute too large".to_string()))?;
        if value_end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: value_end,
                remaining: bytes.len(),
            });
        }

        if name == b"dataWindow" || name == b"displayWindow" {
            if attr_type != b"box2i" {
                return Err(RmpegError::InvalidData(
                    "OpenEXR window is not box2i".to_string(),
                ));
            }
            if value_size != 16 {
                return Err(RmpegError::InvalidData(
                    "OpenEXR window has invalid size".to_string(),
                ));
            }
            let dimensions = dimensions_from_box2i(bytes, pos)?;
            if name == b"displayWindow" {
                display_window = Some(dimensions);
            } else {
                data_window = Some(dimensions);
            }
        }

        pos = value_end;
    }

    display_window.or(data_window).ok_or_else(|| {
        RmpegError::InvalidData("missing OpenEXR displayWindow or dataWindow".to_string())
    })
}

fn dimensions_from_box2i(bytes: &[u8], pos: usize) -> Result<Dimensions> {
    let min_x = read_i32_le(bytes, pos)?;
    let min_y = read_i32_le(bytes, pos + 4)?;
    let max_x = read_i32_le(bytes, pos + 8)?;
    let max_y = read_i32_le(bytes, pos + 12)?;
    if max_x < min_x || max_y < min_y {
        return Err(RmpegError::InvalidData(
            "OpenEXR dataWindow is inverted".to_string(),
        ));
    }
    let width = i64::from(max_x) - i64::from(min_x) + 1;
    let height = i64::from(max_y) - i64::from(min_y) + 1;
    let width = u32::try_from(width)
        .map_err(|_| RmpegError::InvalidData("OpenEXR width overflow".to_string()))?;
    let height = u32::try_from(height)
        .map_err(|_| RmpegError::InvalidData("OpenEXR height overflow".to_string()))?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "OpenEXR dimensions must be nonzero".to_string(),
        ));
    }
    Ok(Dimensions { width, height })
}

fn find_nul(bytes: &[u8], start: usize) -> Result<usize> {
    bytes[start..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|offset| start + offset)
        .ok_or(RmpegError::UnexpectedEof {
            needed: bytes.len() + 1,
            remaining: bytes.len(),
        })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dimensions {
    width: u32,
    height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_exr(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x76\x2f\x31\x01");
        bytes.extend_from_slice(&[2, 0, 0, 0]);
        append_box2i(&mut bytes, b"dataWindow", min_x, min_y, max_x, max_y);
        bytes.push(0);
        bytes
    }

    fn append_box2i(
        bytes: &mut Vec<u8>,
        name: &[u8],
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    ) {
        bytes.extend_from_slice(name);
        bytes.push(0);
        bytes.extend_from_slice(b"box2i\0");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&min_x.to_le_bytes());
        bytes.extend_from_slice(&min_y.to_le_bytes());
        bytes.extend_from_slice(&max_x.to_le_bytes());
        bytes.extend_from_slice(&max_y.to_le_bytes());
    }

    #[test]
    fn parses_exr_data_window_dimensions() {
        let doc = parse_exr(&minimal_exr(0, 0, 47, 31)).expect("valid exr");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "exr_pipe");
        assert_eq!(stream.codec_name, "exr");
        assert_eq!(stream.width, Some(48));
        assert_eq!(stream.height, Some(32));
        assert_eq!(stream.duration_seconds, Some(0.0));
    }

    #[test]
    fn handles_nonzero_data_window_origin() {
        let doc = parse_exr(&minimal_exr(-4, 10, 5, 19)).expect("valid exr");
        assert_eq!(doc.streams[0].width, Some(10));
        assert_eq!(doc.streams[0].height, Some(10));
    }

    #[test]
    fn prefers_display_window_dimensions() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x76\x2f\x31\x01");
        bytes.extend_from_slice(&[2, 0, 0, 0]);
        append_box2i(&mut bytes, b"dataWindow", 0, 0, 97, 97);
        append_box2i(&mut bytes, b"displayWindow", 0, 0, 49, 49);
        bytes.push(0);

        let doc = parse_exr(&bytes).expect("valid exr");
        assert_eq!(doc.streams[0].width, Some(50));
        assert_eq!(doc.streams[0].height, Some(50));
    }

    #[test]
    fn rejects_inverted_data_window() {
        let err = parse_exr(&minimal_exr(10, 0, 9, 1)).expect_err("inverted window");
        assert!(err.to_string().contains("inverted"));
    }
}
