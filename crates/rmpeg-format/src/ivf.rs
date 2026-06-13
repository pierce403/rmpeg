use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_ivf(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 32 {
        return Err(RmpegError::UnexpectedEof {
            needed: 32,
            remaining: bytes.len(),
        });
    }
    if &bytes[0..4] != b"DKIF" {
        return Err(RmpegError::InvalidData("missing IVF signature".to_string()));
    }
    let version = read_u16(bytes, 4)?;
    if version != 0 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported IVF version {version}"
        )));
    }
    let header_size = read_u16(bytes, 6)?;
    if header_size < 32 {
        return Err(RmpegError::InvalidData(format!(
            "invalid IVF header size {header_size}"
        )));
    }
    let codec_name = codec_name(&bytes[8..12])?;
    let width = u32::from(read_u16(bytes, 12)?);
    let height = u32::from(read_u16(bytes, 14)?);
    let timebase_den = read_u32(bytes, 16)?;
    let timebase_num = read_u32(bytes, 20)?;
    let frame_count = read_u32(bytes, 24)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "IVF dimensions must be nonzero".to_string(),
        ));
    }
    if timebase_den == 0 {
        return Err(RmpegError::InvalidData(
            "IVF timebase denominator must be nonzero".to_string(),
        ));
    }
    let duration_seconds = if timebase_num == 0 {
        Some(0.0)
    } else {
        Some(frame_count as f64 * timebase_num as f64 / timebase_den as f64)
    };

    Ok(ProbeDocument {
        format: "ivf".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            codec_name,
            width,
            height,
            duration_seconds,
            None,
        )],
    })
}

fn codec_name(fourcc: &[u8]) -> Result<&'static str> {
    match fourcc {
        b"VP80" => Ok("vp8"),
        b"VP90" => Ok("vp9"),
        b"AV01" => Ok("av1"),
        other => Err(RmpegError::Unsupported(format!(
            "IVF codec {} is not supported",
            String::from_utf8_lossy(other)
        ))),
    }
}

fn read_u16(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_u32(bytes: &[u8], pos: usize) -> Result<u32> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_ivf(fourcc: &[u8; 4]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"DKIF");
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&32_u16.to_le_bytes());
        bytes.extend_from_slice(fourcc);
        bytes.extend_from_slice(&176_u16.to_le_bytes());
        bytes.extend_from_slice(&144_u16.to_le_bytes());
        bytes.extend_from_slice(&30_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&29_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_vp8_header_metadata() {
        let doc = parse_ivf(&minimal_ivf(b"VP80")).expect("valid ivf");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "ivf");
        assert_eq!(stream.codec_name, "vp8");
        assert_eq!(stream.width, Some(176));
        assert_eq!(stream.height, Some(144));
        assert_eq!(stream.duration_seconds, Some(29.0 / 30.0));
    }

    #[test]
    fn rejects_unknown_codec() {
        let err = parse_ivf(&minimal_ivf(b"XXXX")).expect_err("unsupported codec");
        assert!(err.to_string().contains("not supported"));
    }
}
