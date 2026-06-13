use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_jpeg2000_codestream(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 4 {
        return Err(RmpegError::UnexpectedEof {
            needed: 4,
            remaining: bytes.len(),
        });
    }
    if !looks_like_jpeg2000_codestream(bytes) {
        return Err(RmpegError::InvalidData(
            "missing JPEG 2000 codestream signature".to_string(),
        ));
    }

    let dimensions = parse_siz_dimensions(bytes)?;
    Ok(ProbeDocument {
        format: "j2k_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "jpeg2000",
            dimensions.width,
            dimensions.height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_jpeg2000_codestream(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\xff\x4f")
}

fn parse_siz_dimensions(bytes: &[u8]) -> Result<Dimensions> {
    let mut pos = 2;
    while pos + 2 <= bytes.len() {
        if bytes[pos] != 0xff {
            return Err(RmpegError::InvalidData(
                "expected JPEG 2000 marker".to_string(),
            ));
        }
        let marker = bytes[pos + 1];
        pos += 2;

        if marker == 0x51 {
            let segment_len = usize::from(read_u16_be(bytes, pos)?);
            if segment_len < 41 {
                return Err(RmpegError::InvalidData(
                    "truncated JPEG 2000 SIZ segment".to_string(),
                ));
            }
            let segment_start = pos + 2;
            let segment_end = pos.checked_add(segment_len).ok_or_else(|| {
                RmpegError::InvalidData("JPEG 2000 segment too large".to_string())
            })?;
            if segment_end > bytes.len() {
                return Err(RmpegError::UnexpectedEof {
                    needed: segment_end,
                    remaining: bytes.len(),
                });
            }
            return dimensions_from_siz(bytes, segment_start);
        }

        if marker_has_no_payload(marker) {
            continue;
        }
        if marker == 0x93 || marker == 0xd9 {
            break;
        }

        let segment_len = usize::from(read_u16_be(bytes, pos)?);
        if segment_len < 2 {
            return Err(RmpegError::InvalidData(
                "invalid JPEG 2000 segment length".to_string(),
            ));
        }
        pos = pos
            .checked_add(segment_len)
            .ok_or_else(|| RmpegError::InvalidData("JPEG 2000 segment too large".to_string()))?;
    }

    Err(RmpegError::InvalidData(
        "missing JPEG 2000 SIZ segment".to_string(),
    ))
}

fn dimensions_from_siz(bytes: &[u8], pos: usize) -> Result<Dimensions> {
    let xsiz = read_u32_be(bytes, pos + 2)?;
    let ysiz = read_u32_be(bytes, pos + 6)?;
    let xosiz = read_u32_be(bytes, pos + 10)?;
    let yosiz = read_u32_be(bytes, pos + 14)?;
    let component_count = read_u16_be(bytes, pos + 34)?;
    if component_count == 0 {
        return Err(RmpegError::InvalidData(
            "JPEG 2000 component count must be nonzero".to_string(),
        ));
    }
    let xrsiz = u32::from(
        bytes
            .get(pos + 37)
            .copied()
            .ok_or(RmpegError::UnexpectedEof {
                needed: pos + 38,
                remaining: bytes.len(),
            })?,
    );
    let yrsiz = u32::from(
        bytes
            .get(pos + 38)
            .copied()
            .ok_or(RmpegError::UnexpectedEof {
                needed: pos + 39,
                remaining: bytes.len(),
            })?,
    );
    if xrsiz == 0 || yrsiz == 0 {
        return Err(RmpegError::InvalidData(
            "JPEG 2000 component subsampling must be nonzero".to_string(),
        ));
    }

    let width = ceil_div(xsiz, xrsiz)
        .checked_sub(ceil_div(xosiz, xrsiz))
        .ok_or_else(|| {
            RmpegError::InvalidData("JPEG 2000 Xsiz is smaller than XOsiz".to_string())
        })?;
    let height = ceil_div(ysiz, yrsiz)
        .checked_sub(ceil_div(yosiz, yrsiz))
        .ok_or_else(|| {
            RmpegError::InvalidData("JPEG 2000 Ysiz is smaller than YOsiz".to_string())
        })?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "JPEG 2000 dimensions must be nonzero".to_string(),
        ));
    }
    Ok(Dimensions { width, height })
}

fn ceil_div(value: u32, divisor: u32) -> u32 {
    value.div_ceil(divisor)
}

fn marker_has_no_payload(marker: u8) -> bool {
    marker == 0x4f || marker == 0x01
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

struct Dimensions {
    width: u32,
    height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_j2k(xsiz: u32, ysiz: u32, xosiz: u32, yosiz: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\xff\x4f\xff\x51");
        bytes.extend_from_slice(&41_u16.to_be_bytes());
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&xsiz.to_be_bytes());
        bytes.extend_from_slice(&ysiz.to_be_bytes());
        bytes.extend_from_slice(&xosiz.to_be_bytes());
        bytes.extend_from_slice(&yosiz.to_be_bytes());
        bytes.extend_from_slice(&xsiz.to_be_bytes());
        bytes.extend_from_slice(&ysiz.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&[7, 1, 1]);
        bytes
    }

    #[test]
    fn parses_codestream_dimensions() {
        let doc = parse_jpeg2000_codestream(&minimal_j2k(128, 64, 0, 0)).expect("valid j2k");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "j2k_pipe");
        assert_eq!(stream.codec_name, "jpeg2000");
        assert_eq!(stream.width, Some(128));
        assert_eq!(stream.height, Some(64));
        assert_eq!(stream.duration_seconds, Some(0.0));
    }

    #[test]
    fn subtracts_image_origin() {
        let doc = parse_jpeg2000_codestream(&minimal_j2k(130, 70, 2, 6)).expect("valid j2k");
        assert_eq!(doc.streams[0].width, Some(128));
        assert_eq!(doc.streams[0].height, Some(64));
    }

    #[test]
    fn applies_first_component_subsampling() {
        let mut bytes = minimal_j2k(127, 126, 0, 0);
        bytes[43] = 2;
        bytes[44] = 1;

        let doc = parse_jpeg2000_codestream(&bytes).expect("valid j2k");
        assert_eq!(doc.streams[0].width, Some(64));
        assert_eq!(doc.streams[0].height, Some(126));
    }

    #[test]
    fn rejects_inverted_dimensions() {
        let err = parse_jpeg2000_codestream(&minimal_j2k(1, 10, 2, 0)).expect_err("bad j2k");
        assert!(err.to_string().contains("Xsiz"));
    }
}
