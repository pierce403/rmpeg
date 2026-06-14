use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_musepack(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.starts_with(b"MP+") {
        return parse_musepack7(bytes);
    }
    if bytes.starts_with(b"MPCK") {
        return parse_musepack8(bytes);
    }
    Err(RmpegError::InvalidData(
        "missing Musepack header".to_string(),
    ))
}

pub fn looks_like_musepack(bytes: &[u8]) -> bool {
    bytes.starts_with(b"MP+") || bytes.starts_with(b"MPCK")
}

fn parse_musepack7(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 8 || bytes[3] != 7 {
        return Err(RmpegError::InvalidData(
            "unsupported Musepack SV7 header".to_string(),
        ));
    }
    let frames = read_u32_le(bytes, 4)?;
    document("mpc", "musepack7", frames)
}

fn parse_musepack8(bytes: &[u8]) -> Result<ProbeDocument> {
    let Some(ap_pos) = find_bytes(bytes, b"AP") else {
        return Err(RmpegError::InvalidData(
            "missing Musepack SV8 AP packet".to_string(),
        ));
    };
    if ap_pos + 8 > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: ap_pos + 8,
            remaining: bytes.len(),
        });
    }
    let frames = u32::from(read_u16_be(bytes, ap_pos + 6)?);
    document("mpc8", "musepack8", frames)
}

fn document(format: &str, codec: &str, frames: u32) -> Result<ProbeDocument> {
    if frames == 0 {
        return Err(RmpegError::InvalidData(
            "invalid Musepack frame count".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec,
            44_100,
            2,
            0,
            frames as f64 * 1152.0 / 44_100.0,
        )],
    })
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sv7_frame_count_duration() {
        let mut bytes = b"MP+\x07".to_vec();
        bytes.extend_from_slice(&456_u32.to_le_bytes());

        let doc = parse_musepack(&bytes).expect("mpc7");

        assert_eq!(doc.format, "mpc");
        assert_eq!(doc.streams[0].codec_name, "musepack7");
        assert_eq!(
            doc.streams[0].duration_seconds,
            Some(456.0 * 1152.0 / 44_100.0)
        );
    }

    #[test]
    fn parses_observed_sv8_ap_packet_frame_count() {
        let mut bytes = b"MPCKSH\0\0EI\0\0SO\0\0AP".to_vec();
        bytes.extend_from_slice(&[0x81, 0xe7, 0x7d, 0x0e, 0x01, 0xc0, 0x63, 0x80]);

        let doc = parse_musepack(&bytes).expect("mpc8");

        assert_eq!(doc.format, "mpc8");
        assert_eq!(doc.streams[0].codec_name, "musepack8");
        assert_eq!(
            doc.streams[0].duration_seconds,
            Some(448.0 * 1152.0 / 44_100.0)
        );
    }
}
