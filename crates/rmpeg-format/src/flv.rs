use crate::{aac::parse_audio_specific_config, h264::parse_h264_annex_b};
use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_flv(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_flv(bytes) {
        return Err(RmpegError::InvalidData("missing FLV header".to_string()));
    }
    let data_offset = read_u32_be(bytes, 5)? as usize;
    let mut pos = data_offset
        .checked_add(4)
        .ok_or_else(|| RmpegError::InvalidData("FLV data offset overflow".to_string()))?;
    if pos > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: pos,
            remaining: bytes.len(),
        });
    }

    let mut streams = Vec::new();
    let mut saw_h264 = false;
    let mut saw_audio = false;
    while pos + 11 <= bytes.len() {
        let tag_type = bytes[pos];
        let data_size = read_u24_be(bytes, pos + 1)? as usize;
        let data_start = pos + 11;
        let data_end = data_start
            .checked_add(data_size)
            .ok_or_else(|| RmpegError::InvalidData("FLV tag size overflow".to_string()))?;
        if data_end > bytes.len() {
            break;
        }
        let data = &bytes[data_start..data_end];
        match tag_type {
            8 if !saw_audio => {
                if let Some(stream) = parse_audio_tag(data, streams.len())? {
                    streams.push(stream);
                    saw_audio = true;
                }
            }
            9 if !saw_h264 => {
                if let Some(stream) = parse_h264_tag(data, streams.len())? {
                    streams.push(stream);
                    saw_h264 = true;
                }
            }
            _ => {}
        }

        pos = data_end.saturating_add(4);
    }

    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "FLV file has no supported sequence headers".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "flv".to_string(),
        streams,
    })
}

pub fn looks_like_flv(bytes: &[u8]) -> bool {
    bytes.len() >= 9 && bytes.starts_with(b"FLV") && bytes[3] == 1
}

fn parse_audio_tag(data: &[u8], index: usize) -> Result<Option<StreamMetadata>> {
    if data.is_empty() {
        return Ok(None);
    }
    let sound_format = data[0] >> 4;
    if sound_format == 10 {
        return parse_aac_tag(data, index);
    }
    if sound_format == 6 {
        let sample_rate = match (data[0] >> 2) & 0x03 {
            0 => 5_500,
            1 => 11_025,
            2 => 22_050,
            3 => 44_100,
            _ => unreachable!(),
        };
        let channels = if data[0] & 0x01 != 0 { 2 } else { 1 };
        return Ok(Some(StreamMetadata::audio(
            index,
            "nellymoser",
            sample_rate,
            channels,
            0,
            0.0,
        )));
    }
    Ok(None)
}

fn parse_aac_tag(data: &[u8], index: usize) -> Result<Option<StreamMetadata>> {
    if data.len() < 4 {
        return Ok(None);
    }
    let sound_format = data[0] >> 4;
    let aac_packet_type = data[1];
    if sound_format != 10 || aac_packet_type != 0 {
        return Ok(None);
    }
    let Some(config) = parse_audio_specific_config(&data[2..]) else {
        return Err(RmpegError::InvalidData(
            "invalid FLV AAC sequence header".to_string(),
        ));
    };
    Ok(Some(StreamMetadata::audio(
        index,
        config.codec_name,
        config.sample_rate.unwrap_or(0),
        config.channels.unwrap_or(0),
        config.bits_per_sample.unwrap_or(0),
        0.0,
    )))
}

fn parse_h264_tag(data: &[u8], index: usize) -> Result<Option<StreamMetadata>> {
    if data.len() < 11 {
        return Ok(None);
    }
    let codec_id = data[0] & 0x0f;
    let avc_packet_type = data[1];
    if codec_id != 7 || avc_packet_type != 0 {
        return Ok(None);
    }
    let sps = first_avc_sps(&data[5..])?;
    let mut annex_b = Vec::with_capacity(sps.len() + 4);
    annex_b.extend_from_slice(&[0, 0, 0, 1]);
    annex_b.extend_from_slice(sps);
    let doc = parse_h264_annex_b(&annex_b)?;
    let stream = doc.streams.first().ok_or_else(|| {
        RmpegError::InvalidData("FLV AVC sequence header has no SPS stream".to_string())
    })?;

    Ok(Some(StreamMetadata::video(
        index,
        "h264",
        stream.width.unwrap_or(0),
        stream.height.unwrap_or(0),
        Some(0.0),
        None,
    )))
}

fn first_avc_sps(config: &[u8]) -> Result<&[u8]> {
    if config.len() < 7 || config[0] != 1 {
        return Err(RmpegError::InvalidData(
            "invalid AVCDecoderConfigurationRecord".to_string(),
        ));
    }
    let sps_count = config[5] & 0x1f;
    if sps_count == 0 {
        return Err(RmpegError::InvalidData(
            "AVC configuration has no SPS".to_string(),
        ));
    }
    let sps_len = usize::from(read_u16_be(config, 6)?);
    let sps_start = 8_usize;
    let sps_end = sps_start
        .checked_add(sps_len)
        .ok_or_else(|| RmpegError::InvalidData("AVC SPS size overflow".to_string()))?;
    if sps_end > config.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: sps_end,
            remaining: config.len(),
        });
    }
    Ok(&config[sps_start..sps_end])
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u24_be(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 3;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok((u32::from(bytes[offset]) << 16)
        | (u32::from(bytes[offset + 1]) << 8)
        | u32::from(bytes[offset + 2]))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aac_sequence_header() {
        let stream = parse_aac_tag(&[0xaf, 0x00, 0x12, 0x10], 0)
            .expect("valid tag")
            .expect("aac stream");

        assert_eq!(stream.codec_name, "aac");
        assert_eq!(stream.sample_rate, Some(44_100));
        assert_eq!(stream.channels, Some(2));
    }

    #[test]
    fn parses_nellymoser_audio_tag_metadata() {
        let stream = parse_audio_tag(&[0x6a, 0xbe, 0x5b], 0)
            .expect("valid tag")
            .expect("nelly stream");

        assert_eq!(stream.codec_name, "nellymoser");
        assert_eq!(stream.sample_rate, Some(22_050));
        assert_eq!(stream.channels, Some(1));
        assert_eq!(stream.bits_per_sample, Some(0));
    }
}
