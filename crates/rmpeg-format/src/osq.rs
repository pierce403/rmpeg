use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_osq(bytes: &[u8]) -> Result<ProbeDocument> {
    if !bytes.starts_with(b"OSQ ") {
        return Err(RmpegError::InvalidData("missing OSQ marker".to_string()));
    }
    if bytes.len() < 32 {
        return Err(RmpegError::UnexpectedEof {
            needed: 32,
            remaining: bytes.len(),
        });
    }

    let bits_and_channels = read_u16_le(bytes, 10)?;
    let bits_per_sample = bits_and_channels & 0x00ff;
    let channels = (bits_and_channels >> 8) as u16;
    let sample_rate = read_u32_le(bytes, 12)?;
    let total_samples = read_u64_le(bytes, 24)?;
    if channels == 0 || sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "OSQ stream has invalid audio metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "osq".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "osq",
            sample_rate,
            channels,
            bits_per_sample,
            total_samples as f64 / sample_rate as f64,
        )],
    })
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

fn read_u64_le(bytes: &[u8], offset: usize) -> Result<u64> {
    let end = offset + 8;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ]))
}
