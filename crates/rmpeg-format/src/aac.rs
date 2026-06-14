use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AacAudioConfig {
    pub codec_name: &'static str,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub bits_per_sample: Option<u16>,
}

#[derive(Debug, Clone, Copy)]
struct AdtsFrame {
    frame_len: usize,
    sample_rate: u32,
    channels: u16,
    samples: u32,
}

pub fn parse_adts_aac(bytes: &[u8]) -> Result<ProbeDocument> {
    let mut pos = id3v2_skip(bytes)?;
    let mut first = None;
    let mut samples = 0_u64;

    while pos + 7 <= bytes.len() {
        if bytes.get(pos..pos + 3) == Some(b"ID3") {
            pos += id3v2_skip(&bytes[pos..])?;
            continue;
        }
        let frame = parse_adts_header(&bytes[pos..pos + 7])
            .ok_or_else(|| RmpegError::InvalidData("invalid ADTS AAC frame".to_string()))?;
        if pos + frame.frame_len > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + frame.frame_len,
                remaining: bytes.len(),
            });
        }
        first.get_or_insert(frame);
        samples += u64::from(frame.samples);
        pos += frame.frame_len;
    }

    let first =
        first.ok_or_else(|| RmpegError::InvalidData("no ADTS AAC frames found".to_string()))?;
    let duration_seconds = samples as f64 / first.sample_rate as f64;
    Ok(ProbeDocument {
        format: "aac".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "aac",
            first.sample_rate,
            first.channels,
            0,
            duration_seconds,
        )],
    })
}

pub fn looks_like_adts_aac(bytes: &[u8]) -> bool {
    let Ok(pos) = id3v2_skip(bytes) else {
        return false;
    };
    pos + 7 <= bytes.len() && parse_adts_header(&bytes[pos..pos + 7]).is_some()
}

fn parse_adts_header(header: &[u8]) -> Option<AdtsFrame> {
    if header.len() < 7 {
        return None;
    }
    if header[0] != 0xff || (header[1] & 0xf0) != 0xf0 || (header[1] & 0x06) != 0 {
        return None;
    }
    let sample_rate = sample_rate(usize::from((header[2] & 0x3c) >> 2))?;
    let channel_config = ((header[2] & 0x01) << 2) | ((header[3] & 0xc0) >> 6);
    let channels = channels(channel_config)?;
    let frame_len = ((usize::from(header[3] & 0x03)) << 11)
        | (usize::from(header[4]) << 3)
        | usize::from((header[5] & 0xe0) >> 5);
    let header_len = if header[1] & 0x01 != 0 { 7 } else { 9 };
    if frame_len < header_len {
        return None;
    }
    let samples = (u32::from(header[6] & 0x03) + 1) * 1024;
    Some(AdtsFrame {
        frame_len,
        sample_rate,
        channels,
        samples,
    })
}

pub fn parse_audio_specific_config(bytes: &[u8]) -> Option<AacAudioConfig> {
    let mut bits = AscBitReader::new(bytes);
    let object_type = read_audio_object_type(&mut bits)?;
    let mut sample_rate = read_sampling_frequency(&mut bits)?;
    let channel_config = bits.read_bits(4)? as u8;
    let mut channels = channels(channel_config);
    let mut sbr_present = false;

    if matches!(object_type, 5 | 29) {
        sbr_present = true;
        sample_rate = read_sampling_frequency(&mut bits)?;
        read_audio_object_type(&mut bits)?;
        if object_type == 29 {
            channels = Some(2);
        }
    } else if object_type == 36 {
        if let Some(als) = parse_als_wave_metadata(bytes) {
            sample_rate = als.sample_rate.or(sample_rate);
            channels = als.channels.or(channels);
            return Some(AacAudioConfig {
                codec_name: "mp4als",
                sample_rate,
                channels,
                bits_per_sample: als.bits_per_sample,
            });
        }
    } else {
        parse_ga_specific_config(&mut bits, channel_config, &mut channels);
        if let Some(extension) = find_sync_extension(&mut bits) {
            sbr_present = true;
            sample_rate = Some(extension.sample_rate);
            if extension.ps_present || channels == Some(1) {
                channels = Some(2);
            }
        }
    }

    if sbr_present && channels == Some(1) {
        channels = Some(2);
    }

    Some(AacAudioConfig {
        codec_name: "aac",
        sample_rate,
        channels,
        bits_per_sample: None,
    })
}

fn sample_rate(index: usize) -> Option<u32> {
    [
        96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025,
        8_000, 7_350,
    ]
    .get(index)
    .copied()
}

fn channels(config: u8) -> Option<u16> {
    match config {
        1 => Some(1),
        2 => Some(2),
        3 => Some(3),
        4 => Some(4),
        5 => Some(5),
        6 => Some(6),
        7 => Some(8),
        _ => None,
    }
}

fn parse_ga_specific_config(
    bits: &mut AscBitReader<'_>,
    channel_config: u8,
    channels: &mut Option<u16>,
) {
    if bits.read_bits(1).is_none() {
        return;
    }
    if bits.read_bits(1) == Some(1) && bits.skip_bits(14).is_none() {
        return;
    }
    if bits.read_bits(1).is_none() {
        return;
    }
    if channel_config == 0 {
        *channels = parse_program_config_element(bits);
    }
}

fn parse_program_config_element(bits: &mut AscBitReader<'_>) -> Option<u16> {
    bits.skip_bits(4)?;
    bits.skip_bits(2)?;
    bits.skip_bits(4)?;
    let front = bits.read_bits(4)? as usize;
    let side = bits.read_bits(4)? as usize;
    let back = bits.read_bits(4)? as usize;
    let lfe = bits.read_bits(2)? as usize;
    let assoc_data = bits.read_bits(3)? as usize;
    let cc = bits.read_bits(4)? as usize;

    if bits.read_bits(1)? != 0 {
        bits.skip_bits(4)?;
    }
    if bits.read_bits(1)? != 0 {
        bits.skip_bits(4)?;
    }
    if bits.read_bits(1)? != 0 {
        bits.skip_bits(3)?;
    }

    let mut channels = 0_u16;
    for _ in 0..front {
        channels = channels.saturating_add(read_pce_channel_element(bits)?);
    }
    for _ in 0..side {
        channels = channels.saturating_add(read_pce_channel_element(bits)?);
    }
    for _ in 0..back {
        channels = channels.saturating_add(read_pce_channel_element(bits)?);
    }
    for _ in 0..lfe {
        bits.skip_bits(4)?;
        channels = channels.saturating_add(1);
    }
    for _ in 0..assoc_data {
        bits.skip_bits(4)?;
    }
    for _ in 0..cc {
        bits.skip_bits(5)?;
    }

    bits.align_to_byte();
    let comment_len = bits.read_bits(8)? as usize;
    bits.skip_bits(comment_len.checked_mul(8)?)?;
    Some(channels)
}

fn read_pce_channel_element(bits: &mut AscBitReader<'_>) -> Option<u16> {
    let is_cpe = bits.read_bits(1)? != 0;
    bits.skip_bits(4)?;
    Some(if is_cpe { 2 } else { 1 })
}

#[derive(Debug, Clone, Copy)]
struct SyncExtension {
    sample_rate: u32,
    ps_present: bool,
}

fn find_sync_extension(bits: &mut AscBitReader<'_>) -> Option<SyncExtension> {
    while bits.remaining_bits() >= 17 {
        let pos = bits.position();
        if bits.read_bits(11)? == 0x2b7 {
            let object_type = read_audio_object_type(bits)?;
            if object_type == 5 && bits.read_bits(1)? != 0 {
                let sample_rate = read_sampling_frequency(bits)??;
                let mut ps_present = false;
                if bits.remaining_bits() >= 12 {
                    let ps_pos = bits.position();
                    if bits.read_bits(11)? == 0x548 {
                        ps_present = bits.read_bits(1)? != 0;
                    } else {
                        bits.set_position(ps_pos)?;
                    }
                }
                return Some(SyncExtension {
                    sample_rate,
                    ps_present,
                });
            }
        }
        bits.set_position(pos + 1)?;
    }
    None
}

fn read_audio_object_type(bits: &mut AscBitReader<'_>) -> Option<u8> {
    let object_type = bits.read_bits(5)? as u8;
    if object_type == 31 {
        Some(32 + bits.read_bits(6)? as u8)
    } else {
        Some(object_type)
    }
}

fn read_sampling_frequency(bits: &mut AscBitReader<'_>) -> Option<Option<u32>> {
    let index = bits.read_bits(4)? as usize;
    if index == 15 {
        Some(Some(bits.read_bits(24)?))
    } else {
        Some(sample_rate(index))
    }
}

#[derive(Debug, Clone, Copy)]
struct AlsWaveMetadata {
    sample_rate: Option<u32>,
    channels: Option<u16>,
    bits_per_sample: Option<u16>,
}

fn parse_als_wave_metadata(bytes: &[u8]) -> Option<AlsWaveMetadata> {
    let riff = bytes.windows(4).position(|window| window == b"RIFF")?;
    let mut pos = riff.checked_add(12)?;
    while pos + 8 <= bytes.len() {
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let data_start = pos + 8;
        let data_end = data_start.checked_add(size)?;
        if data_end > bytes.len() {
            return None;
        }
        if &bytes[pos..pos + 4] == b"fmt " && size >= 16 {
            let channels = u16::from_le_bytes([bytes[data_start + 2], bytes[data_start + 3]]);
            let sample_rate = u32::from_le_bytes([
                bytes[data_start + 4],
                bytes[data_start + 5],
                bytes[data_start + 6],
                bytes[data_start + 7],
            ]);
            let bits_per_sample =
                u16::from_le_bytes([bytes[data_start + 14], bytes[data_start + 15]]);
            return Some(AlsWaveMetadata {
                sample_rate: Some(sample_rate),
                channels: Some(channels),
                bits_per_sample: Some(bits_per_sample),
            });
        }
        pos = data_end + (size & 1);
    }
    None
}

#[derive(Debug, Clone)]
struct AscBitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
}

impl<'a> AscBitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit_pos: 0 }
    }

    fn read_bits(&mut self, count: usize) -> Option<u32> {
        if count > 32 || self.remaining_bits() < count {
            return None;
        }
        let mut value = 0_u32;
        for _ in 0..count {
            let byte = self.bytes[self.bit_pos / 8];
            let bit = (byte >> (7 - (self.bit_pos % 8))) & 1;
            value = (value << 1) | u32::from(bit);
            self.bit_pos += 1;
        }
        Some(value)
    }

    fn skip_bits(&mut self, count: usize) -> Option<()> {
        if self.remaining_bits() < count {
            None
        } else {
            self.bit_pos += count;
            Some(())
        }
    }

    fn align_to_byte(&mut self) {
        self.bit_pos += (8 - (self.bit_pos % 8)) % 8;
    }

    fn position(&self) -> usize {
        self.bit_pos
    }

    fn set_position(&mut self, bit_pos: usize) -> Option<()> {
        if bit_pos <= self.bytes.len() * 8 {
            self.bit_pos = bit_pos;
            Some(())
        } else {
            None
        }
    }

    fn remaining_bits(&self) -> usize {
        self.bytes.len() * 8 - self.bit_pos
    }
}

fn id3v2_skip(bytes: &[u8]) -> Result<usize> {
    let mut pos = 0;
    while bytes.get(pos..pos + 3) == Some(b"ID3") {
        if bytes.len().saturating_sub(pos) < 10 {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 10,
                remaining: bytes.len(),
            });
        }
        if bytes[pos + 6..pos + 10].iter().any(|byte| byte & 0x80 != 0) {
            return Err(RmpegError::InvalidData(
                "invalid ID3 synchsafe size".to_string(),
            ));
        }
        let size = ((usize::from(bytes[pos + 6])) << 21)
            | ((usize::from(bytes[pos + 7])) << 14)
            | ((usize::from(bytes[pos + 8])) << 7)
            | usize::from(bytes[pos + 9]);
        let footer = if bytes[pos + 5] & 0x10 != 0 { 10 } else { 0 };
        pos = pos
            .checked_add(10 + size + footer)
            .ok_or_else(|| RmpegError::InvalidData("ID3 size overflow".to_string()))?;
    }
    Ok(pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADTS_LC_44K_STEREO_EMPTY_FRAME: [u8; 7] = [0xff, 0xf1, 0x50, 0x80, 0x00, 0xff, 0xfc];

    #[test]
    fn parses_minimal_adts_frame() {
        let doc = parse_adts_aac(&ADTS_LC_44K_STEREO_EMPTY_FRAME).expect("valid adts");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "aac");
        assert_eq!(stream.codec_name, "aac");
        assert_eq!(stream.sample_rate, Some(44_100));
        assert_eq!(stream.channels, Some(2));
        assert!(
            (stream.duration_seconds.expect("duration") - (1024.0 / 44_100.0)).abs() < f64::EPSILON
        );
    }

    #[test]
    fn skips_id3v2_tag_before_adts() {
        let mut bytes = b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec();
        bytes.extend_from_slice(&ADTS_LC_44K_STEREO_EMPTY_FRAME);

        assert!(looks_like_adts_aac(&bytes));
        parse_adts_aac(&bytes).expect("adts after id3");
    }

    #[test]
    fn parses_audio_specific_config_for_lc_mono() {
        let config = parse_audio_specific_config(&[0x14, 0x08]).expect("asc");
        assert_eq!(config.codec_name, "aac");
        assert_eq!(config.sample_rate, Some(16_000));
        assert_eq!(config.channels, Some(1));
        assert_eq!(config.bits_per_sample, None);
    }

    #[test]
    fn parses_audio_specific_config_program_config_and_sbr() {
        let config = parse_audio_specific_config(&[
            0x13, 0x00, 0x05, 0x8c, 0x01, 0x00, 0x01, 0x08, 0x80, 0x00, 0x56, 0xe5, 0x98,
        ])
        .expect("asc");
        assert_eq!(config.codec_name, "aac");
        assert_eq!(config.sample_rate, Some(48_000));
        assert_eq!(config.channels, Some(6));
    }

    #[test]
    fn parses_audio_specific_config_for_mp4_als_wave_metadata() {
        let config = parse_audio_specific_config(&[
            0xf8, 0x9e, 0x01, 0x77, 0x00, 0x00, 0x41, 0x4c, 0x53, 0x00, 0x00, 0x00, 0xbb, 0x80,
            0x00, 0x0a, 0xd8, 0xfd, 0x00, 0x01, 0x24, 0x07, 0xff, 0x01, 0x20, 0x7f, 0x10, 0x80,
            0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, b'R', b'I', b'F', b'F', 0x18, 0x64,
            0x2b, 0x00, b'W', b'A', b'V', b'E', b'f', b'm', b't', b' ', 0x10, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x02, 0x00, 0x80, 0xbb, 0x00, 0x00, 0x00, 0xee, 0x02, 0x00, 0x04, 0x00,
            0x10, 0x00,
        ])
        .expect("asc");
        assert_eq!(config.codec_name, "mp4als");
        assert_eq!(config.sample_rate, Some(48_000));
        assert_eq!(config.channels, Some(2));
        assert_eq!(config.bits_per_sample, Some(16));
    }
}
