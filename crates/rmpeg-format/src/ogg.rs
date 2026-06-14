use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug)]
struct OggPage<'a> {
    granule_position: Option<u64>,
    serial: u32,
    segments: &'a [u8],
    payload: &'a [u8],
    end: usize,
    truncated: bool,
}

#[derive(Debug, Default)]
struct OggStreamState {
    serial: u32,
    first_packet: Vec<u8>,
    first_packet_complete: bool,
    last_granule: Option<u64>,
}

pub fn parse_ogg(bytes: &[u8]) -> Result<ProbeDocument> {
    let mut pos = 0;
    let mut streams = Vec::<OggStreamState>::new();

    while pos < bytes.len() {
        let page = parse_page(bytes, pos)?;
        let stream_index = match streams
            .iter()
            .position(|stream| stream.serial == page.serial)
        {
            Some(index) => index,
            None => {
                streams.push(OggStreamState {
                    serial: page.serial,
                    ..OggStreamState::default()
                });
                streams.len() - 1
            }
        };
        let stream = &mut streams[stream_index];
        if !page.truncated {
            if let Some(granule) = page.granule_position {
                stream.last_granule = Some(granule);
            }
        }

        if !stream.first_packet_complete {
            append_first_packet(
                &page,
                &mut stream.first_packet,
                &mut stream.first_packet_complete,
            );
        }

        let truncated = page.truncated;
        pos = page.end;
        if truncated {
            break;
        }
    }

    let mut parsed_streams = Vec::new();
    for stream in streams {
        let index = parsed_streams.len();
        if let Some(metadata) =
            parse_stream_packet(index, &stream.first_packet, stream.last_granule)?
        {
            parsed_streams.push(metadata);
        }
    }

    if parsed_streams.is_empty() {
        return Err(RmpegError::Unsupported(
            "only Ogg Opus, Vorbis, Theora, and VP8 probing is implemented".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "ogg".to_string(),
        streams: parsed_streams,
    })
}

fn parse_page(bytes: &[u8], pos: usize) -> Result<OggPage<'_>> {
    if bytes.len().saturating_sub(pos) < 27 {
        return Err(RmpegError::UnexpectedEof {
            needed: 27,
            remaining: bytes.len().saturating_sub(pos),
        });
    }
    if &bytes[pos..pos + 4] != b"OggS" {
        return Err(RmpegError::InvalidData(
            "missing OggS capture pattern".to_string(),
        ));
    }
    if bytes[pos + 4] != 0 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported Ogg version {}",
            bytes[pos + 4]
        )));
    }
    let granule_raw = u64::from_le_bytes([
        bytes[pos + 6],
        bytes[pos + 7],
        bytes[pos + 8],
        bytes[pos + 9],
        bytes[pos + 10],
        bytes[pos + 11],
        bytes[pos + 12],
        bytes[pos + 13],
    ]);
    let serial = u32::from_le_bytes([
        bytes[pos + 14],
        bytes[pos + 15],
        bytes[pos + 16],
        bytes[pos + 17],
    ]);
    let page_segments = usize::from(bytes[pos + 26]);
    let segment_start = pos + 27;
    let payload_start = segment_start + page_segments;
    if payload_start > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: payload_start,
            remaining: bytes.len(),
        });
    }
    let payload_len: usize = bytes[segment_start..payload_start]
        .iter()
        .map(|value| usize::from(*value))
        .sum();
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or_else(|| RmpegError::InvalidData("Ogg page size overflow".to_string()))?;
    let truncated = payload_end > bytes.len();
    let available_payload_end = payload_end.min(bytes.len());

    Ok(OggPage {
        granule_position: if granule_raw == u64::MAX {
            None
        } else {
            Some(granule_raw)
        },
        serial,
        segments: &bytes[segment_start..payload_start],
        payload: &bytes[payload_start..available_payload_end],
        end: available_payload_end,
        truncated,
    })
}

fn append_first_packet(page: &OggPage<'_>, packet: &mut Vec<u8>, complete: &mut bool) {
    let mut offset = 0;
    for segment_len in page.segments {
        let len = usize::from(*segment_len);
        let end = offset + len;
        if end > page.payload.len() {
            packet.extend_from_slice(&page.payload[offset..]);
            return;
        }
        packet.extend_from_slice(&page.payload[offset..end]);
        offset += len;
        if len < 255 {
            *complete = true;
            break;
        }
    }
}

fn parse_stream_packet(
    index: usize,
    packet: &[u8],
    last_granule: Option<u64>,
) -> Result<Option<StreamMetadata>> {
    if packet.starts_with(b"OpusHead") {
        return parse_opus_head(index, packet, last_granule).map(Some);
    }
    if packet.starts_with(b"\x01vorbis") {
        return parse_vorbis_identification(index, packet, last_granule).map(Some);
    }
    if packet.starts_with(b"\x80theora") {
        return parse_theora_identification(index, packet, last_granule).map(Some);
    }
    if packet.starts_with(b"OVP80") {
        return parse_ogg_vp8_header(index, packet, last_granule).map(Some);
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ogg_page(granule: u64, packet: &[u8], truncate_payload_by: usize) -> Vec<u8> {
        assert!(packet.len() < 255);
        let payload_len = packet.len().saturating_sub(truncate_payload_by);
        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.push(0);
        page.push(0);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&1_u32.to_le_bytes());
        page.extend_from_slice(&0_u32.to_le_bytes());
        page.extend_from_slice(&0_u32.to_le_bytes());
        page.push(1);
        page.push(packet.len() as u8);
        page.extend_from_slice(&packet[..payload_len]);
        page
    }

    #[test]
    fn parses_vorbis_metadata_from_truncated_final_page() {
        let mut ident = vec![0x01];
        ident.extend_from_slice(b"vorbis");
        ident.extend_from_slice(&0_u32.to_le_bytes());
        ident.push(2);
        ident.extend_from_slice(&44_100_u32.to_le_bytes());
        ident.extend_from_slice(&[0; 14]);

        let mut bytes = ogg_page(0, &ident, 0);
        bytes.extend_from_slice(&ogg_page(220_500, b"complete media page", 0));
        bytes.extend_from_slice(&ogg_page(230_000, b"partial media page", 7));

        let doc = parse_ogg(&bytes).expect("truncated Ogg should still expose metadata");
        assert_eq!(doc.streams[0].codec_name, "vorbis");
        assert_eq!(doc.streams[0].sample_rate, Some(44_100));
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].duration_seconds, Some(5.0));
    }
}

fn parse_opus_head(
    index: usize,
    packet: &[u8],
    last_granule: Option<u64>,
) -> Result<StreamMetadata> {
    if packet.len() < 19 {
        return Err(RmpegError::UnexpectedEof {
            needed: 19,
            remaining: packet.len(),
        });
    }
    let channels = u16::from(packet[9]);
    if channels == 0 {
        return Err(RmpegError::InvalidData(
            "OpusHead has zero channels".to_string(),
        ));
    }
    let duration_seconds = last_granule.unwrap_or(0) as f64 / 48_000.0;
    Ok(StreamMetadata::audio(
        index,
        "opus",
        48_000,
        channels,
        0,
        duration_seconds,
    ))
}

fn parse_vorbis_identification(
    index: usize,
    packet: &[u8],
    last_granule: Option<u64>,
) -> Result<StreamMetadata> {
    if packet.len() < 30 {
        return Err(RmpegError::UnexpectedEof {
            needed: 30,
            remaining: packet.len(),
        });
    }
    let channels = u16::from(packet[11]);
    if channels == 0 {
        return Err(RmpegError::InvalidData(
            "Vorbis identification header has zero channels".to_string(),
        ));
    }
    let sample_rate = u32::from_le_bytes([packet[12], packet[13], packet[14], packet[15]]);
    if sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "Vorbis identification header has zero sample rate".to_string(),
        ));
    }
    let duration_seconds = last_granule.unwrap_or(0) as f64 / sample_rate as f64;
    Ok(StreamMetadata::audio(
        index,
        "vorbis",
        sample_rate,
        channels,
        0,
        duration_seconds,
    ))
}

fn parse_theora_identification(
    index: usize,
    packet: &[u8],
    last_granule: Option<u64>,
) -> Result<StreamMetadata> {
    if packet.len() < 42 {
        return Err(RmpegError::UnexpectedEof {
            needed: 42,
            remaining: packet.len(),
        });
    }
    let width = read_u24_be(packet, 14)?;
    let height = read_u24_be(packet, 17)?;
    let frame_rate_numerator = read_u32_be(packet, 22)?;
    let frame_rate_denominator = read_u32_be(packet, 26)?;
    if width == 0 || height == 0 || frame_rate_numerator == 0 || frame_rate_denominator == 0 {
        return Err(RmpegError::InvalidData(
            "Theora identification metadata must be nonzero".to_string(),
        ));
    }
    let granule_shift = ((packet[40] & 0x03) << 3) | (packet[41] >> 5);
    let duration_seconds = last_granule.map(|granule| {
        let frame_count = if granule_shift == 0 {
            granule
        } else {
            let delta_mask = (1_u64 << granule_shift) - 1;
            (granule >> granule_shift) + (granule & delta_mask)
        };
        frame_count as f64 * frame_rate_denominator as f64 / frame_rate_numerator as f64
    });

    Ok(StreamMetadata::video(
        index,
        "theora",
        width,
        height,
        duration_seconds,
        Some(format!("{frame_rate_numerator}/{frame_rate_denominator}")),
    ))
}

fn parse_ogg_vp8_header(
    index: usize,
    packet: &[u8],
    last_granule: Option<u64>,
) -> Result<StreamMetadata> {
    if packet.len() < 26 {
        return Err(RmpegError::UnexpectedEof {
            needed: 26,
            remaining: packet.len(),
        });
    }
    let width = u32::from(read_u16_be(packet, 8)?);
    let height = u32::from(read_u16_be(packet, 10)?);
    let frame_rate_numerator = read_u32_be(packet, 18)?;
    let frame_rate_denominator = read_u32_be(packet, 22)?;
    if width == 0 || height == 0 || frame_rate_numerator == 0 || frame_rate_denominator == 0 {
        return Err(RmpegError::InvalidData(
            "Ogg VP8 header metadata must be nonzero".to_string(),
        ));
    }
    let duration_seconds = last_granule.map(|granule| {
        let frame_count = granule >> 32;
        frame_count as f64 * frame_rate_denominator as f64 / frame_rate_numerator as f64
    });

    Ok(StreamMetadata::video(
        index,
        "vp8",
        width,
        height,
        duration_seconds,
        Some(format!("{frame_rate_numerator}/{frame_rate_denominator}")),
    ))
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u24_be(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 3;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok((u32::from(bytes[offset]) << 16)
        | (u32::from(bytes[offset + 1]) << 8)
        | u32::from(bytes[offset + 2]))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}
