use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_rpl(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_rpl(bytes) {
        return Err(RmpegError::InvalidData("missing RPL header".to_string()));
    }

    let header_len = bytes.len().min(2048);
    let header = String::from_utf8_lossy(&bytes[..header_len]);
    let lines: Vec<&str> = header.lines().collect();
    if lines.len() < 15 {
        return Err(RmpegError::UnexpectedEof {
            needed: 15,
            remaining: lines.len(),
        });
    }

    let video_format = parse_line_u32(lines[4])?;
    let width = parse_line_u32(lines[5])?;
    let height = parse_line_u32(lines[6])?;
    let fps = parse_line_f64(lines[8])?;
    let audio_sample_rate = parse_line_u32(lines[10])?;
    let channels = parse_line_u16(lines[11])?;
    let bits_per_sample = parse_line_u16(lines[12])?;
    let frames_per_chunk = parse_line_u32(lines[13])?;
    let chunks = parse_line_u32(lines[14])?;
    if video_format == 0
        || width == 0
        || height == 0
        || fps == 0.0
        || audio_sample_rate == 0
        || channels == 0
        || frames_per_chunk == 0
    {
        return Err(RmpegError::InvalidData(
            "invalid RPL stream metadata".to_string(),
        ));
    }

    let video_codec = match video_format {
        124 => "escape124",
        130 => "escape130",
        _ => {
            return Err(RmpegError::InvalidData(format!(
                "unsupported RPL video format {video_format}"
            )));
        }
    };
    let audio_codec = match bits_per_sample {
        8 => "pcm_u8",
        16 => "pcm_s16le",
        _ => {
            return Err(RmpegError::InvalidData(format!(
                "unsupported RPL audio bit depth {bits_per_sample}"
            )));
        }
    };

    Ok(ProbeDocument {
        format: "rpl".to_string(),
        streams: vec![
            StreamMetadata::video(
                0,
                video_codec,
                width,
                height,
                Some((chunks + 1) as f64 * frames_per_chunk as f64 / fps),
                None,
            ),
            StreamMetadata::audio(
                1,
                audio_codec,
                audio_sample_rate,
                channels,
                bits_per_sample,
                0.0,
            ),
        ],
    })
}

pub fn looks_like_rpl(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ARMovie\n")
}

fn parse_line_u32(line: &str) -> Result<u32> {
    parse_line_token(line)?
        .parse::<u32>()
        .map_err(|_| RmpegError::InvalidData(format!("invalid RPL integer line {line:?}")))
}

fn parse_line_u16(line: &str) -> Result<u16> {
    let value = parse_line_u32(line)?;
    u16::try_from(value)
        .map_err(|_| RmpegError::InvalidData(format!("RPL integer does not fit u16: {value}")))
}

fn parse_line_f64(line: &str) -> Result<f64> {
    parse_line_token(line)?
        .parse::<f64>()
        .map_err(|_| RmpegError::InvalidData(format!("invalid RPL float line {line:?}")))
}

fn parse_line_token(line: &str) -> Result<&str> {
    line.split_whitespace()
        .next()
        .ok_or_else(|| RmpegError::InvalidData("empty RPL metadata line".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_escape_124_header() {
        let header = b"ARMovie\npath\ncopyright\nESCAPE 1.0\n124 video format\n320 pixels\n240 pixels\n16 bits\n25.000000 fps\n101 sound\n44100 Hz\n2 channels\n8 bits\n25 frames per chunk\n3 chunks\n";

        let doc = parse_rpl(header).expect("rpl");

        assert_eq!(doc.format, "rpl");
        assert_eq!(doc.streams[0].codec_name, "escape124");
        assert_eq!(doc.streams[0].duration_seconds, Some(4.0));
        assert_eq!(doc.streams[1].codec_name, "pcm_u8");
        assert_eq!(doc.streams[1].duration_seconds, Some(0.0));
    }

    #[test]
    fn rejects_unknown_video_format() {
        let header = b"ARMovie\npath\ncopyright\nESCAPE 1.0\n999 video format\n320 pixels\n240 pixels\n16 bits\n25.000000 fps\n101 sound\n44100 Hz\n2 channels\n8 bits\n25 frames per chunk\n3 chunks\n";

        assert!(parse_rpl(header).is_err());
    }
}
