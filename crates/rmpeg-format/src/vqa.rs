use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_vqa(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_vqa(bytes) {
        return Err(RmpegError::InvalidData(
            "missing Westwood VQA header".to_string(),
        ));
    }
    let vqhd = find_vqhd(bytes)?;
    if vqhd.len() < 27 {
        return Err(RmpegError::UnexpectedEof {
            needed: 27,
            remaining: vqhd.len(),
        });
    }
    let frame_count = u32::from(read_u16_le(vqhd, 4)?);
    let width = u32::from(read_u16_le(vqhd, 6)?);
    let height = u32::from(read_u16_le(vqhd, 8)?);
    let frame_rate = u32::from(vqhd[12]);
    if frame_count == 0 || width == 0 || height == 0 || frame_rate == 0 {
        return Err(RmpegError::InvalidData(
            "invalid Westwood VQA stream metadata".to_string(),
        ));
    }
    let duration = frame_count as f64 / frame_rate as f64;
    let (audio_codec, bits_per_sample) = if read_u16_le(vqhd, 24)? == 22_050 && vqhd[26] == 1 {
        ("adpcm_ima_ws", 4)
    } else {
        ("westwood_snd1", 0)
    };

    Ok(ProbeDocument {
        format: "wsvqa".to_string(),
        streams: vec![
            StreamMetadata::video(0, "ws_vqa", width, height, Some(duration), None),
            StreamMetadata::audio(1, audio_codec, 22_050, 1, bits_per_sample, duration),
        ],
    })
}

pub fn looks_like_vqa(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"FORM" && &bytes[8..12] == b"WVQA"
}

fn find_vqhd(bytes: &[u8]) -> Result<&[u8]> {
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = usize::try_from(read_u32_be(bytes, pos + 4)?)
            .map_err(|_| RmpegError::InvalidData("VQA chunk size is too large".to_string()))?;
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("VQA chunk size overflow".to_string()))?;
        if data_end > bytes.len() {
            break;
        }
        if id == b"VQHD" {
            return Ok(&bytes[data_start..data_end]);
        }
        pos = data_end + (size % 2);
    }
    Err(RmpegError::InvalidData(
        "missing VQA VQHD chunk".to_string(),
    ))
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

    #[test]
    fn parses_observed_vqa_header() {
        let mut vqhd = vec![0; 42];
        vqhd[4..6].copy_from_slice(&96_u16.to_le_bytes());
        vqhd[6..8].copy_from_slice(&140_u16.to_le_bytes());
        vqhd[8..10].copy_from_slice(&110_u16.to_le_bytes());
        vqhd[12] = 15;
        vqhd[24..26].copy_from_slice(&22_050_u16.to_le_bytes());
        vqhd[26] = 1;
        let mut bytes = b"FORM\0\0\0\0WVQAVQHD".to_vec();
        bytes.extend_from_slice(&(vqhd.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&vqhd);

        let doc = parse_vqa(&bytes).expect("vqa");

        assert_eq!(doc.format, "wsvqa");
        assert_eq!(doc.streams[0].codec_name, "ws_vqa");
        assert_eq!(doc.streams[0].width, Some(140));
        assert_eq!(doc.streams[1].codec_name, "adpcm_ima_ws");
        assert_eq!(doc.streams[1].bits_per_sample, Some(4));
        assert_eq!(doc.streams[0].duration_seconds, Some(6.4));
    }
}
