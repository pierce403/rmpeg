use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_bfstm_or_brstm(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_bfstm_or_brstm(bytes) {
        return Err(RmpegError::InvalidData(
            "missing BFSTM/BRSTM header".to_string(),
        ));
    }

    let magic = &bytes[0..4];
    let stream = read_stream_info(bytes, magic)?;

    let (format, codec_name) = match magic {
        b"CSTM" => ("bfstm", "adpcm_thp_le"),
        b"FSTM" => ("bfstm", "adpcm_thp"),
        b"RSTM" => ("brstm", "adpcm_thp"),
        _ => unreachable!(),
    };

    Ok(ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec_name,
            stream.sample_rate,
            stream.channels,
            0,
            stream.sample_count as f64 / stream.sample_rate as f64,
        )],
    })
}

pub fn looks_like_bfstm_or_brstm(bytes: &[u8]) -> bool {
    bytes.len() >= 0x70
        && matches!(&bytes[0..4], b"CSTM" | b"FSTM" | b"RSTM")
        && matches!(&bytes[4..6], [0xff, 0xfe] | [0xfe, 0xff])
}

struct StreamInfo {
    sample_rate: u32,
    channels: u16,
    sample_count: u32,
}

fn read_stream_info(bytes: &[u8], magic: &[u8]) -> Result<StreamInfo> {
    let base = find_section(
        bytes,
        match magic {
            b"RSTM" => b"HEAD",
            _ => b"INFO",
        },
    )?
    .checked_add(0x20)
    .ok_or_else(|| RmpegError::InvalidData("BFSTM stream offset overflow".to_string()))?;
    if base + 0x10 > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: base + 0x10,
            remaining: bytes.len(),
        });
    }

    let channels = u16::from(bytes[base + 2]);
    let sample_rate = match magic {
        b"CSTM" => read_u32_le(bytes, base + 4)?,
        b"FSTM" => read_u32_be(bytes, base + 4)?,
        b"RSTM" => u32::from(read_u16_be(bytes, base + 4)?),
        _ => unreachable!(),
    };
    let sample_count = match magic {
        b"CSTM" => read_u32_le(bytes, base + 12)?,
        _ => read_u32_be(bytes, base + 12)?,
    };

    if channels == 0 || sample_rate == 0 || sample_count == 0 {
        return Err(RmpegError::InvalidData(
            "invalid BFSTM stream metadata".to_string(),
        ));
    }

    Ok(StreamInfo {
        sample_rate,
        channels,
        sample_count,
    })
}

fn find_section(bytes: &[u8], section: &[u8; 4]) -> Result<usize> {
    bytes
        .windows(4)
        .position(|window| window == section)
        .ok_or_else(|| RmpegError::InvalidData("missing BFSTM info section".to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_little_endian_bcstm_header() {
        let mut bytes = vec![0; 0x80];
        bytes[0..6].copy_from_slice(b"CSTM\xff\xfe");
        bytes[0x40..0x44].copy_from_slice(b"INFO");
        bytes[0x60] = 2;
        bytes[0x62] = 2;
        bytes[0x64..0x68].copy_from_slice(&32_000_u32.to_le_bytes());
        bytes[0x6c..0x70].copy_from_slice(&69_760_u32.to_le_bytes());

        let doc = parse_bfstm_or_brstm(&bytes).expect("bcstm");

        assert_eq!(doc.format, "bfstm");
        assert_eq!(doc.streams[0].codec_name, "adpcm_thp_le");
        assert_eq!(doc.streams[0].sample_rate, Some(32_000));
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].duration_seconds, Some(2.18));
    }

    #[test]
    fn parses_observed_big_endian_bfstm_header() {
        let mut bytes = vec![0; 0x80];
        bytes[0..6].copy_from_slice(b"FSTM\xfe\xff");
        bytes[0x40..0x44].copy_from_slice(b"INFO");
        bytes[0x60] = 2;
        bytes[0x62] = 1;
        bytes[0x64..0x68].copy_from_slice(&32_000_u32.to_be_bytes());
        bytes[0x6c..0x70].copy_from_slice(&226_499_u32.to_be_bytes());

        let doc = parse_bfstm_or_brstm(&bytes).expect("bfstm");

        assert_eq!(doc.format, "bfstm");
        assert_eq!(doc.streams[0].codec_name, "adpcm_thp");
        assert_eq!(doc.streams[0].channels, Some(1));
        assert_eq!(doc.streams[0].duration_seconds, Some(226_499.0 / 32_000.0));
    }

    #[test]
    fn parses_observed_brstm_header() {
        let mut bytes = vec![0; 0x80];
        bytes[0..6].copy_from_slice(b"RSTM\xfe\xff");
        bytes[0x40..0x44].copy_from_slice(b"HEAD");
        bytes[0x60] = 2;
        bytes[0x62] = 6;
        bytes[0x64..0x66].copy_from_slice(&32_000_u16.to_be_bytes());
        bytes[0x6c..0x70].copy_from_slice(&2_655_726_u32.to_be_bytes());

        let doc = parse_bfstm_or_brstm(&bytes).expect("brstm");

        assert_eq!(doc.format, "brstm");
        assert_eq!(doc.streams[0].codec_name, "adpcm_thp");
        assert_eq!(doc.streams[0].channels, Some(6));
        assert_eq!(
            doc.streams[0].duration_seconds,
            Some(2_655_726.0 / 32_000.0)
        );
    }
}
