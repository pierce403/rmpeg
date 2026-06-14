use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_voc(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_voc(bytes) {
        return Err(RmpegError::InvalidData("missing VOC header".to_string()));
    }
    if bytes.len() < 30 {
        return Err(RmpegError::UnexpectedEof {
            needed: 30,
            remaining: bytes.len(),
        });
    }

    let data_offset = usize::from(read_u16_le(bytes, 20)?);
    if data_offset + 6 > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_offset + 6,
            remaining: bytes.len(),
        });
    }
    if bytes[data_offset] != 1 {
        return Err(RmpegError::InvalidData(
            "VOC first block is not a sound-data block".to_string(),
        ));
    }
    let time_constant = bytes[data_offset + 4];
    let codec_byte = bytes[data_offset + 5];
    let sample_rate = 1_000_000_u32 / (256 - u32::from(time_constant));
    let (codec_name, bits_per_sample) = match codec_byte {
        1 => ("adpcm_sbpro_4", 4),
        2 => ("adpcm_sbpro_3", 3),
        3 => ("adpcm_sbpro_2", 2),
        _ => {
            return Err(RmpegError::InvalidData(format!(
                "unsupported VOC codec byte {codec_byte}"
            )));
        }
    };
    let encoded_bytes = bytes.len().saturating_sub(data_offset);
    let duration_seconds = encoded_bytes as f64 * 8.0 / bits_per_sample as f64 / sample_rate as f64;

    Ok(ProbeDocument {
        format: "voc".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec_name,
            sample_rate,
            1,
            bits_per_sample,
            duration_seconds,
        )],
    })
}

pub fn looks_like_voc(bytes: &[u8]) -> bool {
    bytes.starts_with(b"Creative Voice File\x1a")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_voc_adpcm_metadata() {
        let mut bytes = b"Creative Voice File\x1a\x1a\0\x0a\x01)\x11".to_vec();
        bytes.extend_from_slice(&[1, 2, 0, 0, 166, 3, 0, 0]);
        let doc = parse_voc(&bytes).expect("valid voc");
        assert_eq!(doc.streams[0].codec_name, "adpcm_sbpro_2");
        assert_eq!(doc.streams[0].sample_rate, Some(11_111));
        assert_eq!(doc.streams[0].bits_per_sample, Some(2));
    }
}
