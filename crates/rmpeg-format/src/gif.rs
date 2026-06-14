use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_gif(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_gif(bytes) {
        return Err(RmpegError::InvalidData("missing GIF signature".to_string()));
    }
    if bytes.len() < 13 {
        return Err(RmpegError::UnexpectedEof {
            needed: 13,
            remaining: bytes.len(),
        });
    }

    let width = u32::from(read_u16_le(bytes, 6)?);
    let height = u32::from(read_u16_le(bytes, 8)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "GIF dimensions must be nonzero".to_string(),
        ));
    }

    let timing = parse_blocks(bytes)?;
    let duration_seconds = if timing.delay_centiseconds > 0 {
        timing.delay_centiseconds as f64 / 100.0
    } else if timing.frame_count > 0 {
        timing.frame_count as f64 / 10.0
    } else {
        0.0
    };

    Ok(ProbeDocument {
        format: "gif".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "gif",
            width,
            height,
            Some(duration_seconds),
            None,
        )],
    })
}

pub fn looks_like_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

#[derive(Debug, Default, PartialEq, Eq)]
struct GifTiming {
    frame_count: u32,
    delay_centiseconds: u64,
}

fn parse_blocks(bytes: &[u8]) -> Result<GifTiming> {
    let mut pos: usize = 13;
    if has_global_color_table(bytes[10]) {
        let table_len = color_table_len(bytes[10])?;
        pos = pos.checked_add(table_len).ok_or_else(|| {
            RmpegError::InvalidData("GIF global color table is too large".to_string())
        })?;
        if pos > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos,
                remaining: bytes.len(),
            });
        }
    }

    let mut timing = GifTiming::default();
    loop {
        let Some(&marker) = bytes.get(pos) else {
            return Ok(timing);
        };
        pos += 1;
        match marker {
            0x21 => {
                let Some(&label) = bytes.get(pos) else {
                    return Err(RmpegError::UnexpectedEof {
                        needed: pos + 1,
                        remaining: bytes.len(),
                    });
                };
                pos += 1;
                if label == 0xF9 {
                    pos = parse_graphic_control_extension(bytes, pos, &mut timing)?;
                } else {
                    pos = skip_sub_blocks(bytes, pos)?;
                }
            }
            0x2C => {
                timing.frame_count = timing.frame_count.saturating_add(1);
                pos = parse_image_descriptor(bytes, pos)?;
            }
            0x3B => return Ok(timing),
            _ => {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported GIF block marker 0x{marker:02x}"
                )));
            }
        }
    }
}

fn parse_graphic_control_extension(
    bytes: &[u8],
    pos: usize,
    timing: &mut GifTiming,
) -> Result<usize> {
    let Some(&block_size) = bytes.get(pos) else {
        return Err(RmpegError::UnexpectedEof {
            needed: pos + 1,
            remaining: bytes.len(),
        });
    };
    if block_size != 4 {
        return Err(RmpegError::InvalidData(format!(
            "unexpected GIF graphic control block size {block_size}"
        )));
    }
    let data_start = pos + 1;
    let data_end = data_start + 4;
    if data_end + 1 > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_end + 1,
            remaining: bytes.len(),
        });
    }
    timing.delay_centiseconds = timing
        .delay_centiseconds
        .saturating_add(u64::from(read_u16_le(bytes, data_start + 1)?));
    if bytes[data_end] != 0 {
        return Err(RmpegError::InvalidData(
            "GIF graphic control extension is not terminated".to_string(),
        ));
    }
    Ok(data_end + 1)
}

fn parse_image_descriptor(bytes: &[u8], pos: usize) -> Result<usize> {
    let descriptor_end = pos + 9;
    if descriptor_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: descriptor_end,
            remaining: bytes.len(),
        });
    }
    let packed = bytes[pos + 8];
    let mut data_pos = descriptor_end;
    if has_global_color_table(packed) {
        let table_len = color_table_len(packed)?;
        data_pos = data_pos.checked_add(table_len).ok_or_else(|| {
            RmpegError::InvalidData("GIF local color table is too large".to_string())
        })?;
        if data_pos > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_pos,
                remaining: bytes.len(),
            });
        }
    }
    if data_pos >= bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_pos + 1,
            remaining: bytes.len(),
        });
    }
    skip_sub_blocks(bytes, data_pos + 1)
}

fn skip_sub_blocks(bytes: &[u8], mut pos: usize) -> Result<usize> {
    loop {
        let Some(&len) = bytes.get(pos) else {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 1,
                remaining: bytes.len(),
            });
        };
        pos += 1;
        if len == 0 {
            return Ok(pos);
        }
        pos = pos
            .checked_add(usize::from(len))
            .ok_or_else(|| RmpegError::InvalidData("GIF sub-block is too large".to_string()))?;
        if pos > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos,
                remaining: bytes.len(),
            });
        }
    }
}

fn has_global_color_table(packed: u8) -> bool {
    packed & 0x80 != 0
}

fn color_table_len(packed: u8) -> Result<usize> {
    3usize
        .checked_mul(1usize << (usize::from(packed & 0x07) + 1))
        .ok_or_else(|| RmpegError::InvalidData("GIF color table is too large".to_string()))
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

    fn gif_header(width: u16, height: u16, packed: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&[packed, 0, 0]);
        if has_global_color_table(packed) {
            bytes.extend(std::iter::repeat_n(0, color_table_len(packed).unwrap()));
        }
        bytes
    }

    fn image_block() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(0x2C);
        bytes.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0, 0]);
        bytes.push(2);
        bytes.push(1);
        bytes.push(0);
        bytes.push(0);
        bytes
    }

    fn graphic_control(delay: u16) -> Vec<u8> {
        let mut bytes = vec![0x21, 0xF9, 4, 0];
        bytes.extend_from_slice(&delay.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes
    }

    #[test]
    fn parses_gif_dimensions_and_delay_duration() {
        let mut bytes = gif_header(32, 12, 0x80);
        bytes.extend_from_slice(&graphic_control(25));
        bytes.extend_from_slice(&image_block());
        bytes.push(0x3B);

        let doc = parse_gif(&bytes).expect("valid gif");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "gif");
        assert_eq!(stream.codec_name, "gif");
        assert_eq!(stream.width, Some(32));
        assert_eq!(stream.height, Some(12));
        assert_eq!(stream.duration_seconds, Some(0.25));
    }

    #[test]
    fn uses_ten_fps_fallback_when_all_delays_are_zero() {
        let mut bytes = gif_header(1, 1, 0);
        bytes.extend_from_slice(&image_block());
        bytes.extend_from_slice(&image_block());
        bytes.push(0x3B);

        let doc = parse_gif(&bytes).expect("valid gif");
        assert_eq!(doc.streams[0].duration_seconds, Some(0.2));
    }
}
