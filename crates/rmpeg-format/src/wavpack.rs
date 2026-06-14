use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_wavpack(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 32 || &bytes[0..4] != b"wvpk" {
        return Err(RmpegError::InvalidData(
            "missing WavPack block header".to_string(),
        ));
    }

    let total_samples = read_u32_le(bytes, 12)?;
    let flags = read_u32_le(bytes, 24)?;
    let mut format = parse_embedded_wave_format(bytes)
        .or_else(|| parse_embedded_dsdiff_format(bytes))
        .ok_or_else(|| RmpegError::InvalidData("missing WavPack source format".to_string()))?;
    let storage_bits = (((flags & 0x3) + 1) * 8) as u16;
    if format.bits_per_sample < storage_bits {
        format.bits_per_sample = storage_bits;
    }
    if format.sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "WavPack sample rate must be nonzero".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "wv".to_string(),
        streams: vec![StreamMetadata {
            index: 0,
            codec_type: "audio".to_string(),
            codec_name: "wavpack".to_string(),
            sample_rate: Some(format.sample_rate),
            channels: Some(format.channels),
            bits_per_sample: Some(format.bits_per_sample),
            duration_seconds: Some(f64::from(total_samples) / f64::from(format.sample_rate)),
            width: None,
            height: None,
            frame_rate: None,
        }],
    })
}

#[derive(Debug, Clone, Copy)]
struct SourceFormat {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
}

fn parse_embedded_wave_format(bytes: &[u8]) -> Option<SourceFormat> {
    for riff_pos in find_all(bytes, b"RIFF", 4096) {
        if riff_pos + 12 > bytes.len() || &bytes[riff_pos + 8..riff_pos + 12] != b"WAVE" {
            continue;
        }
        let mut pos = riff_pos + 12;
        while pos + 8 <= bytes.len() {
            let chunk_id = &bytes[pos..pos + 4];
            let chunk_size = read_u32_le(bytes, pos + 4).ok()? as usize;
            let data_start = pos + 8;
            let data_end = data_start.checked_add(chunk_size)?;
            if data_end > bytes.len() {
                break;
            }
            if chunk_id == b"fmt " && chunk_size >= 16 {
                let channels = read_u16_le(bytes, data_start + 2).ok()?;
                let sample_rate = read_u32_le(bytes, data_start + 4).ok()?;
                let bits_per_sample = read_u16_le(bytes, data_start + 14).ok()?;
                if channels > 0 && sample_rate > 0 && bits_per_sample > 0 {
                    return Some(SourceFormat {
                        sample_rate,
                        channels,
                        bits_per_sample,
                    });
                }
            }
            pos = data_end + (chunk_size % 2);
        }
    }
    None
}

fn parse_embedded_dsdiff_format(bytes: &[u8]) -> Option<SourceFormat> {
    let fs_pos = find_bytes(bytes, b"FS  ", 4096)?;
    let fs_size = read_u64_be(bytes, fs_pos + 4).ok()?;
    if fs_size < 4 {
        return None;
    }
    let dsd_sample_rate = read_u32_be(bytes, fs_pos + 12).ok()?;
    let sample_rate = dsd_sample_rate / 8;

    let channel_pos = find_bytes(bytes, b"CHNL", 4096)?;
    let channel_size = read_u64_be(bytes, channel_pos + 4).ok()?;
    if channel_size < 2 {
        return None;
    }
    let channels = read_u16_be(bytes, channel_pos + 12).ok()?;
    if sample_rate == 0 || channels == 0 {
        return None;
    }
    Some(SourceFormat {
        sample_rate,
        channels,
        bits_per_sample: 8,
    })
}

fn find_all<'a>(
    bytes: &'a [u8],
    needle: &'a [u8],
    limit: usize,
) -> impl Iterator<Item = usize> + 'a {
    let end = bytes.len().min(limit);
    (0..end.saturating_sub(needle.len()).saturating_add(1))
        .filter(move |&pos| &bytes[pos..pos + needle.len()] == needle)
}

fn find_bytes(bytes: &[u8], needle: &[u8], limit: usize) -> Option<usize> {
    find_all(bytes, needle, limit).next()
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(RmpegError::UnexpectedEof {
            needed: offset + 2,
            remaining: bytes.len(),
        })?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(RmpegError::UnexpectedEof {
            needed: offset + 2,
            remaining: bytes.len(),
        })?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(RmpegError::UnexpectedEof {
            needed: offset + 4,
            remaining: bytes.len(),
        })?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(RmpegError::UnexpectedEof {
            needed: offset + 4,
            remaining: bytes.len(),
        })?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64_be(bytes: &[u8], offset: usize) -> Result<u64> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(RmpegError::UnexpectedEof {
            needed: offset + 8,
            remaining: bytes.len(),
        })?;
    Ok(u64::from_be_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_embedded_wave_format() {
        let bytes = wavpack_with_riff_wave(44_100, 2, 16, 132_300);
        let doc = parse_wavpack(&bytes).expect("valid WavPack");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "wv");
        assert_eq!(stream.codec_name, "wavpack");
        assert_eq!(stream.sample_rate, Some(44_100));
        assert_eq!(stream.channels, Some(2));
        assert_eq!(stream.bits_per_sample, Some(16));
        assert_eq!(stream.duration_seconds, Some(3.0));
    }

    #[test]
    fn parses_embedded_dsdiff_format() {
        let mut bytes = wavpack_header(1_764_000);
        bytes.extend_from_slice(b"\x02dff\x00#AFRM8\x00\x00\x00\x00\x005\xd5\xb6DSD ");
        bytes.extend_from_slice(b"FVER\x00\x00\x00\x00\x00\x00\x00\x04\x01\x05\x00\x00");
        bytes.extend_from_slice(b"PROP\x00\x00\x00\x00\x00\x00\x00\x20SND ");
        bytes.extend_from_slice(b"FS  \x00\x00\x00\x00\x00\x00\x00\x04\x00+\x11\x00");
        bytes.extend_from_slice(b"CHNL\x00\x00\x00\x00\x00\x00\x00\x0a\x00\x02SLFTSRGT");

        let doc = parse_wavpack(&bytes).expect("valid DSD WavPack");
        let stream = &doc.streams[0];
        assert_eq!(stream.sample_rate, Some(352_800));
        assert_eq!(stream.channels, Some(2));
        assert_eq!(stream.bits_per_sample, Some(8));
        assert_eq!(stream.duration_seconds, Some(5.0));
    }

    fn wavpack_header(total_samples: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"wvpk");
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0x0410_u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&total_samples.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes
    }

    fn wavpack_with_riff_wave(
        sample_rate: u32,
        channels: u16,
        bits: u16,
        total_samples: u32,
    ) -> Vec<u8> {
        let mut bytes = wavpack_header(total_samples);
        bytes.extend_from_slice(&[0x21, 0x16]);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&36_u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        let byte_rate = sample_rate * u32::from(channels) * u32::from(bits) / 8;
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        let block_align = channels * (bits / 8);
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&bits.to_le_bytes());
        bytes
    }
}
