use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const ASF_HEADER_GUID: [u8; 16] = [
    0x30, 0x26, 0xb2, 0x75, 0x8e, 0x66, 0xcf, 0x11, 0xa6, 0xd9, 0x00, 0xaa, 0x00, 0x62, 0xce, 0x6c,
];
const FILE_PROPERTIES_GUID: [u8; 16] = [
    0xa1, 0xdc, 0xab, 0x8c, 0x47, 0xa9, 0xcf, 0x11, 0x8e, 0xe4, 0x00, 0xc0, 0x0c, 0x20, 0x53, 0x65,
];
const STREAM_PROPERTIES_GUID: [u8; 16] = [
    0x91, 0x07, 0xdc, 0xb7, 0xb7, 0xa9, 0xcf, 0x11, 0x8e, 0xe6, 0x00, 0xc0, 0x0c, 0x20, 0x53, 0x65,
];
const AUDIO_MEDIA_GUID: [u8; 16] = [
    0x40, 0x9e, 0x69, 0xf8, 0x4d, 0x5b, 0xcf, 0x11, 0xa8, 0xfd, 0x00, 0x80, 0x5f, 0x5c, 0x44, 0x2b,
];
const VIDEO_MEDIA_GUID: [u8; 16] = [
    0xc0, 0xef, 0x19, 0xbc, 0x4d, 0x5b, 0xcf, 0x11, 0xa8, 0xfd, 0x00, 0x80, 0x5f, 0x5c, 0x44, 0x2b,
];

#[derive(Debug, Default)]
struct AsfMetadata {
    declared_file_size: Option<u64>,
    play_duration_100ns: Option<u64>,
    preroll_ms: Option<u64>,
    header_size: usize,
    codec_name: Option<&'static str>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    bits_per_sample: Option<u16>,
    average_bytes_per_second: Option<u32>,
    streams: Vec<AsfStream>,
}

#[derive(Debug, Clone)]
enum AsfStream {
    Audio {
        codec_name: &'static str,
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
    },
    Video {
        codec_name: &'static str,
        width: u32,
        height: u32,
    },
}

pub fn parse_asf(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_asf(bytes) {
        return Err(RmpegError::InvalidData("missing ASF header".to_string()));
    }
    if bytes.len() < 30 {
        return Err(RmpegError::UnexpectedEof {
            needed: 30,
            remaining: bytes.len(),
        });
    }

    let header_size = usize::try_from(read_u64_le(bytes, 16)?)
        .map_err(|_| RmpegError::InvalidData("ASF header is too large".to_string()))?;
    if header_size > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: header_size,
            remaining: bytes.len(),
        });
    }
    let object_count = read_u32_le(bytes, 24)?;
    let mut metadata = AsfMetadata {
        header_size,
        ..AsfMetadata::default()
    };

    let mut pos = 30;
    for _ in 0..object_count {
        if pos + 24 > header_size {
            break;
        }
        let guid = &bytes[pos..pos + 16];
        let object_size = usize::try_from(read_u64_le(bytes, pos + 16)?)
            .map_err(|_| RmpegError::InvalidData("ASF object is too large".to_string()))?;
        if object_size < 24 || pos + object_size > header_size {
            return Err(RmpegError::InvalidData(
                "invalid ASF object size".to_string(),
            ));
        }
        let data = &bytes[pos + 24..pos + object_size];
        if guid == FILE_PROPERTIES_GUID {
            parse_file_properties(data, &mut metadata)?;
        } else if guid == STREAM_PROPERTIES_GUID {
            parse_stream_properties(data, &mut metadata)?;
        }
        pos += object_size;
    }

    let duration_seconds = asf_duration_seconds(bytes, &metadata);
    if metadata.streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "ASF file has no supported audio/video stream".to_string(),
        ));
    }
    let truncated = metadata
        .declared_file_size
        .is_some_and(|declared| (bytes.len() as u64) < declared);
    let streams = metadata
        .streams
        .iter()
        .enumerate()
        .map(|(index, stream)| match stream {
            AsfStream::Audio {
                codec_name,
                sample_rate,
                channels,
                bits_per_sample,
            } => StreamMetadata::audio(
                index,
                *codec_name,
                *sample_rate,
                *channels,
                if truncated { 0 } else { *bits_per_sample },
                duration_seconds,
            ),
            AsfStream::Video {
                codec_name,
                width,
                height,
            } => StreamMetadata::video(
                index,
                *codec_name,
                *width,
                *height,
                Some(duration_seconds),
                None,
            ),
        })
        .collect();
    Ok(ProbeDocument {
        format: "asf".to_string(),
        streams,
    })
}

pub fn looks_like_asf(bytes: &[u8]) -> bool {
    bytes.starts_with(&ASF_HEADER_GUID)
}

fn parse_file_properties(data: &[u8], metadata: &mut AsfMetadata) -> Result<()> {
    if data.len() < 64 {
        return Err(RmpegError::UnexpectedEof {
            needed: 64,
            remaining: data.len(),
        });
    }
    metadata.declared_file_size = Some(read_u64_le(data, 16)?);
    metadata.play_duration_100ns = Some(read_u64_le(data, 40)?);
    metadata.preroll_ms = Some(read_u64_le(data, 56)?);
    Ok(())
}

fn parse_stream_properties(data: &[u8], metadata: &mut AsfMetadata) -> Result<()> {
    if data.len() < 54 {
        return Err(RmpegError::UnexpectedEof {
            needed: 54,
            remaining: data.len(),
        });
    }
    let type_len = usize::try_from(read_u32_le(data, 40)?)
        .map_err(|_| RmpegError::InvalidData("ASF stream data is too large".to_string()))?;
    let type_start = 54;
    let type_end = type_start + type_len;
    if type_end > data.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: type_end,
            remaining: data.len(),
        });
    }
    if data[0..16] == AUDIO_MEDIA_GUID {
        parse_audio_stream_data(&data[type_start..type_end], metadata)?;
    } else if data[0..16] == VIDEO_MEDIA_GUID {
        parse_video_stream_data(&data[type_start..type_end], metadata)?;
    }
    Ok(())
}

fn parse_audio_stream_data(wave: &[u8], metadata: &mut AsfMetadata) -> Result<()> {
    if wave.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: wave.len(),
        });
    }
    let format_tag = read_u16_le(wave, 0)?;
    let codec_name = match format_tag {
        0x0163 => Some("wmalossless"),
        0x0162 => Some("wmapro"),
        0x0161 => Some("wmav2"),
        _ => None,
    };
    let Some(codec_name) = codec_name else {
        return Ok(());
    };
    let channels = read_u16_le(wave, 2)?;
    let sample_rate = read_u32_le(wave, 4)?;
    metadata.average_bytes_per_second = Some(read_u32_le(wave, 8)?);
    let header_bits_per_sample = read_u16_le(wave, 14)?;
    let bits_per_sample = if codec_name == "wmalossless" {
        header_bits_per_sample
    } else {
        0
    };
    metadata.codec_name = Some(codec_name);
    metadata.channels = Some(channels);
    metadata.sample_rate = Some(sample_rate);
    metadata.bits_per_sample = Some(bits_per_sample);
    metadata.streams.push(AsfStream::Audio {
        codec_name,
        sample_rate,
        channels,
        bits_per_sample,
    });
    Ok(())
}

fn parse_video_stream_data(video: &[u8], metadata: &mut AsfMetadata) -> Result<()> {
    if video.len() < 31 {
        return Err(RmpegError::UnexpectedEof {
            needed: 31,
            remaining: video.len(),
        });
    }
    let codec_name = match &video[27..31] {
        b"MSS2" => "mss2",
        b"G2M2" | b"G2M3" | b"G2M4" => "g2m",
        _ => return Ok(()),
    };
    let width = read_u32_le(video, 0)?;
    let height = read_u32_le(video, 4)?;
    if width == 0 || height == 0 {
        return Ok(());
    }
    metadata.streams.push(AsfStream::Video {
        codec_name,
        width,
        height,
    });
    Ok(())
}

fn asf_duration_seconds(bytes: &[u8], metadata: &AsfMetadata) -> f64 {
    let header_duration = match (metadata.play_duration_100ns, metadata.preroll_ms) {
        (Some(play), Some(preroll)) => (play as f64 / 10_000_000.0) - (preroll as f64 / 1000.0),
        _ => 0.0,
    };

    let Some(declared_file_size) = metadata.declared_file_size else {
        return header_duration.max(0.0);
    };
    let Some(avg_bytes_per_second) = metadata
        .average_bytes_per_second
        .filter(|value| *value != 0)
    else {
        return header_duration.max(0.0);
    };
    if bytes.len() as u64 >= declared_file_size {
        return header_duration.max(0.0);
    }

    let payload_bytes = bytes.len().saturating_sub(metadata.header_size + 50);
    let truncated_duration = payload_bytes as f64 / avg_bytes_per_second as f64;
    header_duration.max(0.0).min(truncated_duration)
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn read_u64_le(bytes: &[u8], offset: usize) -> Result<u64> {
    let end = offset + 8;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_mss2_video_payload() {
        let mut metadata = AsfMetadata::default();
        let mut payload = vec![0; 64];
        payload[0..4].copy_from_slice(&320u32.to_le_bytes());
        payload[4..8].copy_from_slice(&240u32.to_le_bytes());
        payload[27..31].copy_from_slice(b"MSS2");

        parse_video_stream_data(&payload, &mut metadata).unwrap();

        match metadata.streams.as_slice() {
            [AsfStream::Video {
                codec_name,
                width,
                height,
            }] => {
                assert_eq!(*codec_name, "mss2");
                assert_eq!(*width, 320);
                assert_eq!(*height, 240);
            }
            streams => panic!("unexpected streams: {streams:?}"),
        }
    }

    #[test]
    fn reports_zero_bits_for_wmapro() {
        let mut metadata = AsfMetadata::default();
        let wave = [
            0x62, 0x01, // WMAPro
            0x02, 0x00, // channels
            0x44, 0xac, 0x00, 0x00, // sample rate
            0x85, 0x3e, 0x00, 0x00, // byte rate
            0x9d, 0x0b, // block align
            0x18, 0x00, // header bits per sample
        ];

        parse_audio_stream_data(&wave, &mut metadata).unwrap();

        match metadata.streams.as_slice() {
            [AsfStream::Audio {
                codec_name,
                bits_per_sample,
                ..
            }] => {
                assert_eq!(*codec_name, "wmapro");
                assert_eq!(*bits_per_sample, 0);
            }
            streams => panic!("unexpected streams: {streams:?}"),
        }
    }

    #[test]
    fn maps_observed_g2m_video_tags() {
        let mut metadata = AsfMetadata::default();
        let mut payload = vec![0; 64];
        payload[0..4].copy_from_slice(&1280u32.to_le_bytes());
        payload[4..8].copy_from_slice(&1024u32.to_le_bytes());
        payload[27..31].copy_from_slice(b"G2M4");

        parse_video_stream_data(&payload, &mut metadata).unwrap();

        match metadata.streams.as_slice() {
            [AsfStream::Video {
                codec_name,
                width,
                height,
            }] => {
                assert_eq!(*codec_name, "g2m");
                assert_eq!(*width, 1280);
                assert_eq!(*height, 1024);
            }
            streams => panic!("unexpected streams: {streams:?}"),
        }
    }
}
