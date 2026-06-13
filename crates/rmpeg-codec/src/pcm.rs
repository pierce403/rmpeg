use rmpeg_core::{AudioFrameHash, Result, RmpegError};
use rmpeg_format::WavFile;

use crate::md5::md5_hex;

pub fn pcm_frame_hashes(input: &[u8], wav: &WavFile) -> Result<Vec<AudioFrameHash>> {
    let samples_per_frame = wav_framemd5_samples_per_frame(wav.metadata.sample_rate)?;
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
        let input_size = duration as usize * block_align;
        let frame = decoded_frame_bytes(&input[offset..offset + input_size], wav)?;
        frames.push(AudioFrameHash {
            stream_index: 0,
            dts: pts,
            pts,
            duration,
            size: frame.len(),
            hash: md5_hex(&frame),
        });
        offset += input_size;
        pts += u64::from(duration);
    }

    Ok(frames)
}

pub fn wav_framemd5_samples_per_frame(sample_rate: u32) -> Result<u32> {
    let target = sample_rate / 10;
    if target == 0 {
        return Err(RmpegError::InvalidData(
            "sample_rate is too small for WAV packetization".to_string(),
        ));
    }
    let mut samples = 1_u32;
    while samples <= target / 2 {
        samples *= 2;
    }
    Ok(samples)
}

fn decoded_frame_bytes(input: &[u8], wav: &WavFile) -> Result<Vec<u8>> {
    match wav.metadata.bits_per_sample {
        16 => Ok(input.to_vec()),
        8 => {
            let mut output = Vec::with_capacity(input.len() * 2);
            for byte in input {
                let sample = (i16::from(*byte) - 128) << 8;
                output.extend_from_slice(&sample.to_le_bytes());
            }
            Ok(output)
        }
        other => Err(RmpegError::Unsupported(format!(
            "WAV bits per sample {other} is not supported PCM"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{pcm_frame_hashes, wav_framemd5_samples_per_frame};
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
        let frames = pcm_frame_hashes(&bytes, &wav).expect("hashes");
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].duration, 512);
        assert_eq!(frames[0].size, 1024);
        assert_eq!(frames[0].hash, "0f343b0931126a20f133d67c2b018a3b");
        assert_eq!(frames[1].duration, 512);
        assert_eq!(frames[2].duration, 76);
    }

    #[test]
    fn packet_size_tracks_observed_ffmpeg_wav_behavior() {
        assert_eq!(wav_framemd5_samples_per_frame(8000).unwrap(), 512);
        assert_eq!(wav_framemd5_samples_per_frame(22050).unwrap(), 2048);
        assert_eq!(wav_framemd5_samples_per_frame(44100).unwrap(), 4096);
        assert_eq!(wav_framemd5_samples_per_frame(48000).unwrap(), 4096);
    }
}
