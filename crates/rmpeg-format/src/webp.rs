use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_webp(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 20 {
        return Err(RmpegError::UnexpectedEof {
            needed: 20,
            remaining: bytes.len(),
        });
    }
    if !looks_like_webp(bytes) {
        return Err(RmpegError::InvalidData(
            "missing WebP RIFF header".to_string(),
        ));
    }

    let dimensions = parse_webp_dimensions(bytes)?;
    Ok(ProbeDocument {
        format: "webp_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "webp",
            dimensions.width,
            dimensions.height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

fn parse_webp_dimensions(bytes: &[u8]) -> Result<Dimensions> {
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let chunk_type = &bytes[pos..pos + 4];
        let chunk_size = read_u32_le(bytes, pos + 4)? as usize;
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(chunk_size)
            .ok_or_else(|| RmpegError::InvalidData("WebP chunk is too large".to_string()))?;
        if data_end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_end,
                remaining: bytes.len(),
            });
        }

        match chunk_type {
            b"ANIM" => {
                return Ok(Dimensions {
                    width: 0,
                    height: 0,
                });
            }
            b"VP8X" => {
                if chunk_size < 10 {
                    return Err(RmpegError::InvalidData(
                        "truncated WebP VP8X chunk".to_string(),
                    ));
                }
                if bytes[data_start] & 0x02 != 0 {
                    return Ok(Dimensions {
                        width: 0,
                        height: 0,
                    });
                }
                return Ok(Dimensions {
                    width: read_u24_le(bytes, data_start + 4)? + 1,
                    height: read_u24_le(bytes, data_start + 7)? + 1,
                });
            }
            b"VP8 " => {
                if chunk_size < 10 {
                    return Err(RmpegError::InvalidData(
                        "truncated WebP VP8 chunk".to_string(),
                    ));
                }
                if bytes.get(data_start + 3..data_start + 6) != Some(b"\x9d\x01\x2a") {
                    return Err(RmpegError::InvalidData(
                        "missing WebP VP8 frame signature".to_string(),
                    ));
                }
                let width = u32::from(read_u16_le(bytes, data_start + 6)? & 0x3fff);
                let height = u32::from(read_u16_le(bytes, data_start + 8)? & 0x3fff);
                return Ok(Dimensions { width, height });
            }
            b"VP8L" => {
                if chunk_size < 5 {
                    return Err(RmpegError::InvalidData(
                        "truncated WebP VP8L chunk".to_string(),
                    ));
                }
                if bytes[data_start] != 0x2f {
                    return Err(RmpegError::InvalidData(
                        "missing WebP VP8L signature".to_string(),
                    ));
                }
                let b0 = u32::from(bytes[data_start + 1]);
                let b1 = u32::from(bytes[data_start + 2]);
                let b2 = u32::from(bytes[data_start + 3]);
                let b3 = u32::from(bytes[data_start + 4]);
                let width = 1 + (((b1 & 0x3f) << 8) | b0);
                let height = 1 + (((b3 & 0x0f) << 10) | (b2 << 2) | ((b1 & 0xc0) >> 6));
                return Ok(Dimensions { width, height });
            }
            _ => {}
        }

        let padding = chunk_size % 2;
        pos = data_end
            .checked_add(padding)
            .ok_or_else(|| RmpegError::InvalidData("WebP chunk is too large".to_string()))?;
    }

    Err(RmpegError::InvalidData(
        "missing WebP image chunk".to_string(),
    ))
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

fn read_u24_le(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 3;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(
        u32::from(bytes[pos])
            | (u32::from(bytes[pos + 1]) << 8)
            | (u32::from(bytes[pos + 2]) << 16),
    )
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

struct Dimensions {
    width: u32,
    height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn riff_webp(chunk_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&((4 + 8 + payload.len()) as u32).to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(chunk_type);
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn parses_vp8x_canvas_dimensions() {
        let payload = [0, 0, 0, 0, 99, 0, 0, 29, 0, 0];
        let doc = parse_webp(&riff_webp(b"VP8X", &payload)).expect("valid webp");
        assert_eq!(doc.format, "webp_pipe");
        assert_eq!(doc.streams[0].codec_name, "webp");
        assert_eq!(doc.streams[0].width, Some(100));
        assert_eq!(doc.streams[0].height, Some(30));
    }

    #[test]
    fn reports_zero_dimensions_for_animation_chunks() {
        let doc = parse_webp(&riff_webp(b"ANIM", &[0; 6])).expect("animated webp");
        assert_eq!(doc.streams[0].width, Some(0));
        assert_eq!(doc.streams[0].height, Some(0));
    }

    #[test]
    fn reports_zero_dimensions_for_vp8x_animation_flag() {
        let payload = [0x02, 0, 0, 0, 99, 0, 0, 29, 0, 0];
        let doc = parse_webp(&riff_webp(b"VP8X", &payload)).expect("animated webp");
        assert_eq!(doc.streams[0].width, Some(0));
        assert_eq!(doc.streams[0].height, Some(0));
    }
}
