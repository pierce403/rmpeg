use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_bink(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_bink(bytes) {
        return Err(RmpegError::InvalidData("missing Bink header".to_string()));
    }

    let frame_count = read_u32(bytes, 8)?;
    let width = read_u32(bytes, 20)?;
    let height = read_u32(bytes, 24)?;
    let fps_num = read_u32(bytes, 28)?;
    let fps_den = read_u32(bytes, 32)?;
    if frame_count == 0 || width == 0 || height == 0 || fps_num == 0 || fps_den == 0 {
        return Err(RmpegError::InvalidData(
            "invalid Bink stream metadata".to_string(),
        ));
    }

    let mut streams = vec![StreamMetadata::video(
        0,
        "binkvideo",
        width,
        height,
        Some(frame_count as f64 * fps_den as f64 / fps_num as f64),
        None,
    )];
    let audio_tracks = read_u32(bytes, 40).unwrap_or(0);
    if audio_tracks > 0 {
        if let Some(audio) = parse_first_audio_track(bytes, streams.len()) {
            streams.push(audio);
        }
    }

    Ok(ProbeDocument {
        format: "bink".to_string(),
        streams,
    })
}

pub fn looks_like_bink(bytes: &[u8]) -> bool {
    bytes.len() >= 44 && &bytes[0..3] == b"BIK" && bytes[3].is_ascii_alphabetic()
}

fn parse_first_audio_track(bytes: &[u8], index: usize) -> Option<StreamMetadata> {
    let track = 44;
    let sample_rate = u32::from(read_u16(bytes, track + 4).ok()?);
    let flags = *bytes.get(track + 7)?;
    if sample_rate == 0 {
        return None;
    }
    let codec = if flags & 0x80 != 0 {
        "binkaudio_rdft"
    } else {
        "binkaudio_dct"
    };
    let channels = if flags & 0x20 != 0 { 2 } else { 1 };
    Some(StreamMetadata::audio(
        index,
        codec,
        sample_rate,
        channels,
        0,
        0.0,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
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

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_bink_video_and_audio_header() {
        let mut bytes = vec![0; 64];
        bytes[0..4].copy_from_slice(b"BIKi");
        bytes[8..12].copy_from_slice(&31_u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&640_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&480_u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&30_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&1_u32.to_le_bytes());
        bytes[40..44].copy_from_slice(&1_u32.to_le_bytes());
        bytes[48..50].copy_from_slice(&44100_u16.to_le_bytes());
        bytes[51] = 0x70;

        let doc = parse_bink(&bytes).expect("bink");

        assert_eq!(doc.format, "bink");
        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[0].codec_name, "binkvideo");
        assert_eq!(doc.streams[0].duration_seconds, Some(31.0 / 30.0));
        assert_eq!(doc.streams[1].codec_name, "binkaudio_dct");
        assert_eq!(doc.streams[1].sample_rate, Some(44100));
        assert_eq!(doc.streams[1].channels, Some(2));
    }

    #[test]
    fn parses_observed_bink_rdft_flag() {
        let mut bytes = vec![0; 64];
        bytes[0..4].copy_from_slice(b"BIKi");
        bytes[8..12].copy_from_slice(&280_u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&384_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&512_u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&25_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&1_u32.to_le_bytes());
        bytes[40..44].copy_from_slice(&1_u32.to_le_bytes());
        bytes[48..50].copy_from_slice(&44100_u16.to_le_bytes());
        bytes[51] = 0xe0;

        let doc = parse_bink(&bytes).expect("bink");

        assert_eq!(doc.streams[1].codec_name, "binkaudio_rdft");
        assert_eq!(doc.streams[1].channels, Some(2));
    }
}
