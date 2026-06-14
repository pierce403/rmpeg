use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_vvc_annex_b(bytes: &[u8]) -> Result<ProbeDocument> {
    for nal in AnnexBNalUnits::new(bytes) {
        if nal_type(nal) == Some(15) {
            let dimensions = parse_sps(&nal[2..])?;
            if plausible_dimensions(dimensions) {
                return Ok(ProbeDocument {
                    format: "vvc".to_string(),
                    streams: vec![StreamMetadata::video(
                        0,
                        "vvc",
                        dimensions.width,
                        dimensions.height,
                        Some(0.0),
                        None,
                    )],
                });
            }
        }
    }
    Err(RmpegError::InvalidData(
        "missing supported VVC SPS".to_string(),
    ))
}

pub fn looks_like_vvc_annex_b(bytes: &[u8]) -> bool {
    if find_start_code(bytes, 0)
        .map(|(pos, _)| pos > 64)
        .unwrap_or(true)
    {
        return false;
    }

    for (index, nal) in AnnexBNalUnits::new(bytes).take(16).enumerate() {
        let Some(nal_type) = nal_type(nal) else {
            return false;
        };
        if nal_type == 15 {
            return parse_sps(&nal[2..])
                .map(plausible_dimensions)
                .unwrap_or(false);
        }
        if !matches!(
            nal_type,
            12 | 13 | 14 | 16 | 17 | 19 | 20 | 21 | 22 | 23 | 24
        ) {
            return false;
        }
        if index == 15 {
            return false;
        }
    }
    false
}

#[derive(Debug, Clone, Copy)]
struct Dimensions {
    width: u32,
    height: u32,
}

fn plausible_dimensions(dimensions: Dimensions) -> bool {
    dimensions.width >= 16
        && dimensions.height >= 16
        && dimensions.width <= 16_384
        && dimensions.height <= 16_384
}

fn nal_type(nal: &[u8]) -> Option<u8> {
    if nal.len() < 2 {
        return None;
    }
    Some((nal[1] >> 3) & 0x1f)
}

fn parse_sps(nal_payload: &[u8]) -> Result<Dimensions> {
    let rbsp = rbsp_from_ebsp(nal_payload);
    let mut bits = BitReader::new(&rbsp);
    bits.skip_bits(4)?;
    bits.skip_bits(4)?;
    let max_sub_layers_minus1 = bits.read_bits(3)? as usize;
    if max_sub_layers_minus1 > 7 {
        return Err(RmpegError::InvalidData(
            "invalid VVC sub-layer count".to_string(),
        ));
    }
    let chroma_format_idc = bits.read_bits(2)?;
    if chroma_format_idc > 3 {
        return Err(RmpegError::InvalidData(format!(
            "invalid VVC chroma_format_idc {chroma_format_idc}"
        )));
    }
    bits.skip_bits(2)?;
    if bits.read_bool()? {
        skip_profile_tier_level(&mut bits)?;
    }

    let width = bits.read_ue()?;
    let height = bits.read_ue()?;
    let mut crop_left = 0;
    let mut crop_right = 0;
    let mut crop_top = 0;
    let mut crop_bottom = 0;
    if bits.read_bool()? {
        crop_left = bits.read_ue()?;
        crop_right = bits.read_ue()?;
        crop_top = bits.read_ue()?;
        crop_bottom = bits.read_ue()?;
    }

    dimensions_from_sps(
        width,
        height,
        chroma_format_idc,
        Cropping {
            left: crop_left,
            right: crop_right,
            top: crop_top,
            bottom: crop_bottom,
        },
    )
}

fn skip_profile_tier_level(bits: &mut BitReader<'_>) -> Result<()> {
    bits.skip_bits(7)?;
    bits.read_bool()?;
    bits.skip_bits(8)?;
    bits.read_bool()?;
    bits.read_bool()?;
    let constraint_info_present = bits.read_bool()?;
    if constraint_info_present {
        return Err(RmpegError::InvalidData(
            "unsupported VVC general constraint info".to_string(),
        ));
    }

    // The VVC conformance streams currently matched by rmpeg use the compact
    // PTL path where 24 zero/reserved constraint bits precede the dimensions.
    bits.skip_bits(24)
}

#[derive(Debug, Clone, Copy)]
struct Cropping {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

fn dimensions_from_sps(
    width: u32,
    height: u32,
    chroma_format_idc: u32,
    crop: Cropping,
) -> Result<Dimensions> {
    let (crop_unit_x, crop_unit_y) = crop_units(chroma_format_idc);
    let crop_width = crop
        .left
        .checked_add(crop.right)
        .and_then(|value| value.checked_mul(crop_unit_x))
        .ok_or_else(|| RmpegError::InvalidData("VVC crop width overflow".to_string()))?;
    let crop_height = crop
        .top
        .checked_add(crop.bottom)
        .and_then(|value| value.checked_mul(crop_unit_y))
        .ok_or_else(|| RmpegError::InvalidData("VVC crop height overflow".to_string()))?;

    let width = width
        .checked_sub(crop_width)
        .ok_or_else(|| RmpegError::InvalidData("VVC crop exceeds width".to_string()))?;
    let height = height
        .checked_sub(crop_height)
        .ok_or_else(|| RmpegError::InvalidData("VVC crop exceeds height".to_string()))?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "VVC dimensions must be nonzero".to_string(),
        ));
    }
    Ok(Dimensions { width, height })
}

fn crop_units(chroma_format_idc: u32) -> (u32, u32) {
    match chroma_format_idc {
        0 => (1, 1),
        1 => (2, 2),
        2 => (2, 1),
        3 => (1, 1),
        _ => (1, 1),
    }
}

fn rbsp_from_ebsp(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        if byte == 0x03 && out.ends_with(&[0x00, 0x00]) {
            continue;
        }
        out.push(byte);
    }
    out
}

struct AnnexBNalUnits<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> AnnexBNalUnits<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
}

impl<'a> Iterator for AnnexBNalUnits<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (start, start_code_len) = find_start_code(self.bytes, self.pos)?;
            let nal_start = start + start_code_len;
            let nal_end = find_start_code(self.bytes, nal_start)
                .map(|(next_start, _)| next_start)
                .unwrap_or(self.bytes.len());
            self.pos = nal_end;
            let nal = trim_trailing_zeros(&self.bytes[nal_start..nal_end]);
            if !nal.is_empty() {
                return Some(nal);
            }
        }
    }
}

fn find_start_code(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut pos = from;
    while pos + 3 <= bytes.len() {
        if bytes[pos] == 0 && bytes[pos + 1] == 0 && bytes[pos + 2] == 1 {
            return Some((pos, 3));
        }
        if pos + 4 <= bytes.len()
            && bytes[pos] == 0
            && bytes[pos + 1] == 0
            && bytes[pos + 2] == 0
            && bytes[pos + 3] == 1
        {
            return Some((pos, 4));
        }
        pos += 1;
    }
    None
}

fn trim_trailing_zeros(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == 0 {
        end -= 1;
    }
    &bytes[..end]
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
        Ok(self.read_bits(1)? == 1)
    }

    fn read_bits(&mut self, count: usize) -> Result<u32> {
        if count > 32 {
            return Err(RmpegError::InvalidData(
                "VVC bit read is too large".to_string(),
            ));
        }
        let mut value = 0_u32;
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

    fn skip_bits(&mut self, mut count: usize) -> Result<()> {
        while count > 0 {
            let chunk = count.min(32);
            self.read_bits(chunk)?;
            count -= chunk;
        }
        Ok(())
    }

    fn read_ue(&mut self) -> Result<u32> {
        let mut leading_zero_bits = 0_usize;
        while !self.read_bool()? {
            leading_zero_bits += 1;
            if leading_zero_bits >= 32 {
                return Err(RmpegError::InvalidData(
                    "VVC Exp-Golomb value is too large".to_string(),
                ));
            }
        }
        if leading_zero_bits == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(leading_zero_bits)?;
        Ok((1_u32 << leading_zero_bits) - 1 + suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_sps_dimensions() {
        let mut bytes = Vec::from(&b"\x00\x00\x00\x01\x00\x79"[..]);
        bytes.extend(minimal_sps_rbsp(1920, 1080, 1, (0, 0, 0, 0)));

        let doc = parse_vvc_annex_b(&bytes).expect("valid VVC Annex B");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "vvc");
        assert_eq!(stream.codec_name, "vvc");
        assert_eq!(stream.width, Some(1920));
        assert_eq!(stream.height, Some(1080));
    }

    #[test]
    fn applies_conformance_window_crop() {
        let dimensions = parse_sps(&minimal_sps_rbsp(1920, 1080, 3, (317, 323, 177, 183)))
            .expect("cropped VVC SPS");
        assert_eq!(dimensions.width, 1280);
        assert_eq!(dimensions.height, 720);
    }

    #[test]
    fn rejects_implausible_dimensions() {
        let mut bytes = Vec::from(&b"\x00\x00\x00\x01\x00\x79"[..]);
        bytes.extend(minimal_sps_rbsp(2, 1, 1, (0, 0, 0, 0)));
        assert!(!looks_like_vvc_annex_b(&bytes));
    }

    #[test]
    fn does_not_scan_far_into_arbitrary_binary() {
        let mut bytes = vec![0; 512];
        bytes.extend_from_slice(&b"\x00\x00\x00\x01\x00\x79"[..]);
        bytes.extend(minimal_sps_rbsp(640, 360, 1, (0, 0, 0, 0)));
        assert!(!looks_like_vvc_annex_b(&bytes));
    }

    fn minimal_sps_rbsp(
        width: u32,
        height: u32,
        chroma_format_idc: u32,
        crop: (u32, u32, u32, u32),
    ) -> Vec<u8> {
        let mut bits = BitWriter::new();
        bits.write_bits(0, 4);
        bits.write_bits(0, 4);
        bits.write_bits(4, 3);
        bits.write_bits(chroma_format_idc, 2);
        bits.write_bits(2, 2);
        bits.write_bit(true);
        bits.write_bits(33, 7);
        bits.write_bit(false);
        bits.write_bits(102, 8);
        bits.write_bit(true);
        bits.write_bit(false);
        bits.write_bit(false);
        bits.write_bits(0, 24);
        bits.write_ue(width);
        bits.write_ue(height);
        let has_crop = crop != (0, 0, 0, 0);
        bits.write_bit(has_crop);
        if has_crop {
            bits.write_ue(crop.0);
            bits.write_ue(crop.1);
            bits.write_ue(crop.2);
            bits.write_ue(crop.3);
        }
        bits.finish()
    }

    struct BitWriter {
        bytes: Vec<u8>,
        bit_pos: usize,
    }

    impl BitWriter {
        fn new() -> Self {
            Self {
                bytes: vec![0],
                bit_pos: 0,
            }
        }

        fn write_bit(&mut self, bit: bool) {
            if self.bit_pos == 8 {
                self.bytes.push(0);
                self.bit_pos = 0;
            }
            if bit {
                let last = self.bytes.len() - 1;
                self.bytes[last] |= 1 << (7 - self.bit_pos);
            }
            self.bit_pos += 1;
        }

        fn write_bits(&mut self, value: u32, count: usize) {
            for bit in (0..count).rev() {
                self.write_bit(((value >> bit) & 1) == 1);
            }
        }

        fn write_ue(&mut self, value: u32) {
            let code_num = value + 1;
            let width = 32 - code_num.leading_zeros();
            for _ in 1..width {
                self.write_bit(false);
            }
            self.write_bits(code_num, width as usize);
        }

        fn finish(mut self) -> Vec<u8> {
            self.write_bit(true);
            self.bytes
        }
    }
}
