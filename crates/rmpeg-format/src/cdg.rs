use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const CDG_PACKET_SIZE: usize = 24;

pub fn parse_cdg(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_cdg(bytes) {
        return Err(RmpegError::InvalidData(
            "missing CDG packet stream".to_string(),
        ));
    }
    let packets = bytes.len() / CDG_PACKET_SIZE;

    Ok(ProbeDocument {
        format: "cdg".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "cdgraphics",
            300,
            216,
            Some(packets as f64 / 300.0),
            Some("300/1".to_string()),
        )],
    })
}

pub fn looks_like_cdg(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len().is_multiple_of(CDG_PACKET_SIZE)
        && bytes
            .chunks_exact(CDG_PACKET_SIZE)
            .take(8)
            .all(|packet| packet[0] & 0x3f == 0x09)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cdg_duration_from_packet_count() {
        let mut bytes = Vec::new();
        for _ in 0..600 {
            bytes.extend_from_slice(&[0x09; CDG_PACKET_SIZE]);
        }

        let doc = parse_cdg(&bytes).expect("cdg");

        assert_eq!(doc.format, "cdg");
        assert_eq!(doc.streams[0].codec_name, "cdgraphics");
        assert_eq!(doc.streams[0].width, Some(300));
        assert_eq!(doc.streams[0].height, Some(216));
        assert_eq!(doc.streams[0].duration_seconds, Some(2.0));
        assert_eq!(doc.streams[0].frame_rate.as_deref(), Some("300/1"));
    }
}
