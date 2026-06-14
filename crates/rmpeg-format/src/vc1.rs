use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_raw_vc1(bytes: &[u8]) -> Result<ProbeDocument> {
    let dimensions = parse_raw_vc1_dimensions(bytes)?;
    Ok(ProbeDocument {
        format: "vc1".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "vc1",
            dimensions.width,
            dimensions.height,
            Some(0.0),
            None,
        )],
    })
}

pub fn parse_vc1_rcv(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 24 || bytes[3] != 0xc5 {
        return Err(RmpegError::InvalidData(
            "missing VC-1 RCV header".to_string(),
        ));
    }
    let frame_count = read_u24_le(bytes, 0)?;
    let extradata_size = usize::try_from(read_u32_le(bytes, 4)?)
        .map_err(|_| RmpegError::InvalidData("VC-1 RCV extradata is too large".to_string()))?;
    let dimensions = 8 + extradata_size;
    let height = read_u32_le(bytes, dimensions)?;
    let width = read_u32_le(bytes, dimensions + 4)?;
    if frame_count == 0 || width < 16 || height < 16 {
        return Err(RmpegError::InvalidData(
            "VC-1 RCV metadata is implausible".to_string(),
        ));
    }
    let frame_rate = if extradata_size == 4 { 25.0 } else { 1.0 };
    Ok(ProbeDocument {
        format: "vc1test".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "wmv3",
            width,
            height,
            Some(frame_count as f64 / frame_rate),
            None,
        )],
    })
}

pub fn looks_like_raw_vc1(bytes: &[u8]) -> bool {
    parse_raw_vc1_dimensions(bytes).is_ok()
}

fn parse_raw_vc1_dimensions(bytes: &[u8]) -> Result<Dimensions> {
    let sequence_start = find_sequence_start(bytes)
        .ok_or_else(|| RmpegError::InvalidData("missing VC-1 sequence header".to_string()))?;
    let mut bits = BitReader::new(&bytes[sequence_start + 4..]);
    let profile = bits.read_bits(2)?;
    if profile != 3 {
        return Err(RmpegError::InvalidData(
            "only raw advanced-profile VC-1 headers are supported".to_string(),
        ));
    }
    bits.skip_bits(3)?; // level
    bits.skip_bits(2)?; // chroma format
    bits.skip_bits(3)?; // postprocessing frame-rate quantizer
    bits.skip_bits(5)?; // postprocessing bit-rate quantizer
    bits.skip_bits(1)?; // postproc flag
    let width = (bits.read_bits(12)? + 1) * 2;
    let height = (bits.read_bits(12)? + 1) * 2;
    if width < 16 || height < 16 || width > 8192 || height > 8192 {
        return Err(RmpegError::InvalidData(
            "VC-1 dimensions are implausible".to_string(),
        ));
    }
    Ok(Dimensions { width, height })
}

fn find_sequence_start(bytes: &[u8]) -> Option<usize> {
    let scan = bytes.len().min(64);
    (0..scan.saturating_sub(3)).find(|&pos| bytes[pos..pos + 4] == [0, 0, 1, 0x0f])
}

fn read_u24_le(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 3;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(
        u32::from(bytes[pos])
            | (u32::from(bytes[pos + 1]) << 8)
            | (u32::from(bytes[pos + 2]) << 16),
    )
}

fn read_u32_le(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

#[derive(Debug, Clone, Copy)]
struct Dimensions {
    width: u32,
    height: u32,
}

struct BitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit_pos: 0 }
    }

    fn read_bits(&mut self, count: usize) -> Result<u32> {
        if count > 32 {
            return Err(RmpegError::InvalidData(
                "VC-1 bit read is too large".to_string(),
            ));
        }
        let mut value = 0;
        for _ in 0..count {
            let byte_pos = self.bit_pos / 8;
            let bit_in_byte = 7 - (self.bit_pos % 8);
            let byte = self.bytes.get(byte_pos).ok_or(RmpegError::UnexpectedEof {
                needed: byte_pos + 1,
                remaining: self.bytes.len(),
            })?;
            value = (value << 1) | u32::from((byte >> bit_in_byte) & 1);
            self.bit_pos += 1;
        }
        Ok(value)
    }

    fn skip_bits(&mut self, count: usize) -> Result<()> {
        self.read_bits(count).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_advanced_profile_sequence_dimensions() {
        let bytes = [
            0x00, 0x00, 0x01, 0x0f, 0xc2, 0xc0, 0x05, 0x70, 0x47, 0x8a, 0x05, 0x78,
        ];
        let doc = parse_raw_vc1(&bytes).expect("valid raw vc1");
        assert_eq!(doc.streams[0].codec_name, "vc1");
        assert_eq!(doc.streams[0].width, Some(176));
        assert_eq!(doc.streams[0].height, Some(144));
    }

    #[test]
    fn parses_observed_rcv_wrapper() {
        let mut bytes = vec![0; 24];
        bytes[0..3].copy_from_slice(&[25, 0, 0]);
        bytes[3] = 0xc5;
        bytes[4..8].copy_from_slice(&4_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&576_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&720_u32.to_le_bytes());

        let doc = parse_vc1_rcv(&bytes).expect("valid rcv");
        assert_eq!(doc.format, "vc1test");
        assert_eq!(doc.streams[0].codec_name, "wmv3");
        assert_eq!(doc.streams[0].duration_seconds, Some(1.0));
    }
}
