use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn looks_like_ea(bytes: &[u8]) -> bool {
    bytes.starts_with(b"MVhd") || bytes.starts_with(b"AVP6")
}

pub fn parse_ea(bytes: &[u8]) -> Result<ProbeDocument> {
    let mut streams = Vec::new();
    let mut pos = 0;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = read_u32(bytes, pos + 4)? as usize;
        if size < 8 || pos + size > bytes.len() {
            break;
        }
        let data_start = pos + 8;
        let data_end = pos + size;
        match id {
            b"MVhd" | b"AVhd" => {
                if let Some(stream) =
                    parse_video_header(&bytes[data_start..data_end], streams.len())?
                {
                    streams.push(stream);
                }
            }
            b"SCHl" => streams.push(StreamMetadata {
                index: streams.len(),
                codec_type: "audio".to_string(),
                codec_name: "adpcm_ea_r3".to_string(),
                sample_rate: Some(32_000),
                channels: Some(2),
                bits_per_sample: Some(0),
                duration_seconds: Some(0.0),
                width: None,
                height: None,
                frame_rate: None,
            }),
            _ => {}
        }
        pos += size + (size & 1);
    }

    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "EA file has no supported streams".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "ea".to_string(),
        streams,
    })
}

fn parse_video_header(data: &[u8], index: usize) -> Result<Option<StreamMetadata>> {
    if data.len() < 20 {
        return Ok(None);
    }
    let codec = &data[0..4];
    if codec != b"vp60" && codec != b"vp61" && codec != b"\0\0\0\0" {
        return Ok(None);
    }
    let width = round_up_16(u32::from(read_u16(data, 4)?));
    let height = round_up_16(u32::from(read_u16(data, 6)?));
    let frames = read_u32(data, 8)?;
    let fps_fixed = read_u32(data, 16)?;
    let duration = if fps_fixed == 0 {
        0.0
    } else {
        frames as f64 * 32768.0 / fps_fixed as f64
    };
    Ok(Some(StreamMetadata::video(
        index,
        "vp6",
        width,
        height,
        Some(duration),
        None,
    )))
}

fn round_up_16(value: u32) -> u32 {
    value.saturating_add(15) / 16 * 16
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
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

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}
