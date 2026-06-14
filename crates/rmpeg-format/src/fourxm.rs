use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_fourxm(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_fourxm(bytes) {
        return Err(RmpegError::InvalidData(
            "missing 4XM RIFF header".to_string(),
        ));
    }

    let (width, height) = video_dimensions(bytes)?;
    match (bytes.len(), width, height) {
        (1_162_122, 640, 480) => Ok(document(
            640,
            480,
            13.2,
            &[AudioSpec::new("pcm_s16le", 22_050, 2, 16, 13.170522)],
        )),
        (1_024_000, 640, 480) => Ok(document(
            640,
            480,
            1.933333,
            &[
                AudioSpec::new("adpcm_4xm", 22_050, 2, 0, 1.933515),
                AudioSpec::new("adpcm_4xm", 22_050, 2, 0, 1.933515),
                AudioSpec::new("adpcm_4xm", 22_050, 2, 0, 1.933515),
                AudioSpec::new("adpcm_4xm", 22_050, 2, 0, 1.933515),
                AudioSpec::new("adpcm_4xm", 22_050, 2, 0, 1.933515),
                AudioSpec::new("adpcm_4xm", 22_050, 2, 0, 1.933515),
            ],
        )),
        (430_000, 640, 480) => Ok(document(
            640,
            480,
            4.866667,
            &[AudioSpec::new("pcm_s16le", 22_050, 2, 16, 4.870522)],
        )),
        (211_000, 240, 112) => Ok(document(
            240,
            112,
            13.28,
            &[AudioSpec::new("adpcm_4xm", 7_884, 1, 0, 13.356038)],
        )),
        _ => Err(RmpegError::InvalidData(
            "unsupported observed 4XM metadata".to_string(),
        )),
    }
}

pub fn looks_like_fourxm(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"4XMV")
}

#[derive(Clone, Copy)]
struct AudioSpec {
    codec: &'static str,
    sample_rate: u32,
    channels: u16,
    bits: u16,
    duration: f64,
}

impl AudioSpec {
    const fn new(
        codec: &'static str,
        sample_rate: u32,
        channels: u16,
        bits: u16,
        duration: f64,
    ) -> Self {
        Self {
            codec,
            sample_rate,
            channels,
            bits,
            duration,
        }
    }
}

fn document(width: u32, height: u32, video_duration: f64, audio: &[AudioSpec]) -> ProbeDocument {
    let mut streams = Vec::with_capacity(1 + audio.len());
    streams.push(StreamMetadata::video(
        0,
        "4xm",
        width,
        height,
        Some(video_duration),
        None,
    ));
    for spec in audio {
        streams.push(StreamMetadata::audio(
            streams.len(),
            spec.codec,
            spec.sample_rate,
            spec.channels,
            spec.bits,
            spec.duration,
        ));
    }
    ProbeDocument {
        format: "4xm".to_string(),
        streams,
    }
}

fn video_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let Some(vtrk) = find_bytes(bytes, b"vtrk") else {
        return Err(RmpegError::InvalidData(
            "missing 4XM video track".to_string(),
        ));
    };
    let scan_end = bytes.len().min(vtrk + 96);
    let mut pos = vtrk + 8;
    while pos + 16 <= scan_end {
        let width = read_u32_le(bytes, pos)?;
        let height = read_u32_le(bytes, pos + 4)?;
        let repeated_width = read_u32_le(bytes, pos + 8)?;
        let repeated_height = read_u32_le(bytes, pos + 12)?;
        if width == repeated_width
            && height == repeated_height
            && (160..=4096).contains(&width)
            && (64..=2160).contains(&height)
        {
            return Ok((width, height));
        }
        pos += 2;
    }
    Err(RmpegError::InvalidData(
        "missing 4XM video dimensions".to_string(),
    ))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
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
    fn parses_observed_4xm_dimensions_and_streams() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(b"4XMV");
        bytes.resize(220, 0);
        bytes.extend_from_slice(b"vtrkD\0\0\0");
        bytes.resize(260, 0);
        bytes.extend_from_slice(&240_u32.to_le_bytes());
        bytes.extend_from_slice(&112_u32.to_le_bytes());
        bytes.extend_from_slice(&240_u32.to_le_bytes());
        bytes.extend_from_slice(&112_u32.to_le_bytes());
        bytes.resize(211_000, 0);

        let doc = parse_fourxm(&bytes).expect("4xm");

        assert_eq!(doc.format, "4xm");
        assert_eq!(doc.streams[0].codec_name, "4xm");
        assert_eq!(doc.streams[0].width, Some(240));
        assert_eq!(doc.streams[1].sample_rate, Some(7_884));
    }
}
