use rmpeg_core::{AudioFrameHash, Result, RmpegError};
use rmpeg_format::WavFile;

use crate::md5::md5_hex;

pub fn pcm_s16le_frame_hashes(
    input: &[u8],
    wav: &WavFile,
    samples_per_frame: u32,
) -> Result<Vec<AudioFrameHash>> {
    if samples_per_frame == 0 {
        return Err(RmpegError::InvalidData(
            "samples_per_frame must be greater than zero".to_string(),
        ));
    }

    let block_align = usize::from(wav.metadata.block_align);
    if block_align == 0 {
        return Err(RmpegError::InvalidData("zero WAV block_align".to_string()));
    }

    let end = wav
        .data_offset
        .checked_add(wav.data_size)
        .ok_or_else(|| RmpegError::InvalidData("WAV data range overflow".to_string()))?;
    if end > input.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: input.len(),
        });
    }

    let mut frames = Vec::new();
    let mut offset = wav.data_offset;
    let mut pts = 0_u64;
    while offset < end {
        let remaining_bytes = end - offset;
        let remaining_samples = remaining_bytes / block_align;
        if remaining_samples == 0 {
            break;
        }
        let duration = remaining_samples.min(samples_per_frame as usize) as u32;
        let size = duration as usize * block_align;
        let frame = &input[offset..offset + size];
        frames.push(AudioFrameHash {
            stream_index: 0,
            dts: pts,
            pts,
            duration,
            size,
            hash: md5_hex(frame),
        });
        offset += size;
        pts += u64::from(duration);
    }

    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::pcm_s16le_frame_hashes;
    use rmpeg_format::parse_wav;

    fn silent_wav(sample_count: usize) -> Vec<u8> {
        let data_size = sample_count * 2;
        let riff_size = 4 + 24 + 8 + data_size;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(riff_size as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&8000_u32.to_le_bytes());
        bytes.extend_from_slice(&16000_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data_size as u32).to_le_bytes());
        bytes.resize(bytes.len() + data_size, 0);
        bytes
    }

    #[test]
    fn chunks_like_ffmpeg_framemd5_for_pcm() {
        let bytes = silent_wav(1100);
        let wav = parse_wav(&bytes).expect("valid wav");
        let frames = pcm_s16le_frame_hashes(&bytes, &wav, 1024).expect("hashes");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].duration, 1024);
        assert_eq!(frames[0].size, 2048);
        assert_eq!(frames[0].hash, "c99a74c555371a433d121f551d6c6398");
        assert_eq!(frames[1].duration, 76);
    }
}
