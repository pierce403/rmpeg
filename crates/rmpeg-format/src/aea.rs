use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const AEA_HEADER_SIZE: usize = 2048;
const AEA_OBSERVED_BYTES_PER_SECOND: f64 = 36_500.0;

pub fn parse_aea(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() <= AEA_HEADER_SIZE {
        return Err(RmpegError::UnexpectedEof {
            needed: AEA_HEADER_SIZE + 1,
            remaining: bytes.len(),
        });
    }
    if bytes.get(0..4) != Some(&[0x00, 0x08, 0x00, 0x00]) {
        return Err(RmpegError::InvalidData("missing AEA header".to_string()));
    }
    let duration_seconds = (bytes.len() - AEA_HEADER_SIZE) as f64 / AEA_OBSERVED_BYTES_PER_SECOND;
    Ok(ProbeDocument {
        format: "aea".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "atrac1",
            44_100,
            2,
            0,
            duration_seconds,
        )],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aea_duration_from_payload_size() {
        let mut bytes = vec![0; AEA_HEADER_SIZE + 36_500];
        bytes[0..4].copy_from_slice(&[0x00, 0x08, 0x00, 0x00]);
        let doc = parse_aea(&bytes).expect("valid aea");
        assert_eq!(doc.format, "aea");
        assert_eq!(doc.streams[0].codec_name, "atrac1");
        assert_eq!(doc.streams[0].duration_seconds, Some(1.0));
    }
}
