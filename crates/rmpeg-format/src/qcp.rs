use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const QCELP_GUID: [u8; 16] = [
    0x41, 0x6d, 0x7f, 0x5e, 0x15, 0xb1, 0xd0, 0x11, 0xba, 0x91, 0x00, 0x80, 0x5f, 0xb4, 0xb9, 0x7e,
];
const EVRC_GUID: [u8; 16] = [
    0x8d, 0xd4, 0x89, 0xe6, 0x76, 0x90, 0xb5, 0x46, 0x91, 0xef, 0x73, 0x6a, 0x51, 0x00, 0xce, 0xb4,
];

pub fn parse_qcp(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_qcp(bytes) {
        return Err(RmpegError::InvalidData(
            "missing QCP RIFF header".to_string(),
        ));
    }
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = usize::try_from(read_u32_le(bytes, pos + 4)?)
            .map_err(|_| RmpegError::InvalidData("QCP chunk size is too large".to_string()))?;
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("QCP chunk size overflow".to_string()))?;
        if data_end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_end,
                remaining: bytes.len(),
            });
        }
        if id == b"fmt " {
            let (codec_name, bit_rate) = parse_fmt(&bytes[data_start..data_end])?;
            let estimated_bytes = bytes.len().saturating_sub(data_end);
            return Ok(ProbeDocument {
                format: "qcp".to_string(),
                streams: vec![StreamMetadata::audio(
                    0,
                    codec_name,
                    8_000,
                    1,
                    0,
                    estimated_bytes as f64 * 8.0 / bit_rate as f64,
                )],
            });
        }
        pos = data_end + (size % 2);
    }

    Err(RmpegError::InvalidData("missing QCP fmt chunk".to_string()))
}

pub fn looks_like_qcp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"QLCM"
}

fn parse_fmt(data: &[u8]) -> Result<(&'static str, u32)> {
    if data.len() < 18 {
        return Err(RmpegError::UnexpectedEof {
            needed: 18,
            remaining: data.len(),
        });
    }
    let guid = &data[2..18];
    if guid == QCELP_GUID {
        Ok(("qcelp", 13_000))
    } else if guid == EVRC_GUID {
        Ok(("evrc", 9_600))
    } else {
        Err(RmpegError::InvalidData(
            "unsupported QCP codec GUID".to_string(),
        ))
    }
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
    fn parses_qcelp_qcp_metadata() {
        let mut bytes = b"RIFF\0\0\0\0QLCMfmt ".to_vec();
        bytes.extend_from_slice(&150_u32.to_le_bytes());
        bytes.extend_from_slice(&[1, 0]);
        bytes.extend_from_slice(&QCELP_GUID);
        bytes.resize(12 + 8 + 150, 0);
        bytes.extend_from_slice(b"vrat");
        bytes.extend_from_slice(&[0; 16]);
        bytes.resize(12 + 8 + 150 + 11_436, 0);

        let doc = parse_qcp(&bytes).expect("qcp");

        assert_eq!(doc.format, "qcp");
        assert_eq!(doc.streams[0].codec_name, "qcelp");
        assert_eq!(doc.streams[0].sample_rate, Some(8_000));
        assert!((doc.streams[0].duration_seconds.unwrap() - 7.037538).abs() < 0.000001);
    }

    #[test]
    fn parses_evrc_qcp_metadata() {
        let mut bytes = b"RIFF\0\0\0\0QLCMfmt ".to_vec();
        bytes.extend_from_slice(&150_u32.to_le_bytes());
        bytes.extend_from_slice(&[1, 0]);
        bytes.extend_from_slice(&EVRC_GUID);
        bytes.resize(12 + 8 + 150 + 4_950, 0);

        let doc = parse_qcp(&bytes).expect("qcp");

        assert_eq!(doc.streams[0].codec_name, "evrc");
        assert_eq!(doc.streams[0].duration_seconds, Some(4.125));
    }
}
