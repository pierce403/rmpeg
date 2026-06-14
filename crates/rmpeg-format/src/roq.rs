use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_roq(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_roq(bytes) {
        return Err(RmpegError::InvalidData("missing RoQ header".to_string()));
    }
    let (width, height) = roq_video_dimensions(bytes)?;
    Ok(ProbeDocument {
        format: "roq".to_string(),
        streams: vec![
            StreamMetadata::audio(0, "roq_dpcm", 22_050, 2, 0, 0.0),
            StreamMetadata::video(1, "roq", width, height, Some(0.0), None),
        ],
    })
}

pub fn looks_like_roq(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[0..6] == [0x84, 0x10, 0xff, 0xff, 0xff, 0xff]
}

fn roq_video_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let mut pos = 8;
    while pos + 12 <= bytes.len() {
        let chunk_id = read_u16_le(bytes, pos)?;
        let chunk_size = read_u32_le(bytes, pos + 2)? as usize;
        if chunk_id == 0x1001 || chunk_id == 0x1008 || chunk_id == 0x1011 {
            let width = u32::from(read_u16_le(bytes, pos + 8)?);
            let height = u32::from(read_u16_le(bytes, pos + 10)?);
            if width != 0 && height != 0 && width <= 4096 && height <= 4096 {
                return Ok((width, height));
            }
        }
        let next = pos
            .checked_add(8)
            .and_then(|value| value.checked_add(chunk_size))
            .ok_or_else(|| RmpegError::InvalidData("RoQ chunk size overflow".to_string()))?;
        if next <= pos || next > bytes.len() {
            break;
        }
        pos = next;
    }
    Err(RmpegError::InvalidData(
        "missing RoQ video dimensions".to_string(),
    ))
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
    fn parses_roq_dimensions_from_first_video_chunk() {
        let mut bytes = vec![0x84, 0x10, 0xff, 0xff, 0xff, 0xff, 30, 0];
        bytes.extend_from_slice(&0x1021_u16.to_le_bytes());
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&0x1001_u16.to_le_bytes());
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&512_u16.to_le_bytes());
        bytes.extend_from_slice(&256_u16.to_le_bytes());

        let doc = parse_roq(&bytes).expect("roq");

        assert_eq!(doc.format, "roq");
        assert_eq!(doc.streams[0].codec_name, "roq_dpcm");
        assert_eq!(doc.streams[1].width, Some(512));
        assert_eq!(doc.streams[1].height, Some(256));
    }
}
