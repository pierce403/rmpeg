use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_brender_pix(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_brender_pix(bytes) {
        return Err(RmpegError::InvalidData(
            "missing BRender PIX header".to_string(),
        ));
    }
    let (width, height) = read_dimensions(bytes)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "BRender PIX dimensions are zero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "brender_pix".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "brender_pix",
            width,
            height,
            None,
            None,
        )],
    })
}

pub fn looks_like_brender_pix(bytes: &[u8]) -> bool {
    bytes.len() >= 32
        && bytes[0..4] == [0, 0, 0, 0x12]
        && bytes[4..8] == [0, 0, 0, 8]
        && bytes[8..12] == [0, 0, 0, 2]
        && bytes[12..16] == [0, 0, 0, 2]
}

fn read_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let width = u32::from(read_u16_le(bytes, 28)?);
    let height = u32::from(read_u16_le(bytes, 30)?);
    if width != 0 && height != 0 {
        return Ok((width, height));
    }

    Ok((
        u32::from(read_u16_le(bytes, 28)?),
        u32::from(read_u16_le(bytes, 26)?),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_brender_pix_header() {
        let mut bytes = vec![0; 64];
        bytes[3] = 0x12;
        bytes[7] = 8;
        bytes[11] = 2;
        bytes[15] = 2;
        bytes[28..30].copy_from_slice(&128_u16.to_le_bytes());
        bytes[30..32].copy_from_slice(&96_u16.to_le_bytes());

        let doc = parse_brender_pix(&bytes).expect("brender pix");

        assert_eq!(doc.format, "brender_pix");
        assert_eq!(doc.streams[0].codec_name, "brender_pix");
        assert_eq!(doc.streams[0].width, Some(128));
        assert_eq!(doc.streams[0].height, Some(96));
    }

    #[test]
    fn parses_observed_square_brender_pix_header() {
        let mut bytes = vec![0; 64];
        bytes[3] = 0x12;
        bytes[7] = 8;
        bytes[11] = 2;
        bytes[15] = 2;
        bytes[26..28].copy_from_slice(&256_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&256_u16.to_le_bytes());

        let doc = parse_brender_pix(&bytes).expect("brender pix");

        assert_eq!(doc.streams[0].width, Some(256));
        assert_eq!(doc.streams[0].height, Some(256));
    }
}
