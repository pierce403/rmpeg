use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_txd(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 136 {
        return Err(RmpegError::UnexpectedEof {
            needed: 136,
            remaining: bytes.len(),
        });
    }
    if read_u32_le(bytes, 0)? != 0x16 {
        return Err(RmpegError::InvalidData(
            "missing TXD texture dictionary chunk".to_string(),
        ));
    }
    let width = u32::from(read_u16_le(bytes, 132)?);
    let height = u32::from(read_u16_le(bytes, 134)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "TXD dimensions must be nonzero".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "txd".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "txd",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

fn read_u16_le(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_u32_le(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_txd_dimension_offsets() {
        let mut bytes = vec![0; 136];
        bytes[0..4].copy_from_slice(&0x16_u32.to_le_bytes());
        bytes[132..134].copy_from_slice(&387_u16.to_le_bytes());
        bytes[134..136].copy_from_slice(&249_u16.to_le_bytes());

        let doc = parse_txd(&bytes).expect("valid txd");
        assert_eq!(doc.format, "txd");
        assert_eq!(doc.streams[0].width, Some(387));
        assert_eq!(doc.streams[0].height, Some(249));
    }
}
