use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_jv(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_jv(bytes) {
        return Err(RmpegError::InvalidData("missing JV header".to_string()));
    }

    let width = u32::from(read_u16_le(bytes, 0x50)?);
    let height = u32::from(read_u16_le(bytes, 0x52)?);
    let frames = u32::from(read_u16_le(bytes, 0x54)?);
    let sample_rate = u32::from(read_u16_le(bytes, 0x5c)?);
    if width == 0 || height == 0 || frames == 0 || sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "invalid JV stream metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "jv".to_string(),
        streams: vec![
            StreamMetadata::audio(0, "pcm_u8", sample_rate, 1, 8, 0.0),
            StreamMetadata::video(1, "jv", width, height, Some(frames as f64 / 12.5), None),
        ],
    })
}

pub fn looks_like_jv(bytes: &[u8]) -> bool {
    bytes.len() >= 0x5e && bytes.starts_with(b"JV00 Compression")
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bitmap_brothers_jv_header() {
        let mut bytes = vec![0; 0x5e];
        bytes[0..16].copy_from_slice(b"JV00 Compression");
        bytes[0x50..0x52].copy_from_slice(&320_u16.to_le_bytes());
        bytes[0x52..0x54].copy_from_slice(&200_u16.to_le_bytes());
        bytes[0x54..0x56].copy_from_slice(&2010_u16.to_le_bytes());
        bytes[0x5c..0x5e].copy_from_slice(&22_050_u16.to_le_bytes());

        let doc = parse_jv(&bytes).expect("jv");

        assert_eq!(doc.format, "jv");
        assert_eq!(doc.streams[0].codec_name, "pcm_u8");
        assert_eq!(doc.streams[1].codec_name, "jv");
        assert_eq!(doc.streams[1].duration_seconds, Some(160.8));
    }
}
