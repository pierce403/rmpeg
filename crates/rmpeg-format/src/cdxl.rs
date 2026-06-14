use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const CDXL_AUDIO_SAMPLE_RATE: u32 = 11_025;

pub fn parse_cdxl(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 32 {
        return Err(RmpegError::UnexpectedEof {
            needed: 32,
            remaining: bytes.len(),
        });
    }

    let frame_size = usize::from(read_u16_be(bytes, 4)?);
    let width = u32::from(read_u16_be(bytes, 14)?);
    let height = u32::from(read_u16_be(bytes, 16)?);
    let audio_samples_per_frame = u32::from(read_u16_be(bytes, 22)?);

    if frame_size < 32 {
        return Err(RmpegError::InvalidData(format!(
            "CDXL frame size {frame_size} is too small"
        )));
    }
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "CDXL dimensions must be nonzero".to_string(),
        ));
    }

    let frame_count = bytes.len() / frame_size;
    if frame_count == 0 {
        return Err(RmpegError::InvalidData(
            "CDXL file has no complete frames".to_string(),
        ));
    }

    let mut streams = vec![StreamMetadata::video(
        0,
        "cdxl",
        width,
        height,
        Some(0.0),
        None,
    )];
    if audio_samples_per_frame > 0 {
        let duration_seconds =
            audio_samples_per_frame as f64 * frame_count as f64 / CDXL_AUDIO_SAMPLE_RATE as f64;
        streams.push(StreamMetadata::audio(
            1,
            "pcm_s8_planar",
            CDXL_AUDIO_SAMPLE_RATE,
            1,
            8,
            duration_seconds,
        ));
    }

    Ok(ProbeDocument {
        format: "cdxl".to_string(),
        streams,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cdxl_header(frame_size: u16, width: u16, height: u16, audio_samples: u16) -> Vec<u8> {
        let mut bytes = vec![0; usize::from(frame_size) * 2];
        bytes[0] = 1;
        bytes[4..6].copy_from_slice(&frame_size.to_be_bytes());
        bytes[14..16].copy_from_slice(&width.to_be_bytes());
        bytes[16..18].copy_from_slice(&height.to_be_bytes());
        bytes[22..24].copy_from_slice(&audio_samples.to_be_bytes());
        bytes
    }

    #[test]
    fn parses_video_only_cdxl() {
        let doc = parse_cdxl(&cdxl_header(40, 160, 120, 0)).expect("valid cdxl");
        assert_eq!(doc.format, "cdxl");
        assert_eq!(doc.streams.len(), 1);
        assert_eq!(doc.streams[0].codec_name, "cdxl");
        assert_eq!(doc.streams[0].width, Some(160));
        assert_eq!(doc.streams[0].height, Some(120));
    }

    #[test]
    fn reports_audio_duration_from_complete_frames() {
        let doc = parse_cdxl(&cdxl_header(40, 160, 120, 1_102)).expect("valid cdxl");
        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[1].codec_name, "pcm_s8_planar");
        assert_eq!(doc.streams[1].duration_seconds, Some(2_204.0 / 11_025.0));
    }
}
