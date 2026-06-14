use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_xbm(bytes: &[u8]) -> Result<ProbeDocument> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| RmpegError::InvalidData("XBM header is not valid UTF-8".to_string()))?;
    let mut width = None;
    let mut height = None;

    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() != Some("#define") {
            continue;
        }
        let Some(name) = parts.next() else {
            continue;
        };
        let Some(value) = parts.next() else {
            continue;
        };
        if name.ends_with("_width") {
            width = Some(parse_u32(value, "XBM width")?);
        } else if name.ends_with("_height") {
            height = Some(parse_u32(value, "XBM height")?);
        }
    }

    if !(text.contains("_bits") && text.contains("static")) {
        return Err(RmpegError::InvalidData(
            "XBM bitmap array was not found".to_string(),
        ));
    }
    let (Some(width), Some(height)) = (width, height) else {
        return Err(RmpegError::InvalidData(
            "XBM dimensions were not found".to_string(),
        ));
    };
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "XBM dimensions must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "xbm_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "xbm",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_xbm(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes.get(..bytes.len().min(4096)).unwrap_or(bytes)) else {
        return false;
    };
    text.contains("#define")
        && text.contains("_width")
        && text.contains("_height")
        && text.contains("_bits")
}

fn parse_u32(value: &str, label: &str) -> Result<u32> {
    value
        .parse()
        .map_err(|_| RmpegError::InvalidData(format!("{label} is not a valid integer")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_xbm_dimensions() {
        let doc = parse_xbm(
            b"#define image_width 200\n#define image_height 190\nstatic unsigned char image_bits[] = { 0x00 };\n",
        )
        .expect("xbm");

        assert_eq!(doc.format, "xbm_pipe");
        assert_eq!(doc.streams[0].codec_name, "xbm");
        assert_eq!(doc.streams[0].width, Some(200));
        assert_eq!(doc.streams[0].height, Some(190));
        assert_eq!(doc.streams[0].duration_seconds, Some(0.0));
    }

    #[test]
    fn rejects_missing_bitmap_array() {
        let error = parse_xbm(b"#define image_width 1\n#define image_height 1\n")
            .expect_err("missing bitmap array");

        assert!(error.to_string().contains("bitmap array"));
    }
}
