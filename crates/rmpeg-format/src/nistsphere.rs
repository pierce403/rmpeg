use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Default)]
struct NistHeader {
    channels: Option<u16>,
    sample_rate: Option<u32>,
    sample_count: Option<u64>,
    sample_n_bytes: Option<u16>,
    codec_name: Option<&'static str>,
}

pub fn parse_nistsphere(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_nistsphere(bytes) {
        return Err(RmpegError::InvalidData(
            "missing NIST Sphere header".to_string(),
        ));
    }
    let header_size = parse_header_size(bytes)?;
    if header_size > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: header_size,
            remaining: bytes.len(),
        });
    }
    let header_text = std::str::from_utf8(&bytes[..header_size]).map_err(|_| {
        RmpegError::InvalidData("NIST Sphere header is not valid UTF-8".to_string())
    })?;
    let header = parse_header_fields(header_text)?;
    let channels = header
        .channels
        .ok_or_else(|| RmpegError::InvalidData("missing NIST channel count".to_string()))?;
    let sample_rate = header
        .sample_rate
        .ok_or_else(|| RmpegError::InvalidData("missing NIST sample rate".to_string()))?;
    let sample_count = header
        .sample_count
        .ok_or_else(|| RmpegError::InvalidData("missing NIST sample count".to_string()))?;
    let bits_per_sample = header
        .sample_n_bytes
        .ok_or_else(|| RmpegError::InvalidData("missing NIST sample byte count".to_string()))?
        .saturating_mul(8);
    let codec_name = header
        .codec_name
        .ok_or_else(|| RmpegError::InvalidData("unsupported NIST sample coding".to_string()))?;

    Ok(ProbeDocument {
        format: "nistsphere".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec_name,
            sample_rate,
            channels,
            bits_per_sample,
            sample_count as f64 / sample_rate as f64,
        )],
    })
}

pub fn looks_like_nistsphere(bytes: &[u8]) -> bool {
    bytes.starts_with(b"NIST_1A\n")
}

fn parse_header_size(bytes: &[u8]) -> Result<usize> {
    let Some(rest) = bytes.strip_prefix(b"NIST_1A\n") else {
        return Err(RmpegError::InvalidData(
            "missing NIST Sphere magic".to_string(),
        ));
    };
    let line_end = rest
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or_else(|| RmpegError::InvalidData("missing NIST header-size line".to_string()))?;
    let line = std::str::from_utf8(&rest[..line_end])
        .map_err(|_| RmpegError::InvalidData("NIST header size is not valid UTF-8".to_string()))?;
    line.trim()
        .parse()
        .map_err(|_| RmpegError::InvalidData("invalid NIST header size".to_string()))
}

fn parse_header_fields(text: &str) -> Result<NistHeader> {
    let mut header = NistHeader::default();
    for line in text.lines().skip(2) {
        if line == "end_head" {
            break;
        }
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        let Some(kind) = parts.next() else {
            continue;
        };
        let Some(value) = parts.next() else {
            continue;
        };
        match (key, kind) {
            ("channel_count", "-i") => header.channels = Some(parse_field(value, key)?),
            ("sample_rate", "-i") => header.sample_rate = Some(parse_field(value, key)?),
            ("sample_count", "-i") => header.sample_count = Some(parse_field(value, key)?),
            ("sample_n_bytes", _) => header.sample_n_bytes = Some(parse_field(value, key)?),
            ("sample_coding", _) if value == "ulaw" => header.codec_name = Some("pcm_mulaw"),
            _ => {}
        }
    }
    Ok(header)
}

fn parse_field<T: std::str::FromStr>(value: &str, key: &str) -> Result<T> {
    value
        .parse()
        .map_err(|_| RmpegError::InvalidData(format!("invalid NIST {key}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ulaw_sphere_header() {
        let mut bytes = b"NIST_1A\n   128\nchannel_count -i 1\nsample_rate -i 11025\nsample_coding -s4 ulaw\nsample_n_bytes -s1 1\nsample_count -i 18751\nend_head\n".to_vec();
        bytes.resize(128, b' ');

        let doc = parse_nistsphere(&bytes).expect("nist");

        assert_eq!(doc.format, "nistsphere");
        assert_eq!(doc.streams[0].codec_name, "pcm_mulaw");
        assert_eq!(doc.streams[0].sample_rate, Some(11_025));
        assert_eq!(doc.streams[0].channels, Some(1));
        assert_eq!(doc.streams[0].bits_per_sample, Some(8));
        assert_eq!(doc.streams[0].duration_seconds, Some(18_751.0 / 11_025.0));
    }
}
