use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy)]
pub(crate) struct MpegAudioFrame {
    pub(crate) codec_name: &'static str,
    pub(crate) frame_len: usize,
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) samples_per_frame: u32,
}

pub fn parse_mp3(bytes: &[u8]) -> Result<ProbeDocument> {
    let mut pos = id3v2_skip(bytes)?;
    let mut first = None;
    let mut frames = 0_u32;

    while pos + 4 <= bytes.len() {
        if bytes[pos..].starts_with(b"TAG") {
            break;
        }

        match parse_mpeg_audio_frame_header(&bytes[pos..pos + 4]) {
            Some(frame) if pos + frame.frame_len <= bytes.len() => {
                first.get_or_insert(frame);
                frames += 1;
                pos += frame.frame_len;
            }
            _ => pos += 1,
        }
    }

    let first = first.ok_or_else(|| RmpegError::InvalidData("no MP3 frames found".to_string()))?;
    let duration_seconds =
        frames as f64 * first.samples_per_frame as f64 / first.sample_rate as f64;

    Ok(ProbeDocument {
        format: "mp3".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            first.codec_name,
            first.sample_rate,
            first.channels,
            0,
            duration_seconds,
        )],
    })
}

pub(crate) fn mpeg_audio_frame_stats(bytes: &[u8]) -> Option<(MpegAudioFrame, u32)> {
    if bytes.len() < 4 {
        return None;
    }
    let mut pos = (0..=bytes.len().saturating_sub(4)).find(|&pos| {
        let Some(frame) = parse_mpeg_audio_frame_header(&bytes[pos..pos + 4]) else {
            return false;
        };
        pos + frame.frame_len <= bytes.len()
    })?;
    let first = parse_mpeg_audio_frame_header(&bytes[pos..pos + 4])?;
    let mut frames = 0_u32;
    while pos + 4 <= bytes.len() {
        match parse_mpeg_audio_frame_header(&bytes[pos..pos + 4]) {
            Some(frame) if pos + frame.frame_len <= bytes.len() => {
                frames += 1;
                pos += frame.frame_len;
            }
            _ => break,
        }
    }
    Some((first, frames))
}

fn id3v2_skip(bytes: &[u8]) -> Result<usize> {
    if !bytes.starts_with(b"ID3") {
        return Ok(0);
    }
    if bytes.len() < 10 {
        return Err(RmpegError::UnexpectedEof {
            needed: 10,
            remaining: bytes.len(),
        });
    }
    if bytes[6..10].iter().any(|byte| byte & 0x80 != 0) {
        return Err(RmpegError::InvalidData(
            "invalid ID3 synchsafe size".to_string(),
        ));
    }
    let size = ((usize::from(bytes[6])) << 21)
        | ((usize::from(bytes[7])) << 14)
        | ((usize::from(bytes[8])) << 7)
        | usize::from(bytes[9]);
    Ok(10 + size)
}

pub(crate) fn parse_mpeg_audio_frame_header(header: &[u8]) -> Option<MpegAudioFrame> {
    if header.len() != 4 {
        return None;
    }
    let raw = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    if raw & 0xffe0_0000 != 0xffe0_0000 {
        return None;
    }

    let version_id = (raw >> 19) & 0b11;
    let layer = (raw >> 17) & 0b11;
    let bitrate_index = ((raw >> 12) & 0b1111) as usize;
    let sample_rate_index = ((raw >> 10) & 0b11) as usize;
    let padding = ((raw >> 9) & 0b1) as usize;
    let channel_mode = (raw >> 6) & 0b11;

    if version_id == 0b01 || bitrate_index == 0 || bitrate_index == 15 {
        return None;
    }

    let sample_rate = sample_rate(version_id, sample_rate_index)?;
    let (codec_name, bitrate_kbps, samples_per_frame, coefficient) = match layer {
        0b10 => {
            let bitrate = match version_id {
                0b11 => MPEG1_LAYER2_BITRATES[bitrate_index],
                _ => MPEG2_LAYER2_AND_3_BITRATES[bitrate_index],
            }?;
            ("mp2", bitrate, 1152, 144_000)
        }
        0b01 => {
            let bitrate = match version_id {
                0b11 => MPEG1_LAYER3_BITRATES[bitrate_index],
                _ => MPEG2_LAYER2_AND_3_BITRATES[bitrate_index],
            }?;
            let samples = if version_id == 0b11 { 1152 } else { 576 };
            let coeff = if version_id == 0b11 { 144_000 } else { 72_000 };
            ("mp3", bitrate, samples, coeff)
        }
        _ => return None,
    };
    let frame_len = coefficient * bitrate_kbps as usize / sample_rate as usize + padding;

    Some(MpegAudioFrame {
        codec_name,
        frame_len,
        sample_rate,
        channels: if channel_mode == 0b11 { 1 } else { 2 },
        samples_per_frame,
    })
}

fn sample_rate(version_id: u32, index: usize) -> Option<u32> {
    let base = [44_100, 48_000, 32_000, 0][index];
    if base == 0 {
        return None;
    }
    match version_id {
        0b11 => Some(base),
        0b10 => Some(base / 2),
        0b00 => Some(base / 4),
        _ => None,
    }
}

const MPEG1_LAYER3_BITRATES: [Option<u16>; 16] = [
    None,
    Some(32),
    Some(40),
    Some(48),
    Some(56),
    Some(64),
    Some(80),
    Some(96),
    Some(112),
    Some(128),
    Some(160),
    Some(192),
    Some(224),
    Some(256),
    Some(320),
    None,
];

const MPEG1_LAYER2_BITRATES: [Option<u16>; 16] = [
    None,
    Some(32),
    Some(48),
    Some(56),
    Some(64),
    Some(80),
    Some(96),
    Some(112),
    Some(128),
    Some(160),
    Some(192),
    Some(224),
    Some(256),
    Some(320),
    Some(384),
    None,
];

const MPEG2_LAYER2_AND_3_BITRATES: [Option<u16>; 16] = [
    None,
    Some(8),
    Some(16),
    Some(24),
    Some(32),
    Some(40),
    Some(48),
    Some(56),
    Some(64),
    Some(80),
    Some(96),
    Some(112),
    Some(128),
    Some(144),
    Some(160),
    None,
];
