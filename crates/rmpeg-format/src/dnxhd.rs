use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_raw_dnxhd(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_raw_dnxhd(bytes) {
        return Err(RmpegError::InvalidData(
            "missing raw DNxHD frame header".to_string(),
        ));
    }

    let height = u32::from(read_u16_be(bytes, 24)?);
    let width = u32::from(read_u16_be(bytes, 26)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "DNxHD frame dimensions are zero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "dnxhd".to_string(),
        streams: vec![StreamMetadata::video(0, "dnxhd", width, height, None, None)],
    })
}

pub fn looks_like_raw_dnxhd(bytes: &[u8]) -> bool {
    bytes.len() >= 28 && bytes[4..8] == [0x03, 0x01, 0x80, 0xa0]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_dnxhr_frame_dimensions() {
        let mut bytes = vec![0; 128];
        bytes[4..8].copy_from_slice(&[0x03, 0x01, 0x80, 0xa0]);
        bytes[24..26].copy_from_slice(&2160_u16.to_be_bytes());
        bytes[26..28].copy_from_slice(&3840_u16.to_be_bytes());

        let doc = parse_raw_dnxhd(&bytes).unwrap();

        assert_eq!(doc.format, "dnxhd");
        assert_eq!(doc.streams[0].codec_name, "dnxhd");
        assert_eq!(doc.streams[0].width, Some(3840));
        assert_eq!(doc.streams[0].height, Some(2160));
    }
}
