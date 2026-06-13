use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const TGA_FOOTER_LEN: usize = 26;
const TGA_FOOTER_SIGNATURE: &[u8; 18] = b"TRUEVISION-XFILE.\0";

pub fn parse_tga(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 18 + TGA_FOOTER_LEN {
        return Err(RmpegError::UnexpectedEof {
            needed: 18 + TGA_FOOTER_LEN,
            remaining: bytes.len(),
        });
    }
    if !looks_like_tga(bytes) {
        return Err(RmpegError::InvalidData(
            "missing TGA 2.0 footer signature".to_string(),
        ));
    }

    let header = TgaHeader::parse(bytes)?;

    if !matches!(header.color_map_type, 0 | 1) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported TGA color map type {}",
            header.color_map_type
        )));
    }
    if !matches!(header.image_type, 1 | 2 | 3 | 9 | 10 | 11) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported TGA image type {}",
            header.image_type
        )));
    }
    if header.width == 0 || header.height == 0 {
        return Err(RmpegError::InvalidData(
            "TGA dimensions must be nonzero".to_string(),
        ));
    }
    if header.descriptor & 0xc0 != 0 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported TGA descriptor {:#04x}",
            header.descriptor
        )));
    }

    match header.image_type {
        1 | 9 => {
            if header.color_map_type != 1 || header.color_map_len == 0 {
                return Err(RmpegError::InvalidData(
                    "color-mapped TGA requires a color map".to_string(),
                ));
            }
            if !matches!(header.pixel_depth, 8 | 15 | 16) {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported TGA index depth {}",
                    header.pixel_depth
                )));
            }
            if !matches!(header.color_map_depth, 15 | 16 | 24 | 32) {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported TGA color map depth {}",
                    header.color_map_depth
                )));
            }
        }
        2 | 10 => {
            if header.color_map_type != 0 || header.color_map_len != 0 {
                return Err(RmpegError::InvalidData(
                    "true-color TGA must not declare a color map".to_string(),
                ));
            }
            if !matches!(header.pixel_depth, 15 | 16 | 24 | 32) {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported TGA pixel depth {}",
                    header.pixel_depth
                )));
            }
        }
        3 | 11 => {
            if header.color_map_type != 0 || header.color_map_len != 0 {
                return Err(RmpegError::InvalidData(
                    "grayscale TGA must not declare a color map".to_string(),
                ));
            }
            if !matches!(header.pixel_depth, 8 | 16) {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported TGA grayscale depth {}",
                    header.pixel_depth
                )));
            }
        }
        _ => unreachable!("image type already validated"),
    }

    validate_payload_bounds(bytes, &header)?;

    Ok(ProbeDocument {
        format: "image2".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "targa",
            header.width,
            header.height,
            Some(0.04),
            None,
        )],
    })
}

pub fn looks_like_tga(bytes: &[u8]) -> bool {
    bytes.len() >= TGA_FOOTER_LEN
        && &bytes[bytes.len() - TGA_FOOTER_SIGNATURE.len()..] == TGA_FOOTER_SIGNATURE
}

struct TgaHeader {
    id_len: usize,
    color_map_len: usize,
    color_map_type: u8,
    color_map_depth: u8,
    image_type: u8,
    width: u32,
    height: u32,
    pixel_depth: u8,
    descriptor: u8,
}

impl TgaHeader {
    fn parse(bytes: &[u8]) -> Result<Self> {
        Ok(Self {
            id_len: usize::from(bytes[0]),
            color_map_type: bytes[1],
            image_type: bytes[2],
            color_map_len: usize::from(read_u16_le(bytes, 5)?),
            color_map_depth: bytes[7],
            width: u32::from(read_u16_le(bytes, 12)?),
            height: u32::from(read_u16_le(bytes, 14)?),
            pixel_depth: bytes[16],
            descriptor: bytes[17],
        })
    }
}

fn validate_payload_bounds(bytes: &[u8], header: &TgaHeader) -> Result<()> {
    let image_end = bytes.len() - TGA_FOOTER_LEN;
    let color_map_bytes = header
        .color_map_len
        .checked_mul(bytes_per_pixel(header.color_map_depth))
        .ok_or_else(|| RmpegError::InvalidData("TGA color map is too large".to_string()))?;
    let pixel_start = 18usize
        .checked_add(header.id_len)
        .and_then(|pos| pos.checked_add(color_map_bytes))
        .ok_or_else(|| RmpegError::InvalidData("TGA header offsets overflow".to_string()))?;
    if pixel_start > image_end {
        return Err(RmpegError::UnexpectedEof {
            needed: pixel_start,
            remaining: image_end,
        });
    }

    if matches!(header.image_type, 1..=3) {
        let pixels = usize::try_from(header.width)
            .ok()
            .and_then(|w| {
                usize::try_from(header.height)
                    .ok()
                    .and_then(|h| w.checked_mul(h))
            })
            .ok_or_else(|| RmpegError::InvalidData("TGA dimensions are too large".to_string()))?;
        let pixel_bytes = pixels
            .checked_mul(bytes_per_pixel(header.pixel_depth))
            .ok_or_else(|| RmpegError::InvalidData("TGA image data is too large".to_string()))?;
        let needed = pixel_start
            .checked_add(pixel_bytes)
            .ok_or_else(|| RmpegError::InvalidData("TGA image data overflows".to_string()))?;
        if needed > image_end {
            return Err(RmpegError::UnexpectedEof {
                needed,
                remaining: image_end,
            });
        }
    } else if pixel_start == image_end {
        return Err(RmpegError::UnexpectedEof {
            needed: pixel_start + 1,
            remaining: image_end,
        });
    }

    Ok(())
}

fn bytes_per_pixel(bits: u8) -> usize {
    usize::from(bits).div_ceil(8)
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

    fn minimal_tga(image_type: u8, width: u16, height: u16, pixel_depth: u8) -> Vec<u8> {
        let mut bytes = vec![0; 18];
        bytes[2] = image_type;
        bytes[12..14].copy_from_slice(&width.to_le_bytes());
        bytes[14..16].copy_from_slice(&height.to_le_bytes());
        bytes[16] = pixel_depth;

        let pixel_count = usize::from(width) * usize::from(height);
        let pixel_bytes = if matches!(image_type, 1 | 2 | 3) {
            pixel_count * bytes_per_pixel(pixel_depth)
        } else {
            1
        };
        bytes.resize(18 + pixel_bytes, 0);
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(TGA_FOOTER_SIGNATURE);
        bytes
    }

    #[test]
    fn parses_truecolor_tga_dimensions() {
        let doc = parse_tga(&minimal_tga(2, 128, 64, 24)).expect("valid tga");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "image2");
        assert_eq!(stream.codec_name, "targa");
        assert_eq!(stream.width, Some(128));
        assert_eq!(stream.height, Some(64));
        assert_eq!(stream.duration_seconds, Some(0.04));
    }

    #[test]
    fn accepts_rle_tga_with_footer() {
        let doc = parse_tga(&minimal_tga(10, 16, 16, 24)).expect("valid rle tga");
        assert_eq!(doc.streams[0].width, Some(16));
    }

    #[test]
    fn rejects_tga_without_footer_signature() {
        let mut bytes = minimal_tga(2, 1, 1, 24);
        bytes.pop();
        bytes.push(0xff);
        let err = parse_tga(&bytes).expect_err("missing footer");
        assert!(err.to_string().contains("footer"));
    }

    #[test]
    fn rejects_truncated_uncompressed_pixels() {
        let mut bytes = minimal_tga(2, 2, 2, 24);
        let footer = bytes.split_off(bytes.len() - TGA_FOOTER_LEN);
        bytes.truncate(18);
        bytes.extend_from_slice(&footer);
        let err = parse_tga(&bytes).expect_err("truncated pixels");
        assert!(err.to_string().contains("unexpected end"));
    }
}
