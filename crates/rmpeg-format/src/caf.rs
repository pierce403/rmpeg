use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Default)]
struct CafState {
    sample_rate: Option<u32>,
    codec_name: Option<&'static str>,
    channels: Option<u16>,
    bits_per_sample: Option<u16>,
    bytes_per_packet: Option<u32>,
    frames_per_packet: Option<u32>,
    packet_frames: Option<u64>,
    priming_frames: Option<u32>,
    remainder_frames: Option<u32>,
    data_size: Option<u64>,
}

pub fn parse_caf(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_caf(bytes) {
        return Err(RmpegError::InvalidData("missing CAF header".to_string()));
    }

    let mut state = CafState::default();
    let mut pos = 8;
    while pos + 12 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = read_i64_be(bytes, pos + 4)?;
        if size < 0 {
            break;
        }
        let size = usize::try_from(size)
            .map_err(|_| RmpegError::InvalidData("CAF chunk size is too large".to_string()))?;
        let data_start = pos + 12;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("CAF chunk size overflow".to_string()))?;
        if data_end > bytes.len() {
            break;
        }
        match id {
            b"desc" => parse_desc(&bytes[data_start..data_end], &mut state)?,
            b"pakt" => parse_pakt(&bytes[data_start..data_end], &mut state)?,
            b"data" => state.data_size = Some(size as u64),
            _ => {}
        }
        pos = data_end;
    }

    let sample_rate = state
        .sample_rate
        .filter(|sample_rate| *sample_rate != 0)
        .ok_or_else(|| RmpegError::InvalidData("CAF sample rate missing".to_string()))?;
    let codec_name = state
        .codec_name
        .ok_or_else(|| RmpegError::InvalidData("CAF codec missing".to_string()))?;
    let duration_seconds = caf_duration(&state, sample_rate);

    Ok(ProbeDocument {
        format: "caf".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec_name,
            sample_rate,
            state.channels.unwrap_or(0),
            state.bits_per_sample.unwrap_or(0),
            duration_seconds,
        )],
    })
}

pub fn looks_like_caf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"caff")
}

fn parse_desc(data: &[u8], state: &mut CafState) -> Result<()> {
    if data.len() < 32 {
        return Err(RmpegError::UnexpectedEof {
            needed: 32,
            remaining: data.len(),
        });
    }
    let sample_rate = f64::from_be_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);
    state.sample_rate = Some(sample_rate.round() as u32);
    state.codec_name = Some(match &data[8..12] {
        b"aac " => "aac",
        b"opus" => "opus",
        b"lpcm" => match read_u32_be(data, 12)? {
            2 => "pcm_s16be",
            _ => "pcm_s16be",
        },
        _ => {
            return Err(RmpegError::InvalidData(format!(
                "unsupported CAF codec {}",
                String::from_utf8_lossy(&data[8..12])
            )));
        }
    });
    state.bytes_per_packet = Some(read_u32_be(data, 16)?);
    state.frames_per_packet = Some(read_u32_be(data, 20)?);
    state.channels = Some(
        u16::try_from(read_u32_be(data, 24)?)
            .map_err(|_| RmpegError::InvalidData("CAF channel count is too large".to_string()))?,
    );
    state.bits_per_sample =
        Some(u16::try_from(read_u32_be(data, 28)?).map_err(|_| {
            RmpegError::InvalidData("CAF bits per sample is too large".to_string())
        })?);
    Ok(())
}

fn parse_pakt(data: &[u8], state: &mut CafState) -> Result<()> {
    if data.len() < 24 {
        return Err(RmpegError::UnexpectedEof {
            needed: 24,
            remaining: data.len(),
        });
    }
    state.packet_frames = Some(read_u64_be(data, 8)?);
    state.priming_frames = Some(read_u32_be(data, 16)?);
    state.remainder_frames = Some(read_u32_be(data, 20)?);
    Ok(())
}

fn caf_duration(state: &CafState, sample_rate: u32) -> f64 {
    if let Some(packet_frames) = state.packet_frames {
        let frames = packet_frames
            .saturating_add(u64::from(state.priming_frames.unwrap_or(0)))
            .saturating_add(u64::from(state.remainder_frames.unwrap_or(0)));
        return frames as f64 / sample_rate as f64;
    }
    match (
        state.data_size,
        state.bytes_per_packet,
        state.frames_per_packet,
    ) {
        (Some(data_size), Some(bytes_per_packet), Some(frames_per_packet))
            if bytes_per_packet != 0 =>
        {
            data_size as f64 * frames_per_packet as f64
                / bytes_per_packet as f64
                / sample_rate as f64
        }
        _ => 0.0,
    }
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

fn read_u64_be(bytes: &[u8], pos: usize) -> Result<u64> {
    let end = pos + 8;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u64::from_be_bytes([
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

fn read_i64_be(bytes: &[u8], pos: usize) -> Result<i64> {
    Ok(read_u64_be(bytes, pos)? as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caf_desc(codec: &[u8; 4], bytes_per_packet: u32, frames_per_packet: u32) -> Vec<u8> {
        let mut bytes = b"caff\0\x01\0\0desc\0\0\0\0\0\0\0\x20".to_vec();
        bytes.extend_from_slice(&48_000_f64.to_be_bytes());
        bytes.extend_from_slice(codec);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&bytes_per_packet.to_be_bytes());
        bytes.extend_from_slice(&frames_per_packet.to_be_bytes());
        bytes.extend_from_slice(&2_u32.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes
    }

    #[test]
    fn parses_packet_table_duration() {
        let mut bytes = caf_desc(b"aac ", 0, 1024);
        bytes.extend_from_slice(b"pakt\0\0\0\0\0\0\0\x18");
        bytes.extend_from_slice(&1_u64.to_be_bytes());
        bytes.extend_from_slice(&48_000_u64.to_be_bytes());
        bytes.extend_from_slice(&24_u32.to_be_bytes());
        bytes.extend_from_slice(&24_u32.to_be_bytes());
        let doc = parse_caf(&bytes).expect("valid caf");
        assert_eq!(doc.streams[0].codec_name, "aac");
        assert_eq!(doc.streams[0].duration_seconds, Some(1.001));
    }
}
