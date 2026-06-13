use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const TAG_IMAGE_WIDTH: u16 = 256;
const TAG_IMAGE_LENGTH: u16 = 257;
const TYPE_SHORT: u16 = 3;
const TYPE_LONG: u16 = 4;

pub fn parse_tiff(bytes: &[u8]) -> Result<ProbeDocument> {
    let endian = tiff_endian(bytes).ok_or_else(|| {
        if bytes.len() < 4 {
            RmpegError::UnexpectedEof {
                needed: 4,
                remaining: bytes.len(),
            }
        } else {
            RmpegError::InvalidData("missing TIFF signature".to_string())
        }
    })?;
    if bytes.len() < 8 {
        return Err(RmpegError::UnexpectedEof {
            needed: 8,
            remaining: bytes.len(),
        });
    }

    let ifd_offset = usize::try_from(read_u32(bytes, 4, endian)?)
        .map_err(|_| RmpegError::InvalidData("TIFF IFD offset is too large".to_string()))?;
    let (width, height) = parse_ifd_dimensions(bytes, ifd_offset, endian)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "TIFF dimensions must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "tiff_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "tiff",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_tiff(bytes: &[u8]) -> bool {
    tiff_endian(bytes).is_some()
}

fn parse_ifd_dimensions(bytes: &[u8], ifd_offset: usize, endian: Endian) -> Result<(u32, u32)> {
    let entry_count = usize::from(read_u16(bytes, ifd_offset, endian)?);
    let entries_start = ifd_offset + 2;
    let entries_bytes = entry_count
        .checked_mul(12)
        .ok_or_else(|| RmpegError::InvalidData("TIFF IFD entry count is too large".to_string()))?;
    let entries_end = entries_start
        .checked_add(entries_bytes)
        .ok_or_else(|| RmpegError::InvalidData("TIFF IFD entries overflow".to_string()))?;
    if entries_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: entries_end,
            remaining: bytes.len(),
        });
    }

    let mut width = None;
    let mut height = None;
    for index in 0..entry_count {
        let entry = entries_start + index * 12;
        let tag = read_u16(bytes, entry, endian)?;
        if tag != TAG_IMAGE_WIDTH && tag != TAG_IMAGE_LENGTH {
            continue;
        }
        let value = read_ifd_scalar(bytes, entry, endian)?;
        match tag {
            TAG_IMAGE_WIDTH => width = Some(value),
            TAG_IMAGE_LENGTH => height = Some(value),
            _ => {}
        }
    }

    let width =
        width.ok_or_else(|| RmpegError::InvalidData("missing TIFF ImageWidth".to_string()))?;
    let height =
        height.ok_or_else(|| RmpegError::InvalidData("missing TIFF ImageLength".to_string()))?;
    Ok((width, height))
}

fn read_ifd_scalar(bytes: &[u8], entry: usize, endian: Endian) -> Result<u32> {
    let value_type = read_u16(bytes, entry + 2, endian)?;
    let value_count = read_u32(bytes, entry + 4, endian)?;
    if value_count == 0 {
        return Err(RmpegError::InvalidData(
            "TIFF tag has zero values".to_string(),
        ));
    }

    match value_type {
        TYPE_SHORT => {
            if value_count == 1 {
                Ok(u32::from(read_u16(bytes, entry + 8, endian)?))
            } else {
                let offset =
                    usize::try_from(read_u32(bytes, entry + 8, endian)?).map_err(|_| {
                        RmpegError::InvalidData("TIFF SHORT value offset is too large".to_string())
                    })?;
                Ok(u32::from(read_u16(bytes, offset, endian)?))
            }
        }
        TYPE_LONG => {
            if value_count == 1 {
                read_u32(bytes, entry + 8, endian)
            } else {
                let offset =
                    usize::try_from(read_u32(bytes, entry + 8, endian)?).map_err(|_| {
                        RmpegError::InvalidData("TIFF LONG value offset is too large".to_string())
                    })?;
                read_u32(bytes, offset, endian)
            }
        }
        _ => Err(RmpegError::InvalidData(format!(
            "unsupported TIFF dimension tag type {value_type}"
        ))),
    }
}

fn tiff_endian(bytes: &[u8]) -> Option<Endian> {
    if bytes.len() < 4 {
        return None;
    }
    match &bytes[0..4] {
        b"II\x2a\x00" => Some(Endian::Little),
        b"MM\x00\x2a" => Some(Endian::Big),
        _ => None,
    }
}

fn read_u16(bytes: &[u8], pos: usize, endian: Endian) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    let raw = [bytes[pos], bytes[pos + 1]];
    Ok(match endian {
        Endian::Little => u16::from_le_bytes(raw),
        Endian::Big => u16::from_be_bytes(raw),
    })
}

fn read_u32(bytes: &[u8], pos: usize, endian: Endian) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    let raw = [bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]];
    Ok(match endian {
        Endian::Little => u32::from_le_bytes(raw),
        Endian::Big => u32::from_be_bytes(raw),
    })
}

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_tiff(width: u32, height: u32, endian: Endian) -> Vec<u8> {
        let mut bytes = Vec::new();
        match endian {
            Endian::Little => bytes.extend_from_slice(b"II\x2a\x00"),
            Endian::Big => bytes.extend_from_slice(b"MM\x00\x2a"),
        }
        push_u32(&mut bytes, 8, endian);
        push_u16(&mut bytes, 2, endian);
        push_ifd_long(&mut bytes, TAG_IMAGE_WIDTH, width, endian);
        push_ifd_long(&mut bytes, TAG_IMAGE_LENGTH, height, endian);
        push_u32(&mut bytes, 0, endian);
        bytes
    }

    fn push_ifd_long(bytes: &mut Vec<u8>, tag: u16, value: u32, endian: Endian) {
        push_u16(bytes, tag, endian);
        push_u16(bytes, TYPE_LONG, endian);
        push_u32(bytes, 1, endian);
        push_u32(bytes, value, endian);
    }

    fn push_u16(bytes: &mut Vec<u8>, value: u16, endian: Endian) {
        match endian {
            Endian::Little => bytes.extend_from_slice(&value.to_le_bytes()),
            Endian::Big => bytes.extend_from_slice(&value.to_be_bytes()),
        }
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32, endian: Endian) {
        match endian {
            Endian::Little => bytes.extend_from_slice(&value.to_le_bytes()),
            Endian::Big => bytes.extend_from_slice(&value.to_be_bytes()),
        }
    }

    #[test]
    fn parses_little_endian_tiff_dimensions() {
        let doc = parse_tiff(&minimal_tiff(640, 480, Endian::Little)).expect("valid tiff");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "tiff_pipe");
        assert_eq!(stream.codec_name, "tiff");
        assert_eq!(stream.width, Some(640));
        assert_eq!(stream.height, Some(480));
    }

    #[test]
    fn parses_big_endian_tiff_dimensions() {
        let doc = parse_tiff(&minimal_tiff(2464, 3248, Endian::Big)).expect("valid tiff");
        let stream = &doc.streams[0];
        assert_eq!(stream.width, Some(2464));
        assert_eq!(stream.height, Some(3248));
    }

    #[test]
    fn rejects_missing_height() {
        let mut bytes = minimal_tiff(1, 1, Endian::Little);
        bytes[22..24].copy_from_slice(&999_u16.to_le_bytes());
        let err = parse_tiff(&bytes).expect_err("missing height");
        assert!(err.to_string().contains("ImageLength"));
    }
}
