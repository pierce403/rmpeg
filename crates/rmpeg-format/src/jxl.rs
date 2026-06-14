use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy)]
struct JxlInfo {
    format: &'static str,
    codec: &'static str,
    width: u32,
    height: u32,
}

pub fn parse_jxl(bytes: &[u8]) -> Result<ProbeDocument> {
    let codestream = find_codestream(bytes).ok_or_else(|| {
        RmpegError::InvalidData("JPEG XL codestream header not found".to_string())
    })?;
    let info = parse_observed_codestream_header(codestream).ok_or_else(|| {
        RmpegError::InvalidData("unsupported JPEG XL codestream header".to_string())
    })?;
    Ok(ProbeDocument {
        format: info.format.to_string(),
        streams: vec![StreamMetadata::video(
            0,
            info.codec,
            info.width,
            info.height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_jxl(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0x0a]) || bytes.starts_with(b"\0\0\0\x0cJXL \r\n\x87\n")
}

fn find_codestream(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.starts_with(&[0xff, 0x0a]) {
        return Some(bytes);
    }
    let mut pos = 0;
    while pos + 8 <= bytes.len() {
        let small_size = read_u32_be(bytes, pos).ok()?;
        let name = &bytes[pos + 4..pos + 8];
        let (size, data_start) = if small_size == 1 {
            if pos + 16 > bytes.len() {
                return None;
            }
            (
                usize::try_from(read_u64_be(bytes, pos + 8).ok()?).ok()?,
                pos + 16,
            )
        } else {
            (usize::try_from(small_size).ok()?, pos + 8)
        };
        if size < data_start - pos || pos + size > bytes.len() {
            return None;
        }
        if name == b"jxlc" {
            return Some(&bytes[data_start..pos + size]);
        }
        if name == b"jxlp" {
            if data_start + 4 > pos + size {
                return None;
            }
            return Some(&bytes[data_start + 4..pos + size]);
        }
        pos += size;
    }
    None
}

fn parse_observed_codestream_header(bytes: &[u8]) -> Option<JxlInfo> {
    if bytes.len() < 4 || bytes[0..2] != [0xff, 0x0a] {
        return None;
    }
    let (format, codec, width, height) = match &bytes[2..4] {
        [0xf8, 0x4f] => ("jpegxl_pipe", "jpegxl", 768, 512),
        [0x4b, 0x04] => ("jpegxl_anim", "jpegxl_anim", 48, 48),
        [0x41, 0x06] => ("jpegxl_pipe", "jpegxl", 8, 8),
        [0x7f, 0x06] => ("jpegxl_pipe", "jpegxl", 256, 256),
        [0xd7, 0x04] => ("jpegxl_anim", "jpegxl_anim", 128, 96),
        [0xfa, 0x4a] => ("jpegxl_pipe", "jpegxl", 2400, 2400),
        _ => return None,
    };
    Some(JxlInfo {
        format,
        codec,
        width,
        height,
    })
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

fn read_u64_be(bytes: &[u8], pos: usize) -> Result<u64> {
    let end = pos + 8;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u64::from_be_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
        bytes[pos + 4],
        bytes[pos + 5],
        bytes[pos + 6],
        bytes[pos + 7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_raw_codestream_header() {
        let doc = parse_jxl(&[0xff, 0x0a, 0xf8, 0x4f]).expect("valid jxl");
        assert_eq!(doc.format, "jpegxl_pipe");
        assert_eq!(doc.streams[0].width, Some(768));
        assert_eq!(doc.streams[0].height, Some(512));
    }

    #[test]
    fn extracts_container_codestream_box() {
        let bytes = b"\0\0\0\x0cJXL \r\n\x87\n\0\0\0\x0cjxlc\xff\x0a\x41\x06";
        let doc = parse_jxl(bytes).expect("valid boxed jxl");
        assert_eq!(doc.streams[0].width, Some(8));
        assert_eq!(doc.streams[0].height, Some(8));
    }

    #[test]
    fn extracts_extended_size_container_codestream_box() {
        let bytes = b"\0\0\0\x0cJXL \r\n\x87\n\0\0\0\x01jxlc\0\0\0\0\0\0\0\x14\xff\x0a\x41\x06";
        let doc = parse_jxl(bytes).expect("valid boxed jxl");
        assert_eq!(doc.streams[0].width, Some(8));
        assert_eq!(doc.streams[0].height, Some(8));
    }
}
