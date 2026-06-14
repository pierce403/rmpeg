use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const RIFF_GUID: &[u8; 16] = b"riff\x2e\x91\xcf\x11\xa5\xd6\x28\xdb\x04\xc1\x00\x00";
const WAVE_GUID: &[u8; 16] = b"wave\xf3\xac\xd3\x11\x8c\xd1\x00\xc0\x4f\x8e\xdb\x8a";
const FMT_GUID: &[u8; 16] = b"fmt \xf3\xac\xd3\x11\x8c\xd1\x00\xc0\x4f\x8e\xdb\x8a";
const DATA_GUID: &[u8; 16] = b"data\xf3\xac\xd3\x11\x8c\xd1\x00\xc0\x4f\x8e\xdb\x8a";

#[derive(Default)]
struct W64State {
    channels: Option<u16>,
    sample_rate: Option<u32>,
    block_align: Option<u16>,
    bits_per_sample: Option<u16>,
    data_bytes: Option<usize>,
}

pub fn parse_w64(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_w64(bytes) {
        return Err(RmpegError::InvalidData("missing W64 header".to_string()));
    }
    let mut state = W64State::default();
    let mut pos = 40;
    while pos + 24 <= bytes.len() {
        let guid = &bytes[pos..pos + 16];
        let chunk_size = usize::try_from(read_u64_le(bytes, pos + 16)?)
            .map_err(|_| RmpegError::InvalidData("W64 chunk size is too large".to_string()))?;
        if chunk_size < 24 {
            return Err(RmpegError::InvalidData(
                "W64 chunk size is smaller than its header".to_string(),
            ));
        }
        let data_start = pos + 24;
        let declared_end = pos
            .checked_add(chunk_size)
            .ok_or_else(|| RmpegError::InvalidData("W64 chunk size overflow".to_string()))?;
        let data_end = declared_end.min(bytes.len());
        if guid == FMT_GUID {
            parse_fmt(bytes, data_start, data_end, &mut state)?;
        } else if guid == DATA_GUID {
            state.data_bytes = Some(data_end.saturating_sub(data_start));
        }
        let Some(next) = align_8(declared_end) else {
            break;
        };
        if next <= pos {
            break;
        }
        pos = next;
    }

    let channels = state
        .channels
        .ok_or_else(|| RmpegError::InvalidData("missing W64 channel count".to_string()))?;
    let sample_rate = state
        .sample_rate
        .ok_or_else(|| RmpegError::InvalidData("missing W64 sample rate".to_string()))?;
    let block_align = state
        .block_align
        .ok_or_else(|| RmpegError::InvalidData("missing W64 block align".to_string()))?;
    let bits_per_sample = state
        .bits_per_sample
        .ok_or_else(|| RmpegError::InvalidData("missing W64 bits per sample".to_string()))?;
    let data_bytes = state
        .data_bytes
        .ok_or_else(|| RmpegError::InvalidData("missing W64 data chunk".to_string()))?;
    if channels == 0 || sample_rate == 0 || block_align == 0 || bits_per_sample == 0 {
        return Err(RmpegError::InvalidData(
            "invalid W64 PCM metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "w64".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            pcm_codec_name(bits_per_sample)?,
            sample_rate,
            channels,
            bits_per_sample,
            data_bytes as f64 / f64::from(block_align) / f64::from(sample_rate),
        )],
    })
}

pub fn looks_like_w64(bytes: &[u8]) -> bool {
    bytes.len() >= 40 && &bytes[0..16] == RIFF_GUID && &bytes[24..40] == WAVE_GUID
}

fn parse_fmt(bytes: &[u8], start: usize, end: usize, state: &mut W64State) -> Result<()> {
    if start + 16 > end {
        return Err(RmpegError::UnexpectedEof {
            needed: start + 16,
            remaining: bytes.len(),
        });
    }
    let audio_format = read_u16_le(bytes, start)?;
    if audio_format != 1 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported W64 audio format {audio_format}"
        )));
    }
    state.channels = Some(read_u16_le(bytes, start + 2)?);
    state.sample_rate = Some(read_u32_le(bytes, start + 4)?);
    state.block_align = Some(read_u16_le(bytes, start + 12)?);
    state.bits_per_sample = Some(read_u16_le(bytes, start + 14)?);
    Ok(())
}

fn pcm_codec_name(bits_per_sample: u16) -> Result<&'static str> {
    match bits_per_sample {
        8 => Ok("pcm_u8"),
        16 => Ok("pcm_s16le"),
        24 => Ok("pcm_s24le"),
        32 => Ok("pcm_s32le"),
        _ => Err(RmpegError::InvalidData(format!(
            "unsupported W64 PCM bit depth {bits_per_sample}"
        ))),
    }
}

fn align_8(value: usize) -> Option<usize> {
    value.checked_add(7).map(|value| value & !7)
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

fn read_u64_le(bytes: &[u8], pos: usize) -> Result<u64> {
    let end = pos + 8;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u64::from_le_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
        bytes[pos + 4],
        bytes[pos + 5],
        bytes[pos + 6],
        bytes[pos + 7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pcm_w64_metadata() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(RIFF_GUID);
        bytes.extend_from_slice(&128_u64.to_le_bytes());
        bytes.extend_from_slice(WAVE_GUID);
        bytes.extend_from_slice(FMT_GUID);
        bytes.extend_from_slice(&48_u64.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&11_025_u32.to_le_bytes());
        bytes.extend_from_slice(&22_050_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(DATA_GUID);
        bytes.extend_from_slice(&(24_u64 + 22_050).to_le_bytes());
        bytes.extend_from_slice(&vec![0; 22_050]);

        let doc = parse_w64(&bytes).expect("w64");

        assert_eq!(doc.format, "w64");
        assert_eq!(doc.streams[0].codec_name, "pcm_s16le");
        assert_eq!(doc.streams[0].duration_seconds, Some(1.0));
    }
}
