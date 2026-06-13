use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

use crate::{mp3::parse_mp3, mp4::parse_mp4, wav::parse_wav};

pub fn probe(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.starts_with(b"RIFF") {
        let wav = parse_wav(bytes)?;
        return Ok(ProbeDocument {
            format: "wav".to_string(),
            streams: vec![StreamMetadata::audio(
                wav.metadata.index,
                wav.metadata.codec_name,
                wav.metadata.sample_rate,
                wav.metadata.channels,
                wav.metadata.bits_per_sample,
                wav.metadata.duration_seconds,
            )],
        });
    }

    if bytes.starts_with(b"ID3") || looks_like_mp3_frame(bytes) {
        return parse_mp3(bytes);
    }

    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return parse_mp4(bytes);
    }

    Err(RmpegError::InvalidData(
        "unsupported or unrecognized media format".to_string(),
    ))
}

fn looks_like_mp3_frame(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0xff && (bytes[1] & 0xe0) == 0xe0
}
