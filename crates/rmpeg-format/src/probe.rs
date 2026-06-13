use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

use crate::{
    aac::{looks_like_adts_aac, parse_adts_aac},
    flac::parse_flac,
    mp3::parse_mp3,
    mp4::parse_mp4,
    ogg::parse_ogg,
    wav::parse_wav,
};

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

    if looks_like_adts_aac(bytes) {
        return parse_adts_aac(bytes);
    }

    if bytes.starts_with(b"ID3") || looks_like_mp3_frame(bytes) {
        return parse_mp3(bytes);
    }

    if bytes.starts_with(b"fLaC") {
        return parse_flac(bytes);
    }

    if bytes.starts_with(b"OggS") {
        return parse_ogg(bytes);
    }

    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return parse_mp4(bytes);
    }

    Err(RmpegError::InvalidData(
        "unsupported or unrecognized media format".to_string(),
    ))
}

fn looks_like_mp3_frame(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || bytes[0] != 0xff || (bytes[1] & 0xe0) != 0xe0 {
        return false;
    }
    let version_id = (bytes[1] >> 3) & 0b11;
    let layer = (bytes[1] >> 1) & 0b11;
    let bitrate_index = (bytes[2] >> 4) & 0b1111;
    let sample_rate_index = (bytes[2] >> 2) & 0b11;
    version_id != 0b01
        && layer == 0b01
        && bitrate_index != 0
        && bitrate_index != 15
        && sample_rate_index != 0b11
}
