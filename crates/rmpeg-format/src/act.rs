use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_act(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_act(bytes) {
        return Err(RmpegError::InvalidData(
            "missing ACT RIFF/WAVE header".to_string(),
        ));
    }
    let data_size = find_data_chunk_size(bytes)?;

    Ok(ProbeDocument {
        format: "act".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "g729",
            8_000,
            1,
            0,
            data_size as f64 / 8_000.0,
        )],
    })
}

pub fn looks_like_act(bytes: &[u8]) -> bool {
    bytes.len() >= 44
        && &bytes[0..4] == b"RIFF"
        && &bytes[8..12] == b"WAVE"
        && &bytes[12..16] == b"fmt "
        && read_u16_le_lossy(bytes, 20) == Some(1)
        && read_u16_le_lossy(bytes, 22) == Some(1)
        && read_u32_le_lossy(bytes, 24) == Some(8_000)
}

fn find_data_chunk_size(bytes: &[u8]) -> Result<u32> {
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = read_u32_le(bytes, pos + 4)?;
        if id == b"data" {
            return Ok(size);
        }
        let next = pos
            .checked_add(8)
            .and_then(|value| value.checked_add(size as usize + (size as usize & 1)))
            .ok_or_else(|| RmpegError::InvalidData("ACT chunk size overflow".to_string()))?;
        if next <= pos {
            break;
        }
        pos = next;
    }
    Err(RmpegError::InvalidData(
        "missing ACT data chunk".to_string(),
    ))
}

fn read_u16_le_lossy(bytes: &[u8], pos: usize) -> Option<u16> {
    let end = pos.checked_add(2)?;
    if end > bytes.len() {
        return None;
    }
    Some(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_u32_le_lossy(bytes: &[u8], pos: usize) -> Option<u32> {
    let end = pos.checked_add(4)?;
    if end > bytes.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

fn read_u32_le(bytes: &[u8], pos: usize) -> Result<u32> {
    read_u32_le_lossy(bytes, pos).ok_or(RmpegError::UnexpectedEof {
        needed: pos + 4,
        remaining: bytes.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_act_duration_from_declared_data_size() {
        let mut bytes = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&8_000_u32.to_le_bytes());
        bytes.extend_from_slice(&16_000_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&526_320_u32.to_le_bytes());

        let doc = parse_act(&bytes).expect("act");

        assert_eq!(doc.format, "act");
        assert_eq!(doc.streams[0].codec_name, "g729");
        assert_eq!(doc.streams[0].duration_seconds, Some(65.79));
    }
}
