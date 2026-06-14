use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_smacker(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_smacker(bytes) {
        return Err(RmpegError::InvalidData(
            "missing Smacker header".to_string(),
        ));
    }
    let width = read_u32_le(bytes, 4)?;
    let height = read_u32_le(bytes, 8)?;
    let frames = read_u32_le(bytes, 12)?;
    let frame_delay_ms = read_u32_le(bytes, 16)?;
    if width == 0 || height == 0 || frames == 0 || frame_delay_ms == 0 {
        return Err(RmpegError::InvalidData(
            "invalid Smacker metadata".to_string(),
        ));
    }
    let duration = frames as f64 * frame_delay_ms as f64 / 1000.0;

    let mut streams = vec![StreamMetadata::video(
        0,
        "smackvideo",
        width,
        height,
        Some(duration),
        None,
    )];
    if bytes.len() >= 0x4a {
        let sample_rate = u32::from(read_u16_le(bytes, 0x48)?);
        if sample_rate != 0 {
            streams.push(StreamMetadata::audio(
                streams.len(),
                "smackaudio",
                sample_rate,
                1,
                0,
                0.0,
            ));
        }
    }

    Ok(ProbeDocument {
        format: "smk".to_string(),
        streams,
    })
}

pub fn looks_like_smacker(bytes: &[u8]) -> bool {
    bytes.len() >= 20 && (bytes.starts_with(b"SMK2") || bytes.starts_with(b"SMK4"))
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
    fn parses_observed_smacker_header() {
        let mut bytes = vec![0; 0x4a];
        bytes[0..4].copy_from_slice(b"SMK2");
        bytes[4..8].copy_from_slice(&320_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&200_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&100_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&71_u32.to_le_bytes());
        bytes[0x48..0x4a].copy_from_slice(&22_050_u16.to_le_bytes());

        let doc = parse_smacker(&bytes).expect("smacker");

        assert_eq!(doc.format, "smk");
        assert_eq!(doc.streams[0].duration_seconds, Some(7.1));
        assert_eq!(doc.streams[1].codec_name, "smackaudio");
    }
}
