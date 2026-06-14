use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy)]
struct XwmaFmt {
    format_tag: u16,
    channels: u16,
    sample_rate: u32,
}

pub fn parse_xwma(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_xwma(bytes) {
        return Err(RmpegError::InvalidData(
            "missing XWMA RIFF header".to_string(),
        ));
    }

    let mut fmt = None;
    let mut decoded_samples = None;
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = read_u32_le(bytes, pos + 4)? as usize;
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(chunk_size)
            .ok_or_else(|| RmpegError::InvalidData("XWMA chunk size overflow".to_string()))?;
        if data_end > bytes.len() {
            break;
        }
        match chunk_id {
            b"fmt " => fmt = Some(parse_fmt(&bytes[data_start..data_end])?),
            b"dpds" => decoded_samples = parse_dpds(&bytes[data_start..data_end])?,
            _ => {}
        }
        pos = data_end + (chunk_size & 1);
    }

    let fmt = fmt.ok_or_else(|| RmpegError::InvalidData("missing XWMA fmt chunk".to_string()))?;
    if fmt.format_tag != 0x0161 || fmt.channels == 0 || fmt.sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "unsupported XWMA audio format".to_string(),
        ));
    }
    let decoded_samples = decoded_samples
        .ok_or_else(|| RmpegError::InvalidData("missing XWMA decoded-packet table".to_string()))?;

    Ok(ProbeDocument {
        format: "xwma".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "wmav2",
            fmt.sample_rate,
            fmt.channels,
            0,
            decoded_samples as f64 / (f64::from(fmt.channels) * 2.0 * fmt.sample_rate as f64),
        )],
    })
}

pub fn looks_like_xwma(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"XWMA"
}

fn parse_fmt(bytes: &[u8]) -> Result<XwmaFmt> {
    if bytes.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: bytes.len(),
        });
    }
    Ok(XwmaFmt {
        format_tag: read_u16_le(bytes, 0)?,
        channels: read_u16_le(bytes, 2)?,
        sample_rate: read_u32_le(bytes, 4)?,
    })
}

fn parse_dpds(bytes: &[u8]) -> Result<Option<u32>> {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(4) {
        return Ok(None);
    }
    read_u32_le(bytes, bytes.len() - 4).map(Some)
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
    fn parses_xwma_duration_from_decoded_packet_table() {
        let mut bytes = b"RIFF\0\0\0\0XWMAfmt ".to_vec();
        bytes.extend_from_slice(&18_u32.to_le_bytes());
        bytes.extend_from_slice(&0x0161_u16.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&48_000_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(b"dpds");
        bytes.extend_from_slice(&8_u32.to_le_bytes());
        bytes.extend_from_slice(&96_000_u32.to_le_bytes());
        bytes.extend_from_slice(&192_000_u32.to_le_bytes());

        let doc = parse_xwma(&bytes).expect("xwma");

        assert_eq!(doc.format, "xwma");
        assert_eq!(doc.streams[0].codec_name, "wmav2");
        assert_eq!(doc.streams[0].duration_seconds, Some(1.0));
    }
}
