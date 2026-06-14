use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug)]
struct OggPage<'a> {
    granule_position: Option<u64>,
    segments: &'a [u8],
    payload: &'a [u8],
    end: usize,
    truncated: bool,
}

pub fn parse_ogg(bytes: &[u8]) -> Result<ProbeDocument> {
    let mut pos = 0;
    let mut first_packet = Vec::new();
    let mut saw_first_packet = false;
    let mut last_granule = None;

    while pos < bytes.len() {
        let page = parse_page(bytes, pos)?;
        if !page.truncated {
            if let Some(granule) = page.granule_position {
                last_granule = Some(granule);
            }
        }

        if !saw_first_packet {
            append_first_packet(&page, &mut first_packet, &mut saw_first_packet);
        }

        let truncated = page.truncated;
        pos = page.end;
        if truncated {
            break;
        }
    }

    if first_packet.starts_with(b"OpusHead") {
        parse_opus_head(&first_packet, last_granule)
    } else if first_packet.starts_with(b"\x01vorbis") {
        parse_vorbis_identification(&first_packet, last_granule)
    } else {
        Err(RmpegError::Unsupported(
            "only Ogg Opus and Ogg Vorbis probing is implemented".to_string(),
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
    let truncated = payload_end > bytes.len();
    let available_payload_end = payload_end.min(bytes.len());

    Ok(OggPage {
        granule_position: if granule_raw == u64::MAX {
            None
        } else {
            Some(granule_raw)
        },
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

fn parse_vorbis_identification(packet: &[u8], last_granule: Option<u64>) -> Result<ProbeDocument> {
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
    Ok(ProbeDocument {
        format: "ogg".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "vorbis",
            sample_rate,
            channels,
            0,
            duration_seconds,
        )],
    })
}
