use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_mpeg4_visual(bytes: &[u8]) -> Result<ProbeDocument> {
    let dimensions = parse_vol_dimensions(bytes)?;
    Ok(ProbeDocument {
        format: "m4v".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "mpeg4",
            dimensions.width,
            dimensions.height,
            Some(0.0),
            None,
        )],
    })
}

pub fn looks_like_mpeg4_visual(bytes: &[u8]) -> bool {
    parse_vol_dimensions(bytes).is_ok()
}

fn parse_vol_dimensions(bytes: &[u8]) -> Result<Dimensions> {
    let start = find_vol_start(bytes)
        .ok_or_else(|| RmpegError::InvalidData("missing MPEG-4 visual object layer".to_string()))?;
    let mut bits = BitReader::new(&bytes[start + 4..]);
    bits.skip_bits(1)?; // random_accessible_vol
    bits.skip_bits(8)?; // video_object_type_indication
    if bits.read_bool()? {
        bits.skip_bits(4)?; // video_object_layer_verid
        bits.skip_bits(3)?; // video_object_layer_priority
    }
    let aspect_ratio_info = bits.read_bits(4)?;
    if aspect_ratio_info == 15 {
        bits.skip_bits(16)?;
    }
    if bits.read_bool()? {
        bits.skip_bits(2)?; // chroma_format
        bits.skip_bits(1)?; // low_delay
        if bits.read_bool()? {
            return Err(RmpegError::InvalidData(
                "unsupported MPEG-4 VBV parameters".to_string(),
            ));
        }
    }
    let shape = bits.read_bits(2)?;
    if shape != 0 {
        return Err(RmpegError::InvalidData(
            "only rectangular MPEG-4 visual objects are supported".to_string(),
        ));
    }
    bits.expect_marker()?;
    let vop_time_increment_resolution = bits.read_bits(16)?;
    bits.expect_marker()?;
    if bits.read_bool()? {
        let bits_needed = vop_time_increment_resolution
            .saturating_sub(1)
            .leading_zeros();
        let width = 32 - bits_needed;
        bits.skip_bits(width.max(1) as usize)?;
    }
    bits.expect_marker()?;
    let width = bits.read_bits(13)?;
    bits.expect_marker()?;
    let height = bits.read_bits(13)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "MPEG-4 dimensions must be nonzero".to_string(),
        ));
    }
    Ok(Dimensions { width, height })
}

fn find_vol_start(bytes: &[u8]) -> Option<usize> {
    let scan = bytes.len().min(4096);
    (0..scan.saturating_sub(3)).find(|&pos| {
        bytes[pos] == 0
            && bytes[pos + 1] == 0
            && bytes[pos + 2] == 1
            && (0x20..=0x2f).contains(&bytes[pos + 3])
    })
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

    fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read_bits(1)? != 0)
    }

    fn read_bits(&mut self, count: usize) -> Result<u32> {
        if count > 32 {
            return Err(RmpegError::InvalidData(
                "MPEG-4 bit read is too large".to_string(),
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

    fn expect_marker(&mut self) -> Result<()> {
        if self.read_bits(1)? == 1 {
            Ok(())
        } else {
            Err(RmpegError::InvalidData(
                "missing MPEG-4 marker bit".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rectangular_vol_dimensions() {
        let bytes = [
            0x00, 0x00, 0x01, 0x20, 0x00, 0xc4, 0x8d, 0x88, 0x00, 0x2d, 0x0a, 0x04, 0x1e, 0x14,
        ];

        let doc = parse_mpeg4_visual(&bytes).expect("valid mpeg4 visual");
        assert_eq!(doc.format, "m4v");
        assert_eq!(doc.streams[0].width, Some(320));
        assert_eq!(doc.streams[0].height, Some(240));
    }
}
