use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_alg_mm(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 16 || bytes[2] != 0x18 {
        return Err(RmpegError::InvalidData(
            "missing ALG MM header shape".to_string(),
        ));
    }
    let width = u32::from(bytes[13]) * 256;
    let height = u32::from(bytes[14]);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid ALG MM dimensions".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "mm".to_string(),
        streams: vec![
            StreamMetadata::video(0, "mmvideo", width, height, Some(0.0), None),
            StreamMetadata::audio(1, "pcm_u8", 8_000, 1, 8, 0.0),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extension_gated_alg_mm_dimensions() {
        let mut bytes = vec![0; 16];
        bytes[2] = 0x18;
        bytes[13] = 1;
        bytes[14] = 160;

        let doc = parse_alg_mm(&bytes).expect("mm");

        assert_eq!(doc.format, "mm");
        assert_eq!(doc.streams[0].codec_name, "mmvideo");
        assert_eq!(doc.streams[0].width, Some(256));
        assert_eq!(doc.streams[1].codec_name, "pcm_u8");
    }
}
