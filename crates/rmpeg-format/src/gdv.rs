use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_gdv(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_gdv(bytes) {
        return Err(RmpegError::InvalidData("missing GDV header".to_string()));
    }

    let frames = u32::from(read_u16_le(bytes, 6)?);
    let fps = u32::from(read_u16_le(bytes, 8)?);
    let width = u32::from(read_u16_le(bytes, 20)?);
    let height = u32::from(read_u16_le(bytes, 22)?);
    if frames == 0 || fps == 0 || width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid GDV video metadata".to_string(),
        ));
    }

    let mut streams = vec![StreamMetadata::video(
        0,
        "gdv",
        width,
        height,
        Some(frames as f64 / fps as f64),
        None,
    )];
    let audio_tag = read_u16_le(bytes, 10)?;
    let sample_rate = u32::from(read_u16_le(bytes, 12)?);
    let channels = u16::from(bytes[19]);
    if audio_tag != 0 && sample_rate != 0 && channels != 0 {
        streams.push(StreamMetadata::audio(
            streams.len(),
            "gremlin_dpcm",
            sample_rate,
            channels,
            0,
            0.0,
        ));
    }

    Ok(ProbeDocument {
        format: "gdv".to_string(),
        streams,
    })
}

pub fn looks_like_gdv(bytes: &[u8]) -> bool {
    bytes.len() >= 24 && bytes[0..4] == [0x94, 0x19, 0x11, 0x29]
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
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
    fn parses_video_only_gdv_header() {
        let mut bytes = vec![0; 24];
        bytes[0..4].copy_from_slice(&[0x94, 0x19, 0x11, 0x29]);
        bytes[6..8].copy_from_slice(&21_u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&12_u16.to_le_bytes());
        bytes[20..22].copy_from_slice(&170_u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&140_u16.to_le_bytes());

        let doc = parse_gdv(&bytes).expect("gdv");

        assert_eq!(doc.format, "gdv");
        assert_eq!(doc.streams.len(), 1);
        assert_eq!(doc.streams[0].duration_seconds, Some(1.75));
    }

    #[test]
    fn parses_gdv_audio_metadata_when_present() {
        let mut bytes = vec![0; 24];
        bytes[0..4].copy_from_slice(&[0x94, 0x19, 0x11, 0x29]);
        bytes[6..8].copy_from_slice(&85_u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&12_u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&15_u16.to_le_bytes());
        bytes[12..14].copy_from_slice(&21_168_u16.to_le_bytes());
        bytes[19] = 2;
        bytes[20..22].copy_from_slice(&320_u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&280_u16.to_le_bytes());

        let doc = parse_gdv(&bytes).expect("gdv");

        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[1].codec_name, "gremlin_dpcm");
        assert_eq!(doc.streams[1].sample_rate, Some(21_168));
        assert_eq!(doc.streams[1].channels, Some(2));
    }
}
