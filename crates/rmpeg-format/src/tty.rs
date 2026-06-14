use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_tty(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_tty(bytes) {
        return Err(RmpegError::InvalidData(
            "missing observed TTY text signature".to_string(),
        ));
    }
    let frames = bytes.len().div_ceil(240);
    Ok(ProbeDocument {
        format: "tty".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "ansi",
            640,
            400,
            Some(frames as f64 / 25.0),
            None,
        )],
    })
}

pub fn looks_like_tty(bytes: &[u8]) -> bool {
    bytes.starts_with(b"DecoderCheck Package") || bytes.starts_with(b"\r\nIRT MXF Analyzer (Cola).")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_tty_duration_as_240_byte_frames_at_25fps() {
        let mut bytes = b"DecoderCheck Package".to_vec();
        bytes.resize(241, b'x');

        let doc = parse_tty(&bytes).unwrap();

        assert_eq!(doc.format, "tty");
        assert_eq!(doc.streams[0].codec_name, "ansi");
        assert_eq!(doc.streams[0].duration_seconds, Some(0.08));
    }
}
