use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_dsdiff(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_dsdiff(bytes) {
        return Err(RmpegError::InvalidData("missing DSDIFF header".to_string()));
    }

    let dsd_rate = read_u32_be(find_chunk_payload(bytes, b"FS  ")?, 0)?;
    let sample_rate = dsd_rate / 8;
    let chnl = find_chunk_payload(bytes, b"CHNL")?;
    let channels = read_u16_be(chnl, 0)?;
    let frte = find_chunk_payload(bytes, b"FRTE")?;
    let frame_count = read_u32_be(frte, 0)?;
    let frame_rate = u32::from(read_u16_be(frte, 4)?);
    if sample_rate == 0 || channels == 0 || frame_count == 0 || frame_rate == 0 {
        return Err(RmpegError::InvalidData(
            "invalid DSDIFF DST metadata".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "iff".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            "dst",
            sample_rate,
            channels,
            0,
            frame_count as f64 / frame_rate as f64,
        )],
    })
}

pub fn looks_like_dsdiff(bytes: &[u8]) -> bool {
    bytes.len() >= 16 && bytes.starts_with(b"FRM8") && bytes.get(12..16) == Some(b"DSD ")
}

fn find_chunk_payload<'a>(bytes: &'a [u8], id: &[u8; 4]) -> Result<&'a [u8]> {
    let Some(pos) = find_bytes(bytes, id) else {
        return Err(RmpegError::InvalidData(format!(
            "missing DSDIFF {} chunk",
            String::from_utf8_lossy(id)
        )));
    };
    if pos + 12 > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: pos + 12,
            remaining: bytes.len(),
        });
    }
    let size = usize::try_from(read_u64_be(bytes, pos + 4)?)
        .map_err(|_| RmpegError::InvalidData("DSDIFF chunk too large".to_string()))?;
    let start = pos + 12;
    let end = start
        .checked_add(size)
        .ok_or_else(|| RmpegError::InvalidData("DSDIFF chunk size overflow".to_string()))?;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(&bytes[start..end])
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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

fn read_u64_be(bytes: &[u8], offset: usize) -> Result<u64> {
    let end = offset + 8;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u64::from_be_bytes([
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
    fn parses_dst_metadata_from_dsdiff_chunks() {
        let mut bytes = b"FRM8\0\0\0\0\0\0\0\0DSD ".to_vec();
        bytes.extend_from_slice(&chunk(b"FS  ", &2_822_400_u32.to_be_bytes()));
        let mut chnl = Vec::new();
        chnl.extend_from_slice(&2_u16.to_be_bytes());
        chnl.extend_from_slice(b"SLFTSRGT");
        bytes.extend_from_slice(&chunk(b"CHNL", &chnl));
        bytes.extend_from_slice(&chunk(b"CMPR", b"DST \0DST"));
        let mut frte = Vec::new();
        frte.extend_from_slice(&10_u32.to_be_bytes());
        frte.extend_from_slice(&75_u16.to_be_bytes());
        bytes.extend_from_slice(&chunk(b"FRTE", &frte));

        let doc = parse_dsdiff(&bytes).expect("dsdiff");

        assert_eq!(doc.format, "iff");
        assert_eq!(doc.streams[0].codec_name, "dst");
        assert_eq!(doc.streams[0].sample_rate, Some(352_800));
        assert_eq!(doc.streams[0].duration_seconds, Some(10.0 / 75.0));
    }

    fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&(data.len() as u64).to_be_bytes());
        out.extend_from_slice(data);
        out
    }
}
