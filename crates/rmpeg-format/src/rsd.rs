use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_rsd(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_rsd(bytes) {
        return Err(RmpegError::InvalidData("missing RSD header".to_string()));
    }
    if bytes.len() < 28 {
        return Err(RmpegError::UnexpectedEof {
            needed: 28,
            remaining: bytes.len(),
        });
    }

    let codec = &bytes[4..8];
    let channels = read_u32_le(bytes, 8)?;
    let sample_rate = read_u32_le(bytes, 16)?;
    if channels == 0 || channels > u32::from(u16::MAX) || sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "invalid RSD audio metadata".to_string(),
        ));
    }

    let (codec_name, data_offset, sample_count) = match codec {
        b"RADP" => {
            let data_offset = 0x800_usize;
            if bytes.len() < data_offset {
                return Err(RmpegError::UnexpectedEof {
                    needed: data_offset,
                    remaining: bytes.len(),
                });
            }
            let data_bytes = bytes.len() - data_offset;
            ("adpcm_ima_rad", data_offset, data_bytes as u64 * 4 / 5)
        }
        b"GADP" => {
            let data_offset = usize::try_from(read_u32_le(bytes, 24)?)
                .map_err(|_| RmpegError::InvalidData("RSD data offset is too large".to_string()))?;
            if bytes.len() < data_offset {
                return Err(RmpegError::UnexpectedEof {
                    needed: data_offset,
                    remaining: bytes.len(),
                });
            }
            let data_bytes = bytes.len() - data_offset;
            ("adpcm_thp_le", data_offset, data_bytes as u64 * 7 / 4)
        }
        _ => {
            return Err(RmpegError::InvalidData(
                "unsupported RSD codec tag".to_string(),
            ));
        }
    };
    if sample_count == 0 || data_offset == bytes.len() {
        return Err(RmpegError::InvalidData(
            "empty RSD audio payload".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "rsd".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec_name,
            sample_rate,
            channels as u16,
            0,
            sample_count as f64 / sample_rate as f64,
        )],
    })
}

pub fn looks_like_rsd(bytes: &[u8]) -> bool {
    bytes.len() >= 12
        && matches!(&bytes[0..4], b"RSD3" | b"RSD4")
        && matches!(&bytes[4..8], b"RADP" | b"GADP")
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
    fn parses_radp_rsd_duration_from_payload_size() {
        let mut bytes = vec![b'-'; 0x800 + 40_000];
        bytes[0..4].copy_from_slice(b"RSD4");
        bytes[4..8].copy_from_slice(b"RADP");
        bytes[8..12].copy_from_slice(&2_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&24_000_u32.to_le_bytes());

        let doc = parse_rsd(&bytes).expect("rsd radp");

        assert_eq!(doc.format, "rsd");
        assert_eq!(doc.streams[0].codec_name, "adpcm_ima_rad");
        assert_eq!(doc.streams[0].sample_rate, Some(24_000));
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].duration_seconds, Some(32_000.0 / 24_000.0));
    }

    #[test]
    fn parses_gadp_rsd_duration_from_payload_size() {
        let mut bytes = vec![0; 160 + 2_240];
        bytes[0..4].copy_from_slice(b"RSD3");
        bytes[4..8].copy_from_slice(b"GADP");
        bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&22_050_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&160_u32.to_le_bytes());

        let doc = parse_rsd(&bytes).expect("rsd gadp");

        assert_eq!(doc.streams[0].codec_name, "adpcm_thp_le");
        assert_eq!(doc.streams[0].duration_seconds, Some(3_920.0 / 22_050.0));
    }
}
