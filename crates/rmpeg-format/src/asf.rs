use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const ASF_HEADER_GUID: [u8; 16] = [
    0x30, 0x26, 0xb2, 0x75, 0x8e, 0x66, 0xcf, 0x11, 0xa6, 0xd9, 0x00, 0xaa, 0x00, 0x62, 0xce, 0x6c,
];
const FILE_PROPERTIES_GUID: [u8; 16] = [
    0xa1, 0xdc, 0xab, 0x8c, 0x47, 0xa9, 0xcf, 0x11, 0x8e, 0xe4, 0x00, 0xc0, 0x0c, 0x20, 0x53, 0x65,
];
const STREAM_PROPERTIES_GUID: [u8; 16] = [
    0x91, 0x07, 0xdc, 0xb7, 0xb7, 0xa9, 0xcf, 0x11, 0x8e, 0xe6, 0x00, 0xc0, 0x0c, 0x20, 0x53, 0x65,
];
const AUDIO_MEDIA_GUID: [u8; 16] = [
    0x40, 0x9e, 0x69, 0xf8, 0x4d, 0x5b, 0xcf, 0x11, 0xa8, 0xfd, 0x00, 0x80, 0x5f, 0x5c, 0x44, 0x2b,
];

#[derive(Debug, Default)]
struct AsfMetadata {
    declared_file_size: Option<u64>,
    play_duration_100ns: Option<u64>,
    preroll_ms: Option<u64>,
    header_size: usize,
    codec_name: Option<&'static str>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    bits_per_sample: Option<u16>,
    average_bytes_per_second: Option<u32>,
}

pub fn parse_asf(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_asf(bytes) {
        return Err(RmpegError::InvalidData("missing ASF header".to_string()));
    }
    if bytes.len() < 30 {
        return Err(RmpegError::UnexpectedEof {
            needed: 30,
            remaining: bytes.len(),
        });
    }

    let header_size = usize::try_from(read_u64_le(bytes, 16)?)
        .map_err(|_| RmpegError::InvalidData("ASF header is too large".to_string()))?;
    if header_size > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: header_size,
            remaining: bytes.len(),
        });
    }
    let object_count = read_u32_le(bytes, 24)?;
    let mut metadata = AsfMetadata {
        header_size,
        ..AsfMetadata::default()
    };

    let mut pos = 30;
    for _ in 0..object_count {
        if pos + 24 > header_size {
            break;
        }
        let guid = &bytes[pos..pos + 16];
        let object_size = usize::try_from(read_u64_le(bytes, pos + 16)?)
            .map_err(|_| RmpegError::InvalidData("ASF object is too large".to_string()))?;
        if object_size < 24 || pos + object_size > header_size {
            return Err(RmpegError::InvalidData(
                "invalid ASF object size".to_string(),
            ));
        }
        let data = &bytes[pos + 24..pos + object_size];
        if guid == FILE_PROPERTIES_GUID {
            parse_file_properties(data, &mut metadata)?;
        } else if guid == STREAM_PROPERTIES_GUID {
            parse_stream_properties(data, &mut metadata)?;
        }
        pos += object_size;
    }

    let duration_seconds = asf_duration_seconds(bytes, &metadata);
    let bits_per_sample = if metadata
        .declared_file_size
        .is_some_and(|declared| (bytes.len() as u64) < declared)
    {
        0
    } else {
        metadata.bits_per_sample.unwrap_or(0)
    };
    Ok(ProbeDocument {
        format: "asf".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            metadata.codec_name.ok_or_else(|| {
                RmpegError::InvalidData("ASF file has no supported audio stream".to_string())
            })?,
            metadata.sample_rate.ok_or_else(|| {
                RmpegError::InvalidData("ASF audio stream has no sample rate".to_string())
            })?,
            metadata.channels.ok_or_else(|| {
                RmpegError::InvalidData("ASF audio stream has no channel count".to_string())
            })?,
            bits_per_sample,
            duration_seconds,
        )],
    })
}

pub fn looks_like_asf(bytes: &[u8]) -> bool {
    bytes.starts_with(&ASF_HEADER_GUID)
}

fn parse_file_properties(data: &[u8], metadata: &mut AsfMetadata) -> Result<()> {
    if data.len() < 64 {
        return Err(RmpegError::UnexpectedEof {
            needed: 64,
            remaining: data.len(),
        });
    }
    metadata.declared_file_size = Some(read_u64_le(data, 16)?);
    metadata.play_duration_100ns = Some(read_u64_le(data, 40)?);
    metadata.preroll_ms = Some(read_u64_le(data, 56)?);
    Ok(())
}

fn parse_stream_properties(data: &[u8], metadata: &mut AsfMetadata) -> Result<()> {
    if data.len() < 54 {
        return Err(RmpegError::UnexpectedEof {
            needed: 54,
            remaining: data.len(),
        });
    }
    if data[0..16] != AUDIO_MEDIA_GUID {
        return Ok(());
    }
    let type_len = usize::try_from(read_u32_le(data, 40)?)
        .map_err(|_| RmpegError::InvalidData("ASF stream data is too large".to_string()))?;
    let wave_start = 54;
    let wave_end = wave_start + type_len;
    if wave_end > data.len() || type_len < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: wave_end,
            remaining: data.len(),
        });
    }
    let wave = &data[wave_start..wave_end];
    let format_tag = read_u16_le(wave, 0)?;
    metadata.codec_name = match format_tag {
        0x0163 => Some("wmalossless"),
        _ => None,
    };
    metadata.channels = Some(read_u16_le(wave, 2)?);
    metadata.sample_rate = Some(read_u32_le(wave, 4)?);
    metadata.average_bytes_per_second = Some(read_u32_le(wave, 8)?);
    metadata.bits_per_sample = Some(read_u16_le(wave, 14)?);
    Ok(())
}

fn asf_duration_seconds(bytes: &[u8], metadata: &AsfMetadata) -> f64 {
    let header_duration = match (metadata.play_duration_100ns, metadata.preroll_ms) {
        (Some(play), Some(preroll)) => (play as f64 / 10_000_000.0) - (preroll as f64 / 1000.0),
        _ => 0.0,
    };

    let Some(declared_file_size) = metadata.declared_file_size else {
        return header_duration.max(0.0);
    };
    let Some(avg_bytes_per_second) = metadata
        .average_bytes_per_second
        .filter(|value| *value != 0)
    else {
        return header_duration.max(0.0);
    };
    if bytes.len() as u64 >= declared_file_size {
        return header_duration.max(0.0);
    }

    let payload_bytes = bytes.len().saturating_sub(metadata.header_size + 50);
    let truncated_duration = payload_bytes as f64 / avg_bytes_per_second as f64;
    header_duration.max(0.0).min(truncated_duration)
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
