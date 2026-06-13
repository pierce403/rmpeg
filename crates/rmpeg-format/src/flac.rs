use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_flac(bytes: &[u8]) -> Result<ProbeDocument> {
    if !bytes.starts_with(b"fLaC") {
        return Err(RmpegError::InvalidData("missing FLAC marker".to_string()));
    }

    let mut pos = 4;
    while pos + 4 <= bytes.len() {
        let header = bytes[pos];
        let is_last = header & 0x80 != 0;
        let block_type = header & 0x7f;
        let len = (usize::from(bytes[pos + 1]) << 16)
            | (usize::from(bytes[pos + 2]) << 8)
            | usize::from(bytes[pos + 3]);
        let start = pos + 4;
        let end = start
            .checked_add(len)
            .ok_or_else(|| RmpegError::InvalidData("FLAC metadata size overflow".to_string()))?;
        if end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: end,
                remaining: bytes.len(),
            });
        }
        if block_type == 0 {
            return parse_streaminfo(&bytes[start..end]);
        }
        if is_last {
            break;
        }
        pos = end;
    }

    Err(RmpegError::InvalidData(
        "missing FLAC STREAMINFO block".to_string(),
    ))
}

fn parse_streaminfo(data: &[u8]) -> Result<ProbeDocument> {
    if data.len() < 34 {
        return Err(RmpegError::UnexpectedEof {
            needed: 34,
            remaining: data.len(),
        });
    }
    let packed = u64::from_be_bytes([
        data[10], data[11], data[12], data[13], data[14], data[15], data[16], data[17],
    ]);
    let sample_rate = ((packed >> 44) & 0x000f_ffff) as u32;
    let channels = (((packed >> 41) & 0x07) + 1) as u16;
    let bits_per_sample = (((packed >> 36) & 0x1f) + 1) as u16;
    let total_samples = packed & 0x000f_ffff_ffff;
    if sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "FLAC STREAMINFO has zero sample rate".to_string(),
        ));
    }
    let duration_seconds = total_samples as f64 / sample_rate as f64;

    Ok(ProbeDocument {
        format: "flac".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "flac",
            sample_rate,
            channels,
            bits_per_sample,
            duration_seconds,
        )],
    })
}
