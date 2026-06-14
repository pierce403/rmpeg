use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_alp(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_alp(bytes) {
        return Err(RmpegError::InvalidData("missing ALP header".to_string()));
    }
    if bytes.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: bytes.len(),
        });
    }
    let header_size = usize::try_from(read_u32_le(bytes, 4)?)
        .map_err(|_| RmpegError::InvalidData("ALP header is too large".to_string()))?;
    let data_start = 8 + header_size;
    if data_start > bytes.len() || header_size < 8 {
        return Err(RmpegError::InvalidData(
            "invalid ALP header size".to_string(),
        ));
    }
    if bytes.get(8..14) != Some(b"ADPCM\0") {
        return Err(RmpegError::InvalidData(
            "unsupported ALP codec header".to_string(),
        ));
    }
    let channels = u16::from(bytes[15]);
    let sample_rate = if header_size >= 12 {
        read_u32_le(bytes, 16)?
    } else if channels == 2 {
        22_050
    } else {
        11_025
    };
    if channels == 0 || sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "ALP audio metadata must be nonzero".to_string(),
        ));
    }
    let duration_seconds =
        (bytes.len() - data_start) as f64 * 2.0 / sample_rate as f64 / f64::from(channels);
    Ok(ProbeDocument {
        format: "alp".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "adpcm_ima_alp",
            sample_rate,
            channels,
            4,
            duration_seconds,
        )],
    })
}

pub fn looks_like_alp(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ALP ")
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
    fn parses_alp_adpcm_header() {
        let mut bytes = b"ALP ".to_vec();
        bytes.extend_from_slice(&12_u32.to_le_bytes());
        bytes.extend_from_slice(b"ADPCM\0");
        bytes.push(0);
        bytes.push(1);
        bytes.extend_from_slice(&11_025_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 11025]);

        let doc = parse_alp(&bytes).expect("valid alp");
        assert_eq!(doc.streams[0].codec_name, "adpcm_ima_alp");
        assert_eq!(doc.streams[0].duration_seconds, Some(2.0));
    }
}
