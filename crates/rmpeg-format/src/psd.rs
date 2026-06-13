use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_psd(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 26 {
        return Err(RmpegError::UnexpectedEof {
            needed: 26,
            remaining: bytes.len(),
        });
    }
    if &bytes[0..4] != b"8BPS" {
        return Err(RmpegError::InvalidData("missing PSD signature".to_string()));
    }

    let version = read_u16_be(bytes, 4)?;
    if !matches!(version, 1 | 2) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported PSD version {version}"
        )));
    }
    let channels = read_u16_be(bytes, 12)?;
    let height = read_u32_be(bytes, 14)?;
    let width = read_u32_be(bytes, 18)?;
    let depth = read_u16_be(bytes, 22)?;
    if channels == 0 || channels > 64 {
        return Err(RmpegError::InvalidData(format!(
            "invalid PSD channel count {channels}"
        )));
    }
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "PSD dimensions must be nonzero".to_string(),
        ));
    }
    if !matches!(depth, 1 | 8 | 16 | 32) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported PSD bit depth {depth}"
        )));
    }

    Ok(ProbeDocument {
        format: "psd_pipe".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "psd",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_psd(bytes: &[u8]) -> bool {
    bytes.starts_with(b"8BPS")
}

fn read_u16_be(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_u32_be(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_psd(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0; 26];
        bytes[0..4].copy_from_slice(b"8BPS");
        bytes[4..6].copy_from_slice(&1_u16.to_be_bytes());
        bytes[12..14].copy_from_slice(&3_u16.to_be_bytes());
        bytes[14..18].copy_from_slice(&height.to_be_bytes());
        bytes[18..22].copy_from_slice(&width.to_be_bytes());
        bytes[22..24].copy_from_slice(&8_u16.to_be_bytes());
        bytes
    }

    #[test]
    fn parses_psd_dimensions() {
        let doc = parse_psd(&minimal_psd(128, 64)).expect("valid psd");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "psd_pipe");
        assert_eq!(stream.codec_name, "psd");
        assert_eq!(stream.width, Some(128));
        assert_eq!(stream.height, Some(64));
    }

    #[test]
    fn rejects_bad_depth() {
        let mut bytes = minimal_psd(1, 1);
        bytes[22..24].copy_from_slice(&7_u16.to_be_bytes());
        let err = parse_psd(&bytes).expect_err("bad depth");
        assert!(err.to_string().contains("bit depth"));
    }
}
