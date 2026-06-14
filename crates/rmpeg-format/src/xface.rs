use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_xface(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_xface(bytes) {
        return Err(RmpegError::InvalidData(
            "missing X-Face payload".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "image2".to_string(),
        streams: vec![StreamMetadata::video(0, "xface", 48, 48, Some(0.04), None)],
    })
}

pub fn looks_like_xface(bytes: &[u8]) -> bool {
    let payload = bytes.trim_ascii_end();
    let payload = payload.strip_suffix(&[0]).unwrap_or(payload);
    !payload.is_empty()
        && bytes.len() <= 1024
        && payload
            .iter()
            .all(|byte| byte.is_ascii_graphic() || byte.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extension_gated_xface_as_fixed_size_image() {
        let doc = parse_xface(b"D*,E=#:i&,]R*n\"b").expect("xface");

        assert_eq!(doc.format, "image2");
        assert_eq!(doc.streams[0].codec_name, "xface");
        assert_eq!(doc.streams[0].width, Some(48));
        assert_eq!(doc.streams[0].height, Some(48));
        assert_eq!(doc.streams[0].duration_seconds, Some(0.04));
    }
}
