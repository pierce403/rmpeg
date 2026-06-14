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
            if &chunk_id == b"data" {
                data = Some((chunk_start, bytes.len().saturating_sub(chunk_start)));
                break;
            }
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

    validate_wav_format(fmt)?;

    let duration_seconds = duration_seconds(fmt, data_size)?;

    Ok(WavFile {
        metadata: AudioStreamMetadata {
            index: 0,
            codec_type: "audio".to_string(),
            codec_name: codec_name(fmt)?.to_string(),
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
    let mut fmt = WavFmt {
        audio_format: reader.read_u16_le()?,
        channels: reader.read_u16_le()?,
        sample_rate: reader.read_u32_le()?,
        byte_rate: reader.read_u32_le()?,
        block_align: reader.read_u16_le()?,
        bits_per_sample: reader.read_u16_le()?,
    };
    if fmt.audio_format == 0xfffe {
        fmt.audio_format = parse_extensible_subformat(bytes)?;
    }
    Ok(fmt)
}

fn validate_wav_format(fmt: WavFmt) -> Result<()> {
    if fmt.channels == 0 {
        return Err(RmpegError::Unsupported(format!(
            "WAV channel count {} is not supported",
            fmt.channels
        )));
    }
    if fmt.audio_format == 0x0200 {
        if fmt.channels != 1 || fmt.bits_per_sample != 4 || fmt.sample_rate == 0 {
            return Err(RmpegError::Unsupported(
                "Creative ADPCM WAV layout is not supported".to_string(),
            ));
        }
        return Ok(());
    }
    if fmt.audio_format != 1 {
        return Err(RmpegError::Unsupported(format!(
            "WAV audio format {} is not PCM",
            fmt.audio_format
        )));
    }
    if !matches!(fmt.bits_per_sample, 8 | 16 | 24 | 32) {
        return Err(RmpegError::Unsupported(format!(
            "WAV bits per sample {} is not supported PCM",
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

fn duration_seconds(fmt: WavFmt, data_size: usize) -> Result<f64> {
    if fmt.audio_format == 0x0200 {
        return Ok(data_size as f64 * 2.0 / fmt.sample_rate as f64);
    }
    let bytes_per_second = u32::from(fmt.block_align)
        .checked_mul(fmt.sample_rate)
        .ok_or_else(|| RmpegError::InvalidData("WAV byte rate overflow".to_string()))?;
    if bytes_per_second == 0 {
        Ok(0.0)
    } else {
        Ok(data_size as f64 / bytes_per_second as f64)
    }
}

fn codec_name(fmt: WavFmt) -> Result<&'static str> {
    if fmt.audio_format == 0x0200 {
        return Ok("adpcm_ct");
    }
    match fmt.bits_per_sample {
        8 => Ok("pcm_u8"),
        16 => Ok("pcm_s16le"),
        24 => Ok("pcm_s24le"),
        32 => Ok("pcm_s32le"),
        other => Err(RmpegError::Unsupported(format!(
            "WAV bits per sample {other} is not supported PCM"
        ))),
    }
}

fn parse_extensible_subformat(bytes: &[u8]) -> Result<u16> {
    if bytes.len() < 40 {
        return Err(RmpegError::UnexpectedEof {
            needed: 40,
            remaining: bytes.len(),
        });
    }
    let cb_size = u16::from_le_bytes([bytes[16], bytes[17]]);
    if cb_size < 22 {
        return Err(RmpegError::UnexpectedEof {
            needed: 40,
            remaining: bytes.len(),
        });
    }
    if bytes[26..40] != PCM_SUBFORMAT_GUID_TAIL {
        return Err(RmpegError::Unsupported(
            "WAV extensible subformat is not PCM".to_string(),
        ));
    }
    Ok(u16::from_le_bytes([bytes[24], bytes[25]]))
}

const PCM_SUBFORMAT_GUID_TAIL: [u8; 14] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];

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

    fn extensible_pcm_wav_24bit() -> Vec<u8> {
        let channels = 2_u16;
        let sample_rate = 96_000_u32;
        let bits_per_sample = 24_u16;
        let block_align = channels * 3;
        let byte_rate = sample_rate * u32::from(block_align);
        let data = [0_u8; 6];
        let fmt_size = 40_u32;
        let riff_size = 4 + (8 + fmt_size) + (8 + data.len() as u32);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&riff_size.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&fmt_size.to_le_bytes());
        bytes.extend_from_slice(&0xfffe_u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
        bytes.extend_from_slice(&22_u16.to_le_bytes());
        bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&PCM_SUBFORMAT_GUID_TAIL);
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&data);
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

    #[test]
    fn parses_extensible_pcm_subformat() {
        let wav = parse_wav(&extensible_pcm_wav_24bit()).expect("valid extensible pcm wav");
        assert_eq!(wav.metadata.codec_name, "pcm_s24le");
        assert_eq!(wav.metadata.channels, 2);
        assert_eq!(wav.metadata.sample_rate, 96_000);
        assert_eq!(wav.metadata.bits_per_sample, 24);
        assert_eq!(wav.data_size, 6);
    }

    #[test]
    fn parses_truncated_creative_adpcm_data_chunk() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0x1000_u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&20_u32.to_le_bytes());
        bytes.extend_from_slice(&0x0200_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&44_100_u32.to_le_bytes());
        bytes.extend_from_slice(&22_050_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&4_u16.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&100_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 10]);

        let wav = parse_wav(&bytes).expect("valid truncated creative adpcm wav");
        assert_eq!(wav.metadata.codec_name, "adpcm_ct");
        assert_eq!(wav.data_size, 10);
        assert_eq!(wav.metadata.duration_seconds, 20.0 / 44_100.0);
    }
}
