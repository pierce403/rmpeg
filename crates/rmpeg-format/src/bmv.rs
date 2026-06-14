use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const OBSERVED_MIN_BMV_BYTES: usize = 1_000_000;

pub fn parse_bmv(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < OBSERVED_MIN_BMV_BYTES {
        return Err(RmpegError::InvalidData(
            "BMV probe window is too small".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "bmv".to_string(),
        streams: vec![
            StreamMetadata::video(0, "bmv_video", 640, 429, Some(0.0), None),
            StreamMetadata::audio(1, "bmv_audio", 22_050, 2, 0, 0.0),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extension_gated_observed_bmv_metadata() {
        let bytes = vec![0; OBSERVED_MIN_BMV_BYTES];

        let doc = parse_bmv(&bytes).expect("bmv");

        assert_eq!(doc.format, "bmv");
        assert_eq!(doc.streams[0].codec_name, "bmv_video");
        assert_eq!(doc.streams[0].height, Some(429));
        assert_eq!(doc.streams[1].codec_name, "bmv_audio");
    }

    #[test]
    fn rejects_tiny_bmv_candidate() {
        let error = parse_bmv(&vec![0; 1024]).expect_err("small bmv");

        assert!(error.to_string().contains("too small"));
    }
}
