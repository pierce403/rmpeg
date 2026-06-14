use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_ape(bytes: &[u8]) -> Result<ProbeDocument> {
    if !bytes.starts_with(b"MAC ") {
        return Err(RmpegError::InvalidData(
            "missing APE magic header".to_string(),
        ));
    }
    if bytes.len() < 32 {
        return Err(RmpegError::UnexpectedEof {
            needed: 32,
            remaining: bytes.len(),
        });
    }

    let version = read_u16_le(bytes, 4)?;
    let metadata = if version >= 3_980 {
        parse_new_header(bytes)?
    } else {
        parse_old_header(bytes, version)?
    };

    Ok(ProbeDocument {
        format: "ape".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "ape",
            metadata.sample_rate,
            metadata.channels,
            metadata.bits_per_sample,
            metadata.duration_seconds,
        )],
    })
}

struct ApeMetadata {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    duration_seconds: f64,
}

fn parse_new_header(bytes: &[u8]) -> Result<ApeMetadata> {
    if bytes.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: bytes.len(),
        });
    }
    let descriptor_bytes = usize::try_from(read_u32_le(bytes, 8)?)
        .map_err(|_| RmpegError::InvalidData("APE descriptor is too large".to_string()))?;
    let header_bytes = usize::try_from(read_u32_le(bytes, 12)?)
        .map_err(|_| RmpegError::InvalidData("APE header is too large".to_string()))?;
    if header_bytes < 24 {
        return Err(RmpegError::InvalidData(
            "APE header is shorter than expected".to_string(),
        ));
    }
    let header_end = descriptor_bytes
        .checked_add(24)
        .ok_or_else(|| RmpegError::InvalidData("APE header offset overflow".to_string()))?;
    if header_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: header_end,
            remaining: bytes.len(),
        });
    }

    let blocks_per_frame = read_u32_le(bytes, descriptor_bytes + 4)?;
    let final_frame_blocks = read_u32_le(bytes, descriptor_bytes + 8)?;
    let total_frames = read_u32_le(bytes, descriptor_bytes + 12)?;
    let bits_per_sample = read_u16_le(bytes, descriptor_bytes + 16)?;
    let channels = read_u16_le(bytes, descriptor_bytes + 18)?;
    let sample_rate = read_u32_le(bytes, descriptor_bytes + 20)?;
    metadata(
        sample_rate,
        channels,
        bits_per_sample,
        blocks_per_frame,
        final_frame_blocks,
        total_frames,
    )
}

fn parse_old_header(bytes: &[u8], version: u16) -> Result<ApeMetadata> {
    if bytes.len() < 32 {
        return Err(RmpegError::UnexpectedEof {
            needed: 32,
            remaining: bytes.len(),
        });
    }
    let compression_level = read_u16_le(bytes, 6)?;
    let format_flags = read_u16_le(bytes, 8)?;
    let channels = read_u16_le(bytes, 10)?;
    let sample_rate = read_u32_le(bytes, 12)?;
    let total_frames = read_u32_le(bytes, 24)?;
    let final_frame_blocks = read_u32_le(bytes, 28)?;
    let bits_per_sample = old_bits_per_sample(format_flags);
    let blocks_per_frame = old_blocks_per_frame(version, compression_level);
    metadata(
        sample_rate,
        channels,
        bits_per_sample,
        blocks_per_frame,
        final_frame_blocks,
        total_frames,
    )
}

fn metadata(
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    blocks_per_frame: u32,
    final_frame_blocks: u32,
    total_frames: u32,
) -> Result<ApeMetadata> {
    if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
        return Err(RmpegError::InvalidData(
            "APE audio metadata has zero fields".to_string(),
        ));
    }
    if blocks_per_frame == 0 || total_frames == 0 {
        return Err(RmpegError::InvalidData(
            "APE frame metadata has zero fields".to_string(),
        ));
    }
    let last_frame_blocks = if final_frame_blocks == 0 {
        blocks_per_frame
    } else {
        final_frame_blocks
    };
    let full_frames = u64::from(total_frames.saturating_sub(1));
    let total_blocks = full_frames
        .checked_mul(u64::from(blocks_per_frame))
        .and_then(|value| value.checked_add(u64::from(last_frame_blocks)))
        .ok_or_else(|| RmpegError::InvalidData("APE sample count overflow".to_string()))?;

    Ok(ApeMetadata {
        sample_rate,
        channels,
        bits_per_sample,
        duration_seconds: total_blocks as f64 / f64::from(sample_rate),
    })
}

fn old_bits_per_sample(format_flags: u16) -> u16 {
    const APE_FORMAT_FLAG_8_BIT: u16 = 1;
    const APE_FORMAT_FLAG_24_BIT: u16 = 8;
    if format_flags & APE_FORMAT_FLAG_8_BIT != 0 {
        8
    } else if format_flags & APE_FORMAT_FLAG_24_BIT != 0 {
        24
    } else {
        16
    }
}

fn old_blocks_per_frame(version: u16, compression_level: u16) -> u32 {
    if version >= 3_900 || compression_level >= 4_000 {
        73_728
    } else {
        9_216
    }
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
    fn parses_old_ape_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MAC ");
        bytes.extend_from_slice(&3_800_u16.to_le_bytes());
        bytes.extend_from_slice(&2_000_u16.to_le_bytes());
        bytes.extend_from_slice(&6_u16.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&44_100_u32.to_le_bytes());
        bytes.extend_from_slice(&44_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&290_u32.to_le_bytes());
        bytes.extend_from_slice(&3_744_u32.to_le_bytes());

        let doc = parse_ape(&bytes).expect("valid old APE header");
        assert_eq!(doc.streams[0].sample_rate, Some(44_100));
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].bits_per_sample, Some(16));
        assert_eq!(doc.streams[0].duration_seconds, Some(60.48));
    }

    #[test]
    fn parses_new_ape_header() {
        let mut bytes = vec![0_u8; 52];
        bytes[0..4].copy_from_slice(b"MAC ");
        bytes[4..6].copy_from_slice(&3_990_u16.to_le_bytes());
        bytes[8..12].copy_from_slice(&52_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&24_u32.to_le_bytes());
        bytes.extend_from_slice(&3_000_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&73_728_u32.to_le_bytes());
        bytes.extend_from_slice(&62_805_u32.to_le_bytes());
        bytes.extend_from_slice(&76_u32.to_le_bytes());
        bytes.extend_from_slice(&24_u16.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&44_100_u32.to_le_bytes());

        let doc = parse_ape(&bytes).expect("valid new APE header");
        assert_eq!(doc.streams[0].sample_rate, Some(44_100));
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].bits_per_sample, Some(24));
        assert_eq!(doc.streams[0].duration_seconds, Some(126.81190476190476));
    }
}
