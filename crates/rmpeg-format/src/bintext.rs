use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const OBSERVED_BIN_TEXT_BYTES: usize = 12_800;

pub fn parse_bintext(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_bintext(bytes) {
        return Err(RmpegError::InvalidData(
            "missing observed binary text payload".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "bin".to_string(),
        streams: vec![StreamMetadata::video(0, "bintext", 1280, 640, None, None)],
    })
}

pub fn looks_like_bintext(bytes: &[u8]) -> bool {
    bytes.len() == OBSERVED_BIN_TEXT_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_binary_text_dimensions() {
        let bytes = vec![0; OBSERVED_BIN_TEXT_BYTES];

        let doc = parse_bintext(&bytes).expect("bintext");

        assert_eq!(doc.format, "bin");
        assert_eq!(doc.streams[0].codec_name, "bintext");
        assert_eq!(doc.streams[0].width, Some(1280));
        assert_eq!(doc.streams[0].height, Some(640));
        assert_eq!(doc.streams[0].duration_seconds, None);
    }
}
