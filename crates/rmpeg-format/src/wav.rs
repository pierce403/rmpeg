use rmpeg_core::{io::ByteReader, AudioStreamMetadata, Result, RmpegError};

#[derive(Debug, Clone, PartialEq)]
pub struct WavFile {
    pub metadata: AudioStreamMetadata,
    pub data_offset: usize,
    pub data_size: usize,
}

#[derive(Debug, Clone, Copy)]
struct WavFmt {
    audio_format: u16,
    channels: u16,
    sample_rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits_per_sample: u16,
}

pub fn parse_wav(bytes: &[u8]) -> Result<WavFile> {
    let mut reader = ByteReader::new(bytes);
    let riff = reader.read_fourcc()?;
    if &riff != b"RIFF" {
        return Err(RmpegError::InvalidData("missing RIFF header".to_string()));
    }

    let _riff_size = reader.read_u32_le()?;
    let wave = reader.read_fourcc()?;
    if &wave != b"WAVE" {
        return Err(RmpegError::InvalidData("missing WAVE header".to_string()));
    }

    let mut fmt = None;
    let mut data = None;

    while reader.remaining() >= 8 {
        let chunk_id = reader.read_fourcc()?;
        let chunk_size = reader.read_u32_le()? as usize;
        let chunk_start = reader.position();
        let chunk_end = chunk_start
            .checked_add(chunk_size)
            .ok_or_else(|| RmpegError::InvalidData("WAV chunk size overflow".to_string()))?;
        if chunk_end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: chunk_end,
                remaining: bytes.len(),
            });
        }

        match &chunk_id {
            b"fmt " => fmt = Some(parse_fmt(&bytes[chunk_start..chunk_end])?),
            b"data" => data = Some((chunk_start, chunk_size)),
            _ => {}
        }

        let padded_end = chunk_end + (chunk_size % 2);
        reader.seek(padded_end.min(bytes.len()))?;
    }

    let fmt = fmt.ok_or_else(|| RmpegError::InvalidData("missing fmt chunk".to_string()))?;
    let (data_offset, data_size) =
        data.ok_or_else(|| RmpegError::InvalidData("missing data chunk".to_string()))?;

    validate_pcm_s16le(fmt)?;

    let bytes_per_second = u32::from(fmt.block_align)
        .checked_mul(fmt.sample_rate)
        .ok_or_else(|| RmpegError::InvalidData("WAV byte rate overflow".to_string()))?;
    let duration_seconds = if bytes_per_second == 0 {
        0.0
    } else {
        data_size as f64 / bytes_per_second as f64
    };

    Ok(WavFile {
        metadata: AudioStreamMetadata {
            index: 0,
            codec_type: "audio".to_string(),
            codec_name: "pcm_s16le".to_string(),
            sample_rate: fmt.sample_rate,
            channels: fmt.channels,
            bits_per_sample: fmt.bits_per_sample,
            duration_seconds,
            data_size: data_size as u32,
            block_align: fmt.block_align,
        },
        data_offset,
        data_size,
    })
}

fn parse_fmt(bytes: &[u8]) -> Result<WavFmt> {
    if bytes.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: bytes.len(),
        });
    }

    let mut reader = ByteReader::new(bytes);
    Ok(WavFmt {
        audio_format: reader.read_u16_le()?,
        channels: reader.read_u16_le()?,
        sample_rate: reader.read_u32_le()?,
        byte_rate: reader.read_u32_le()?,
        block_align: reader.read_u16_le()?,
        bits_per_sample: reader.read_u16_le()?,
    })
}

fn validate_pcm_s16le(fmt: WavFmt) -> Result<()> {
    if fmt.audio_format != 1 {
        return Err(RmpegError::Unsupported(format!(
            "WAV audio format {} is not PCM",
            fmt.audio_format
        )));
    }
    if fmt.channels != 1 && fmt.channels != 2 {
        return Err(RmpegError::Unsupported(format!(
            "WAV channel count {} is not supported",
            fmt.channels
        )));
    }
    if fmt.bits_per_sample != 16 {
        return Err(RmpegError::Unsupported(format!(
            "WAV bits per sample {} is not pcm_s16le",
            fmt.bits_per_sample
        )));
    }
    let expected_block_align = fmt
        .channels
        .checked_mul(fmt.bits_per_sample / 8)
        .ok_or_else(|| RmpegError::InvalidData("WAV block align overflow".to_string()))?;
    if fmt.block_align != expected_block_align {
        return Err(RmpegError::InvalidData(format!(
            "WAV block_align {} does not match expected {}",
            fmt.block_align, expected_block_align
        )));
    }
    let expected_byte_rate = fmt
        .sample_rate
        .checked_mul(u32::from(fmt.block_align))
        .ok_or_else(|| RmpegError::InvalidData("WAV byte rate overflow".to_string()))?;
    if fmt.byte_rate != expected_byte_rate {
        return Err(RmpegError::InvalidData(format!(
            "WAV byte_rate {} does not match expected {}",
            fmt.byte_rate, expected_byte_rate
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_wav(samples: &[i16], channels: u16, sample_rate: u32) -> Vec<u8> {
        let mut pcm = Vec::new();
        for sample in samples {
            pcm.extend_from_slice(&sample.to_le_bytes());
        }
        let fmt_size = 16_u32;
        let data_size = pcm.len() as u32;
        let block_align = channels * 2;
        let byte_rate = sample_rate * u32::from(block_align);
        let riff_size = 4 + (8 + fmt_size) + (8 + data_size);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&riff_size.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&fmt_size.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_size.to_le_bytes());
        bytes.extend_from_slice(&pcm);
        bytes
    }

    #[test]
    fn parses_minimal_pcm_s16le() {
        let bytes = minimal_wav(&[0, 1, -1, 2], 2, 44_100);
        let wav = parse_wav(&bytes).expect("valid wav");
        assert_eq!(wav.metadata.codec_name, "pcm_s16le");
        assert_eq!(wav.metadata.channels, 2);
        assert_eq!(wav.metadata.sample_rate, 44_100);
        assert_eq!(wav.data_size, 8);
    }

    #[test]
    fn rejects_truncated_riff() {
        let err =
            parse_wav(b"RIFF\x10\x00\x00\x00WAVEfmt \x10\x00\x00\x00").expect_err("truncated wav");
        assert!(err.to_string().contains("unexpected end"));
    }
}
