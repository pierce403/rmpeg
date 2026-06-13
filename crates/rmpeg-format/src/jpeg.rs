use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_jpeg(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 4 {
        return Err(RmpegError::UnexpectedEof {
            needed: 4,
            remaining: bytes.len(),
        });
    }
    if !looks_like_jpeg(bytes) {
        return Err(RmpegError::InvalidData("missing JPEG SOI".to_string()));
    }

    let dimensions = find_sof_dimensions(bytes)?;
    Ok(ProbeDocument {
        format: "image2".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "mjpeg",
            dimensions.width,
            dimensions.height,
            Some(0.04),
            None,
        )],
    })
}

pub fn looks_like_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\xff\xd8")
}

fn find_sof_dimensions(bytes: &[u8]) -> Result<Dimensions> {
    let mut pos = 2;
    while pos < bytes.len() {
        while bytes.get(pos) == Some(&0xff) {
            pos += 1;
        }
        let marker = *bytes.get(pos).ok_or(RmpegError::UnexpectedEof {
            needed: pos + 1,
            remaining: bytes.len(),
        })?;
        pos += 1;

        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if marker_has_no_payload(marker) {
            continue;
        }

        if pos + 2 > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 2,
                remaining: bytes.len(),
            });
        }
        let segment_len = usize::from(read_u16_be(bytes, pos)?);
        if segment_len < 2 {
            return Err(RmpegError::InvalidData(
                "invalid JPEG segment length".to_string(),
            ));
        }
        let segment_start = pos + 2;
        let segment_end = pos + segment_len;
        if segment_end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: segment_end,
                remaining: bytes.len(),
            });
        }

        if is_start_of_frame(marker) {
            if segment_len < 7 {
                return Err(RmpegError::InvalidData(
                    "truncated JPEG SOF segment".to_string(),
                ));
            }
            let height = u32::from(read_u16_be(bytes, segment_start + 1)?);
            let width = u32::from(read_u16_be(bytes, segment_start + 3)?);
            if width == 0 || height == 0 {
                return Err(RmpegError::InvalidData(
                    "JPEG dimensions must be nonzero".to_string(),
                ));
            }
            return Ok(Dimensions { width, height });
        }

        pos = segment_end;
    }

    Err(RmpegError::InvalidData(
        "missing JPEG start-of-frame segment".to_string(),
    ))
}

fn marker_has_no_payload(marker: u8) -> bool {
    marker == 0x01 || (0xd0..=0xd7).contains(&marker)
}

fn is_start_of_frame(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
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

struct Dimensions {
    width: u32,
    height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_jpeg(marker: u8, width: u16, height: u16) -> Vec<u8> {
        vec![
            0xff,
            0xd8,
            0xff,
            marker,
            0x00,
            0x0b,
            8,
            (height >> 8) as u8,
            height as u8,
            (width >> 8) as u8,
            width as u8,
            3,
            1,
            0x11,
            0,
            0xff,
            0xd9,
        ]
    }

    #[test]
    fn parses_baseline_jpeg_dimensions() {
        let doc = parse_jpeg(&minimal_jpeg(0xc0, 64, 43)).expect("valid jpeg");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "image2");
        assert_eq!(stream.codec_name, "mjpeg");
        assert_eq!(stream.width, Some(64));
        assert_eq!(stream.height, Some(43));
        assert_eq!(stream.duration_seconds, Some(0.04));
    }

    #[test]
    fn parses_progressive_jpeg_dimensions() {
        let doc = parse_jpeg(&minimal_jpeg(0xc2, 128, 72)).expect("valid jpeg");
        assert_eq!(doc.streams[0].width, Some(128));
        assert_eq!(doc.streams[0].height, Some(72));
    }
}
