use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Default)]
struct SmjpegState {
    duration_seconds: f64,
    audio_sample_rate: Option<u32>,
    audio_channels: Option<u16>,
    video_width: Option<u32>,
    video_height: Option<u32>,
}

pub fn parse_smjpeg(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_smjpeg(bytes) {
        return Err(RmpegError::InvalidData(
            "missing SMJPEG signature".to_string(),
        ));
    }
    if bytes.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: bytes.len(),
        });
    }

    let mut state = SmjpegState {
        duration_seconds: read_u32_be(bytes, 12)? as f64 / 1000.0,
        ..SmjpegState::default()
    };
    let mut pos = 16;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = usize::try_from(read_u32_be(bytes, pos + 4)?)
            .map_err(|_| RmpegError::InvalidData("SMJPEG chunk size is too large".to_string()))?;
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("SMJPEG chunk size overflow".to_string()))?;
        if data_end > bytes.len() {
            break;
        }
        match id {
            b"_SND" => parse_snd(&bytes[data_start..data_end], &mut state)?,
            b"_VID" => parse_vid(&bytes[data_start..data_end], &mut state)?,
            b"HEND" => break,
            _ => {}
        }
        pos = data_end;
    }

    let mut streams = Vec::new();
    if let Some(sample_rate) = state.audio_sample_rate {
        streams.push(StreamMetadata::audio(
            streams.len(),
            "adpcm_ima_smjpeg",
            sample_rate,
            state.audio_channels.unwrap_or(1),
            0,
            state.duration_seconds,
        ));
    }
    if let (Some(width), Some(height)) = (state.video_width, state.video_height) {
        streams.push(StreamMetadata::video(
            streams.len(),
            "mjpeg",
            width,
            height,
            Some(state.duration_seconds),
            None,
        ));
    }
    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "SMJPEG stream descriptors missing".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "smjpeg".to_string(),
        streams,
    })
}

pub fn looks_like_smjpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && &bytes[2..8] == b"SMJPEG"
}

fn parse_snd(data: &[u8], state: &mut SmjpegState) -> Result<()> {
    if data.len() < 8 {
        return Err(RmpegError::UnexpectedEof {
            needed: 8,
            remaining: data.len(),
        });
    }
    state.audio_sample_rate = Some(u32::from(read_u16_be(data, 0)?));
    state.audio_channels = Some(u16::from(data[3]));
    Ok(())
}

fn parse_vid(data: &[u8], state: &mut SmjpegState) -> Result<()> {
    if data.len() < 12 {
        return Err(RmpegError::UnexpectedEof {
            needed: 12,
            remaining: data.len(),
        });
    }
    state.video_width = Some(u32::from(read_u16_be(data, 4)?));
    state.video_height = Some(u32::from(read_u16_be(data, 6)?));
    Ok(())
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

    #[test]
    fn parses_smjpeg_streams() {
        let mut bytes = b"\0\x0aSMJPEG\0\0\0\0\0\0\x03\xe8".to_vec();
        bytes.extend_from_slice(b"_SND\0\0\0\x08");
        bytes.extend_from_slice(&[0x56, 0x22, 0x10, 1, b'A', b'P', b'C', b'M']);
        bytes.extend_from_slice(b"_VID\0\0\0\x0c");
        bytes.extend_from_slice(&[0, 0, 0, 1, 1, 0x40, 0, 0xf0, b'M', b'J', b'P', b'G']);
        let doc = parse_smjpeg(&bytes).expect("valid smjpeg");
        assert_eq!(doc.streams[0].sample_rate, Some(22_050));
        assert_eq!(doc.streams[1].width, Some(320));
        assert_eq!(doc.streams[1].duration_seconds, Some(1.0));
    }
}
