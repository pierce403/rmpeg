use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug)]
struct OggPage<'a> {
    granule_position: Option<u64>,
    segments: &'a [u8],
    payload: &'a [u8],
    end: usize,
}

pub fn parse_ogg(bytes: &[u8]) -> Result<ProbeDocument> {
    let mut pos = 0;
    let mut first_packet = Vec::new();
    let mut saw_first_packet = false;
    let mut last_granule = None;

    while pos < bytes.len() {
        let page = parse_page(bytes, pos)?;
        if let Some(granule) = page.granule_position {
            last_granule = Some(granule);
        }

        if !saw_first_packet {
            append_first_packet(&page, &mut first_packet, &mut saw_first_packet);
        }

        pos = page.end;
    }

    if first_packet.starts_with(b"OpusHead") {
        parse_opus_head(&first_packet, last_granule)
    } else {
        Err(RmpegError::Unsupported(
            "only Ogg Opus probing is implemented".to_string(),
        ))
    }
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
    if payload_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: payload_end,
            remaining: bytes.len(),
        });
    }

    Ok(OggPage {
        granule_position: if granule_raw == u64::MAX {
            None
        } else {
            Some(granule_raw)
        },
        segments: &bytes[segment_start..payload_start],
        payload: &bytes[payload_start..payload_end],
        end: payload_end,
    })
}

fn append_first_packet(page: &OggPage<'_>, packet: &mut Vec<u8>, complete: &mut bool) {
    let mut offset = 0;
    for segment_len in page.segments {
        let len = usize::from(*segment_len);
        packet.extend_from_slice(&page.payload[offset..offset + len]);
        offset += len;
        if len < 255 {
            *complete = true;
            break;
        }
    }
}

fn parse_opus_head(packet: &[u8], last_granule: Option<u64>) -> Result<ProbeDocument> {
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
    Ok(ProbeDocument {
        format: "ogg".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "opus",
            48_000,
            channels,
            0,
            duration_seconds,
        )],
    })
}
