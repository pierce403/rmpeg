use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_tak(bytes: &[u8]) -> Result<ProbeDocument> {
    if !bytes.starts_with(b"tBaK") {
        return Err(RmpegError::InvalidData("missing TAK marker".to_string()));
    }

    let riff_start = find_bytes(bytes, b"RIFF").ok_or_else(|| {
        RmpegError::InvalidData("TAK file has no embedded WAVE metadata".to_string())
    })?;
    let wave = &bytes[riff_start..];
    if wave.len() < 12 || &wave[8..12] != b"WAVE" {
        return Err(RmpegError::InvalidData(
            "TAK embedded RIFF is not WAVE".to_string(),
        ));
    }

    let mut pos = 12;
    let mut fmt = None;
    let mut data_size = None;
    while pos + 8 <= wave.len() {
        let id = &wave[pos..pos + 4];
        let size = read_u32_le(wave, pos + 4)? as usize;
        let chunk_start = pos + 8;
        let chunk_end = chunk_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("TAK WAVE chunk overflow".to_string()))?;
        match id {
            b"fmt " if chunk_start + 16 <= wave.len() => {
                fmt = Some(WaveFmt {
                    channels: read_u16_le(wave, chunk_start + 2)?,
                    sample_rate: read_u32_le(wave, chunk_start + 4)?,
                    block_align: read_u16_le(wave, chunk_start + 12)?,
                    bits_per_sample: read_u16_le(wave, chunk_start + 14)?,
                });
            }
            b"data" => data_size = Some(size),
            _ => {}
        }
        if fmt.is_some() && data_size.is_some() {
            break;
        }
        pos = chunk_end.saturating_add(size % 2);
        if pos > wave.len() {
            break;
        }
    }

    let fmt = fmt
        .ok_or_else(|| RmpegError::InvalidData("TAK WAVE metadata has no fmt chunk".to_string()))?;
    let data_size = data_size.ok_or_else(|| {
        RmpegError::InvalidData("TAK WAVE metadata has no data chunk".to_string())
    })?;
    if fmt.channels == 0 || fmt.sample_rate == 0 || fmt.block_align == 0 {
        return Err(RmpegError::InvalidData(
            "TAK WAVE metadata is invalid".to_string(),
        ));
    }
    let duration = data_size as f64 / (fmt.sample_rate as f64 * fmt.block_align as f64);

    Ok(ProbeDocument {
        format: "tak".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "tak",
            fmt.sample_rate,
            fmt.channels,
            fmt.bits_per_sample,
            duration,
        )],
    })
}

#[derive(Debug, Clone, Copy)]
struct WaveFmt {
    channels: u16,
    sample_rate: u32,
    block_align: u16,
    bits_per_sample: u16,
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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
