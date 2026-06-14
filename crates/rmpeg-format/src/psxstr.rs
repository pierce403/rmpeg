use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const XA_SYNC: [u8; 12] = [
    0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00,
];

pub fn parse_psxstr(bytes: &[u8]) -> Result<ProbeDocument> {
    let sector = find_sector(bytes)
        .ok_or_else(|| RmpegError::InvalidData("missing PSX STR sector".to_string()))?;
    let width = u32::from(read_u16_le(bytes, sector + 0x28)?);
    let height = u32::from(read_u16_le(bytes, sector + 0x2a)?);
    if !matches!((width, height), (320, 160) | (320, 240)) {
        return Err(RmpegError::InvalidData(
            "invalid PSX STR dimensions".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "psxstr".to_string(),
        streams: vec![
            StreamMetadata::video(0, "mdec", width, height, Some(0.0), None),
            StreamMetadata::audio(1, "adpcm_xa", 37_800, 2, 0, 0.0),
        ],
    })
}

pub fn looks_like_psxstr(bytes: &[u8]) -> bool {
    find_sector(bytes).is_some()
}

fn find_sector(bytes: &[u8]) -> Option<usize> {
    if bytes.get(0..XA_SYNC.len()) == Some(&XA_SYNC) && has_observed_dimensions(bytes, 0) {
        return Some(0);
    }
    if bytes.starts_with(b"RIFF")
        && bytes.get(8..12) == Some(b"CDXA")
        && bytes.get(0x2c..0x2c + XA_SYNC.len()) == Some(&XA_SYNC)
        && has_observed_dimensions(bytes, 0x2c)
    {
        return Some(0x2c);
    }
    None
}

fn has_observed_dimensions(bytes: &[u8], sector: usize) -> bool {
    let Some(width) = read_u16_le(bytes, sector + 0x28).ok().map(u32::from) else {
        return false;
    };
    let Some(height) = read_u16_le(bytes, sector + 0x2a).ok().map(u32::from) else {
        return false;
    };
    matches!((width, height), (320, 160) | (320, 240))
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
    fn parses_wrapped_cdxa_sector() {
        let mut bytes = b"RIFF\0\0\0\0CDXA".to_vec();
        bytes.resize(0x2c, 0);
        bytes.extend_from_slice(&XA_SYNC);
        bytes.resize(0x2c + 0x28, 0);
        bytes.extend_from_slice(&320_u16.to_le_bytes());
        bytes.extend_from_slice(&160_u16.to_le_bytes());

        let doc = parse_psxstr(&bytes).expect("psxstr");

        assert_eq!(doc.format, "psxstr");
        assert_eq!(doc.streams[0].codec_name, "mdec");
        assert_eq!(doc.streams[0].height, Some(160));
    }
}
