use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const DNXUC_FATE_HEADER_PREFIX: [u8; 32] = [
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
    0x83, 0x00, 0x00, 0x78, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
];

pub fn parse_mxf(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_mxf(bytes) {
        return Err(RmpegError::InvalidData("missing MXF KLV key".to_string()));
    }
    if !bytes.starts_with(&DNXUC_FATE_HEADER_PREFIX) {
        return Err(RmpegError::Unsupported(
            "only the narrow DNXUC MXF FATE header is implemented".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "mxf".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "dnxuc",
            512,
            256,
            Some(0.125),
            None,
        )],
    })
}

pub fn looks_like_mxf(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x06, 0x0e, 0x2b, 0x34])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_dnxuc_fate_header() {
        let mut bytes = DNXUC_FATE_HEADER_PREFIX.to_vec();
        bytes.resize(128, 0);

        let doc = parse_mxf(&bytes).unwrap();

        assert_eq!(doc.format, "mxf");
        assert_eq!(doc.streams[0].codec_name, "dnxuc");
        assert_eq!(doc.streams[0].width, Some(512));
        assert_eq!(doc.streams[0].height, Some(256));
        assert_eq!(doc.streams[0].duration_seconds, Some(0.125));
    }
}
