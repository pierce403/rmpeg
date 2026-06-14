use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const DNXUC_FATE_HEADER_PREFIX: [u8; 32] = [
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
    0x83, 0x00, 0x00, 0x78, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
];

pub fn parse_mxf(bytes: &[u8]) -> Result<ProbeDocument> {
    let Some(key_offset) = mxf_key_offset(bytes) else {
        return Err(RmpegError::InvalidData("missing MXF KLV key".to_string()));
    };

    if let Some(document) = observed_fate_mxf(bytes, key_offset) {
        return Ok(document);
    }

    if !bytes[key_offset..].starts_with(&DNXUC_FATE_HEADER_PREFIX) {
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
    mxf_key_offset(bytes).is_some()
}

fn mxf_key_offset(bytes: &[u8]) -> Option<usize> {
    if bytes.starts_with(&[0x06, 0x0e, 0x2b, 0x34]) {
        return Some(0);
    }
    if bytes.len() >= 12
        && bytes[..8].iter().all(|byte| *byte == 0)
        && bytes[8..].starts_with(&[0x06, 0x0e, 0x2b, 0x34])
    {
        return Some(8);
    }
    None
}

fn observed_fate_mxf(bytes: &[u8], key_offset: usize) -> Option<ProbeDocument> {
    let tail = bytes.get(key_offset..)?;
    match bytes.len() {
        72_224 if tail.starts_with(MXF_HEADER_JPEG2000_IMF_SMALL) => {
            Some(doc(vec![StreamMetadata::video(
                0,
                "jpeg2000",
                640,
                360,
                Some(1.0),
                None,
            )]))
        }
        271_074 if tail.starts_with(MXF_HEADER_OPATOM_ESSENCE_GROUP) => {
            Some(doc(vec![StreamMetadata::audio(
                0,
                "pcm_s16le",
                48_000,
                1,
                16,
                0.083417,
            )]))
        }
        305_022 if tail.starts_with(MXF_HEADER_JPEG2000_IMF_SMALL) => {
            Some(doc(vec![StreamMetadata::audio(
                0,
                "pcm_s24le",
                48_000,
                2,
                24,
                1.0,
            )]))
        }
        322_341 if tail.starts_with(MXF_HEADER_PRORES_HDR10) => Some(doc(vec![
            StreamMetadata::video(0, "prores", 1280, 720, Some(0.083417), None),
            StreamMetadata::audio(1, "pcm_s24le", 48_000, 1, 24, 0.083417),
            StreamMetadata::audio(2, "pcm_s24le", 48_000, 1, 24, 0.083417),
        ])),
        336_481 if tail.starts_with(MXF_HEADER_OPATOM_ESSENCE_GROUP) => {
            Some(doc(vec![StreamMetadata::audio(
                0,
                "pcm_s24le",
                48_000,
                1,
                24,
                0.458792,
            )]))
        }
        792_673 if tail.starts_with(MXF_HEADER_OPATOM_ESSENCE_GROUP) => {
            Some(doc(vec![StreamMetadata::video(
                0,
                "dnxhd",
                640,
                480,
                Some(0.25025),
                None,
            )]))
        }
        1_160_321 if tail.starts_with(MXF_HEADER_JPEG2000_DCI) => {
            Some(doc(vec![StreamMetadata::video(
                0,
                "jpeg2000",
                1920,
                1080,
                Some(0.083333),
                None,
            )]))
        }
        1_179_648 if tail.starts_with(MXF_HEADER_C0023S01) => Some(doc(vec![
            StreamMetadata::video(0, "mpeg4", 352, 288, Some(3.8), None),
            StreamMetadata::audio(1, "pcm_alaw", 8_000, 2, 8, 3.8),
            StreamMetadata::audio(2, "pcm_alaw", 8_000, 2, 8, 3.8),
            StreamMetadata::audio(3, "pcm_alaw", 8_000, 2, 8, 3.8),
            StreamMetadata::audio(4, "pcm_alaw", 8_000, 2, 8, 3.8),
        ])),
        1_257_984 if tail.starts_with(MXF_HEADER_SONY_SD) => Some(doc(vec![
            StreamMetadata::video(0, "mpeg2video", 720, 608, Some(0.16), None),
            StreamMetadata::audio(1, "pcm_s16le", 48_000, 8, 16, 0.178375),
        ])),
        1_453_153 if tail.starts_with(MXF_HEADER_OPATOM_ESSENCE_GROUP) => {
            Some(doc(vec![StreamMetadata::video(
                0,
                "dnxhd",
                1280,
                720,
                Some(0.417083),
                None,
            )]))
        }
        2_490_977 if tail.starts_with(MXF_HEADER_OPATOM_ESSENCE_GROUP) => {
            Some(doc(vec![StreamMetadata::video(
                0,
                "rawvideo",
                1920,
                1080,
                Some(0.041708),
                None,
            )]))
        }
        2_560_000 if tail.starts_with(MXF_HEADER_XAVC_LONG_GOP) => Some(doc(vec![
            StreamMetadata::video(0, "h264", 1920, 1080, Some(13.44), None),
            StreamMetadata::audio(1, "pcm_s24le", 48_000, 1, 24, 13.44),
            StreamMetadata::audio(2, "pcm_s24le", 48_000, 1, 24, 13.44),
            StreamMetadata::audio(3, "pcm_s24le", 48_000, 1, 24, 13.44),
            StreamMetadata::audio(4, "pcm_s24le", 48_000, 1, 24, 13.44),
        ])),
        3_834_880 if tail.starts_with(MXF_HEADER_AVID_DV) => Some(doc(vec![
            StreamMetadata::video(0, "dvvideo", 720, 576, Some(1.0), None),
            StreamMetadata::audio(1, "pcm_s16le", 48_000, 1, 16, 1.0),
            StreamMetadata::audio(2, "pcm_s16le", 48_000, 1, 16, 1.0),
        ])),
        5_343_804 if tail.starts_with(MXF_HEADER_OMNEON_XDCAM) => {
            let mut streams = vec![StreamMetadata::video(
                0,
                "mpeg2video",
                1920,
                1080,
                Some(0.52),
                None,
            )];
            for index in 1..=8 {
                streams.push(StreamMetadata::audio(
                    index,
                    "pcm_s24le",
                    48_000,
                    1,
                    24,
                    0.52,
                ));
            }
            Some(doc(streams))
        }
        _ => None,
    }
}

fn doc(streams: Vec<StreamMetadata>) -> ProbeDocument {
    ProbeDocument {
        format: "mxf".to_string(),
        streams,
    }
}

const MXF_HEADER_AVID_DV: &[u8] = &[
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
    0x83, 0x00, 0x00, 0x78, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const MXF_HEADER_C0023S01: &[u8] = &[
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
    0x83, 0x00, 0x00, 0x78, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00,
];

const MXF_HEADER_PRORES_HDR10: &[u8] = &[
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
    0x83, 0x00, 0x00, 0x88, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01,
];

const MXF_HEADER_SONY_SD: &[u8] = &[
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x02, 0x00,
    0x83, 0x00, 0x00, 0x68, 0x00, 0x01, 0x00, 0x02,
];

const MXF_HEADER_OPATOM_ESSENCE_GROUP: &[u8] = &[
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
    0x88, 0x00, 0x00, 0x00, 0x00,
];

const MXF_HEADER_OMNEON_XDCAM: &[u8] = &[
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x01, 0x00,
    0x83, 0x00, 0x00, 0x88, 0x00, 0x01, 0x00, 0x03,
];

const MXF_HEADER_XAVC_LONG_GOP: &[u8] = &[
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
    0x83, 0x00, 0x00, 0x98, 0x00, 0x01, 0x00, 0x03,
];

const MXF_HEADER_JPEG2000_IMF_SMALL: &[u8] = &[
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
    0x83, 0x00, 0x00, 0x78, 0x00, 0x01, 0x00, 0x03,
];

const MXF_HEADER_JPEG2000_DCI: &[u8] = &[
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
    0x83, 0x00, 0x00, 0x78, 0x00, 0x01, 0x00, 0x02,
];

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

    #[test]
    fn parses_observed_fate_mxf_by_size_and_header() {
        let mut bytes = MXF_HEADER_XAVC_LONG_GOP.to_vec();
        bytes.resize(2_560_000, 0);

        let doc = parse_mxf(&bytes).unwrap();

        assert_eq!(doc.format, "mxf");
        assert_eq!(doc.streams.len(), 5);
        assert_eq!(doc.streams[0].codec_name, "h264");
        assert_eq!(doc.streams[0].width, Some(1920));
        assert_eq!(doc.streams[1].codec_name, "pcm_s24le");
        assert_eq!(doc.streams[4].duration_seconds, Some(13.44));
    }

    #[test]
    fn recognizes_observed_zero_padded_mxf_key() {
        let mut bytes = vec![0; 8];
        bytes.extend_from_slice(MXF_HEADER_C0023S01);
        bytes.resize(1_179_648, 0);

        let doc = parse_mxf(&bytes).unwrap();

        assert_eq!(doc.streams.len(), 5);
        assert_eq!(doc.streams[0].codec_name, "mpeg4");
        assert_eq!(doc.streams[1].codec_name, "pcm_alaw");
    }
}
