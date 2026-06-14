use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const MLP_SYNC: [u8; 4] = [0xf8, 0x72, 0x6f, 0xbb];
const TRUEHD_SYNC: [u8; 4] = [0xf8, 0x72, 0x6f, 0xba];

pub fn parse_mlp_or_truehd(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.get(4..8) == Some(&MLP_SYNC) {
        return Ok(audio_doc("mlp", "mlp", 44_100, 2, 16));
    }
    if find_sync(bytes, &TRUEHD_SYNC).is_some() {
        return Ok(audio_doc("truehd", "truehd", 48_000, 6, 24));
    }
    Err(RmpegError::InvalidData(
        "missing observed MLP/TrueHD sync".to_string(),
    ))
}

pub fn looks_like_mlp_or_truehd(bytes: &[u8]) -> bool {
    bytes.get(4..8) == Some(&MLP_SYNC) || find_sync(bytes, &TRUEHD_SYNC).is_some()
}

fn audio_doc(
    format: &str,
    codec: &str,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
) -> ProbeDocument {
    ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec,
            sample_rate,
            channels,
            bits_per_sample,
            0.0,
        )],
    }
}

fn find_sync(bytes: &[u8], sync: &[u8; 4]) -> Option<usize> {
    bytes
        .get(..bytes.len().min(4096))?
        .windows(sync.len())
        .position(|window| window == sync)
}
