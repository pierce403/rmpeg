use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy)]
struct PpBnkInfo {
    sample_rate: u32,
    channels_per_stream: u16,
    data_size: usize,
    descriptors: usize,
}

pub fn parse_pp_bnk(bytes: &[u8]) -> Result<ProbeDocument> {
    let info = parse_info(bytes)?;
    let stream_count = (info.descriptors / usize::from(info.channels_per_stream)).max(1);
    let duration_seconds = info.data_size as f64 * 2.0 / info.sample_rate as f64;
    let streams = (0..stream_count)
        .map(|index| {
            StreamMetadata::audio(
                index,
                "adpcm_ima_cunning",
                info.sample_rate,
                info.channels_per_stream,
                0,
                duration_seconds,
            )
        })
        .collect();
    Ok(ProbeDocument {
        format: "pp_bnk".to_string(),
        streams,
    })
}

fn parse_info(bytes: &[u8]) -> Result<PpBnkInfo> {
    if bytes.len() < 40 {
        return Err(RmpegError::UnexpectedEof {
            needed: 40,
            remaining: bytes.len(),
        });
    }
    let sample_rate = read_u32_le(bytes, 4)?;
    let declared_descriptors = read_u32_le(bytes, 12)?;
    let channels_per_stream = read_u32_le(bytes, 16)?;
    let data_size = usize::try_from(read_u32_le(bytes, 24)?)
        .map_err(|_| RmpegError::InvalidData("PP_BNK data size is too large".to_string()))?;

    if !matches!(sample_rate, 5_512 | 11_025 | 22_050 | 44_100 | 48_000) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported PP_BNK sample rate {sample_rate}"
        )));
    }
    if declared_descriptors == 0 || declared_descriptors > 8 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported PP_BNK descriptor count {declared_descriptors}"
        )));
    }
    if channels_per_stream == 0 || channels_per_stream > 2 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported PP_BNK channel count {channels_per_stream}"
        )));
    }
    if read_u32_le(bytes, 8)? != 1 || read_u32_le(bytes, 28)? != sample_rate {
        return Err(RmpegError::InvalidData(
            "PP_BNK descriptor header did not validate".to_string(),
        ));
    }
    if data_size == 0 {
        return Err(RmpegError::InvalidData(
            "PP_BNK data size must be nonzero".to_string(),
        ));
    }

    let mut descriptors = 1;
    let mut descriptor_pos = 40usize.checked_add(data_size).ok_or_else(|| {
        RmpegError::InvalidData("PP_BNK descriptor position overflow".to_string())
    })?;
    while descriptors < declared_descriptors as usize && descriptor_pos + 20 <= bytes.len() {
        if read_u32_le(bytes, descriptor_pos + 8)? != sample_rate {
            break;
        }
        if read_u32_le(bytes, descriptor_pos + 12)? != 1 {
            break;
        }
        descriptors += 1;
        descriptor_pos = descriptor_pos
            .checked_add(20)
            .and_then(|pos| pos.checked_add(data_size))
            .ok_or_else(|| {
                RmpegError::InvalidData("PP_BNK descriptor position overflow".to_string())
            })?;
    }

    Ok(PpBnkInfo {
        sample_rate,
        channels_per_stream: u16::try_from(channels_per_stream).map_err(|_| {
            RmpegError::InvalidData("PP_BNK channel count is too large".to_string())
        })?,
        data_size,
        descriptors,
    })
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

    fn header(sample_rate: u32, descriptors: u32, channels: u32, data_size: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&35_u32.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&descriptors.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&398_u32.to_le_bytes());
        bytes.extend_from_slice(&data_size.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_two_mono_descriptors_as_two_streams() {
        let mut bytes = header(11_025, 2, 1, 4);
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&header(11_025, 1, 1, 4)[20..40]);

        let doc = parse_pp_bnk(&bytes).expect("valid pp_bnk");
        assert_eq!(doc.format, "pp_bnk");
        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[0].codec_name, "adpcm_ima_cunning");
        assert_eq!(doc.streams[0].duration_seconds, Some(8.0 / 11_025.0));
    }

    #[test]
    fn maps_two_descriptors_for_stereo_as_one_stream() {
        let mut bytes = header(44_100, 2, 2, 4);
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&header(44_100, 1, 1, 4)[20..40]);

        let doc = parse_pp_bnk(&bytes).expect("valid pp_bnk");
        assert_eq!(doc.streams.len(), 1);
        assert_eq!(doc.streams[0].channels, Some(2));
    }
}
