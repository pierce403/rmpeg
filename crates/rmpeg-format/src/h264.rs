use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_h264_annex_b(bytes: &[u8]) -> Result<ProbeDocument> {
    for nal in AnnexBNalUnits::new(bytes) {
        if nal.first().map(|header| header & 0x1f) == Some(7) {
            let dimensions = parse_sps(&nal[1..])?;
            return Ok(ProbeDocument {
                format: "h264".to_string(),
                streams: vec![StreamMetadata::video(
                    0,
                    "h264",
                    dimensions.width,
                    dimensions.height,
                    Some(0.0),
                    None,
                )],
            });
        }
    }
    Err(RmpegError::InvalidData("missing H.264 SPS".to_string()))
}

pub fn looks_like_h264_annex_b(bytes: &[u8]) -> bool {
    if find_start_code(bytes, 0)
        .map(|(pos, _)| pos > 64)
        .unwrap_or(true)
    {
        return false;
    }

    for (index, nal) in AnnexBNalUnits::new(bytes).take(8).enumerate() {
        let Some(nal_type) = nal.first().map(|header| header & 0x1f) else {
            return false;
        };
        if nal_type == 7 {
            return parse_sps(&nal[1..]).is_ok();
        }
        if !matches!(nal_type, 6 | 8 | 9 | 12) {
            return false;
        }
        if index == 7 {
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

fn parse_sps(nal_payload: &[u8]) -> Result<Dimensions> {
    let rbsp = rbsp_from_ebsp(nal_payload);
    let mut bits = BitReader::new(&rbsp);
    let profile_idc = bits.read_bits(8)?;
    bits.read_bits(8)?;
    bits.read_bits(8)?;
    bits.read_ue()?;

    let mut chroma_format_idc = 1;
    let mut separate_colour_plane_flag = false;
    if uses_high_profile_syntax(profile_idc) {
        chroma_format_idc = bits.read_ue()?;
        if chroma_format_idc > 3 {
            return Err(RmpegError::InvalidData(format!(
                "invalid H.264 chroma_format_idc {chroma_format_idc}"
            )));
        }
        if chroma_format_idc == 3 {
            separate_colour_plane_flag = bits.read_bool()?;
        }
        bits.read_ue()?;
        bits.read_ue()?;
        bits.read_bool()?;
        if bits.read_bool()? {
            skip_scaling_matrices(&mut bits, chroma_format_idc)?;
        }
    }

    bits.read_ue()?;
    let pic_order_cnt_type = bits.read_ue()?;
    match pic_order_cnt_type {
        0 => {
            bits.read_ue()?;
        }
        1 => {
            bits.read_bool()?;
            bits.read_se()?;
            bits.read_se()?;
            let cycle_count = bits.read_ue()?;
            for _ in 0..cycle_count {
                bits.read_se()?;
            }
        }
        _ => {}
    }
    bits.read_ue()?;
    bits.read_bool()?;
    let pic_width_in_mbs_minus1 = bits.read_ue()?;
    let pic_height_in_map_units_minus1 = bits.read_ue()?;
    let frame_mbs_only_flag = bits.read_bool()?;
    if !frame_mbs_only_flag {
        bits.read_bool()?;
    }
    bits.read_bool()?;

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
        pic_width_in_mbs_minus1,
        pic_height_in_map_units_minus1,
        frame_mbs_only_flag,
        chroma_format_idc,
        separate_colour_plane_flag,
        Cropping {
            left: crop_left,
            right: crop_right,
            top: crop_top,
            bottom: crop_bottom,
        },
    )
}

fn uses_high_profile_syntax(profile_idc: u32) -> bool {
    matches!(
        profile_idc,
        44 | 83 | 86 | 100 | 110 | 118 | 122 | 128 | 134 | 135 | 138 | 139 | 244
    )
}

fn skip_scaling_matrices(bits: &mut BitReader<'_>, chroma_format_idc: u32) -> Result<()> {
    let count = if chroma_format_idc == 3 { 12 } else { 8 };
    for index in 0..count {
        if bits.read_bool()? {
            let size = if index < 6 { 16 } else { 64 };
            skip_scaling_list(bits, size)?;
        }
    }
    Ok(())
}

fn skip_scaling_list(bits: &mut BitReader<'_>, size: usize) -> Result<()> {
    let mut last_scale = 8_i32;
    let mut next_scale = 8_i32;
    for _ in 0..size {
        if next_scale != 0 {
            let delta_scale = bits.read_se()?;
            next_scale = (last_scale + delta_scale + 256) % 256;
        }
        if next_scale != 0 {
            last_scale = next_scale;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Cropping {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

fn dimensions_from_sps(
    pic_width_in_mbs_minus1: u32,
    pic_height_in_map_units_minus1: u32,
    frame_mbs_only_flag: bool,
    chroma_format_idc: u32,
    separate_colour_plane_flag: bool,
    crop: Cropping,
) -> Result<Dimensions> {
    let width = checked_add_one(pic_width_in_mbs_minus1, "H.264 macroblock width")?
        .checked_mul(16)
        .ok_or_else(|| RmpegError::InvalidData("H.264 width overflow".to_string()))?;
    let frame_height_factor = if frame_mbs_only_flag { 1 } else { 2 };
    let height = checked_add_one(pic_height_in_map_units_minus1, "H.264 macroblock height")?
        .checked_mul(16)
        .and_then(|value| value.checked_mul(frame_height_factor))
        .ok_or_else(|| RmpegError::InvalidData("H.264 height overflow".to_string()))?;

    let (crop_unit_x, crop_unit_y) = crop_units(
        chroma_format_idc,
        separate_colour_plane_flag,
        frame_mbs_only_flag,
    );
    let crop_width = crop
        .left
        .checked_add(crop.right)
        .and_then(|value| value.checked_mul(crop_unit_x))
        .ok_or_else(|| RmpegError::InvalidData("H.264 crop width overflow".to_string()))?;
    let crop_height = crop
        .top
        .checked_add(crop.bottom)
        .and_then(|value| value.checked_mul(crop_unit_y))
        .ok_or_else(|| RmpegError::InvalidData("H.264 crop height overflow".to_string()))?;

    let width = width
        .checked_sub(crop_width)
        .ok_or_else(|| RmpegError::InvalidData("H.264 crop exceeds width".to_string()))?;
    let height = height
        .checked_sub(crop_height)
        .ok_or_else(|| RmpegError::InvalidData("H.264 crop exceeds height".to_string()))?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "H.264 dimensions must be nonzero".to_string(),
        ));
    }
    Ok(Dimensions { width, height })
}

fn checked_add_one(value: u32, label: &str) -> Result<u32> {
    value
        .checked_add(1)
        .ok_or_else(|| RmpegError::InvalidData(format!("{label} overflow")))
}

fn crop_units(
    chroma_format_idc: u32,
    separate_colour_plane_flag: bool,
    frame_mbs_only_flag: bool,
) -> (u32, u32) {
    if chroma_format_idc == 0 || separate_colour_plane_flag {
        return (1, if frame_mbs_only_flag { 1 } else { 2 });
    }
    let (sub_width_c, sub_height_c) = match chroma_format_idc {
        1 => (2, 2),
        2 => (2, 1),
        3 => (1, 1),
        _ => (1, 1),
    };
    (
        sub_width_c,
        sub_height_c * if frame_mbs_only_flag { 1 } else { 2 },
    )
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
                "H.264 bit read is too large".to_string(),
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

    fn read_ue(&mut self) -> Result<u32> {
        let mut leading_zero_bits = 0_usize;
        while !self.read_bool()? {
            leading_zero_bits += 1;
            if leading_zero_bits >= 32 {
                return Err(RmpegError::InvalidData(
                    "H.264 Exp-Golomb value is too large".to_string(),
                ));
            }
        }
        if leading_zero_bits == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(leading_zero_bits)?;
        Ok((1_u32 << leading_zero_bits) - 1 + suffix)
    }

    fn read_se(&mut self) -> Result<i32> {
        let code_num = self.read_ue()?;
        let magnitude = code_num.div_ceil(2) as i32;
        if code_num % 2 == 0 {
            Ok(-magnitude)
        } else {
            Ok(magnitude)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_baseline_sps_dimensions() {
        let rbsp = baseline_sps_rbsp(10, 8);
        let mut bytes = Vec::from(&b"\x00\x00\x00\x01\x67"[..]);
        bytes.extend(rbsp);
        let doc = parse_h264_annex_b(&bytes).expect("valid H.264 Annex B");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "h264");
        assert_eq!(stream.codec_name, "h264");
        assert_eq!(stream.width, Some(176));
        assert_eq!(stream.height, Some(144));
    }

    #[test]
    fn ignores_emulation_prevention_bytes() {
        let rbsp = rbsp_from_ebsp(&[0, 0, 3, 1, 0, 0, 3, 2]);
        assert_eq!(rbsp, vec![0, 0, 1, 0, 0, 2]);
    }

    #[test]
    fn does_not_scan_far_into_arbitrary_binary() {
        let mut bytes = vec![0; 512];
        bytes.extend_from_slice(&b"\x00\x00\x00\x01\x67"[..]);
        bytes.extend(baseline_sps_rbsp(10, 8));
        assert!(!looks_like_h264_annex_b(&bytes));
    }

    fn baseline_sps_rbsp(width_mbs_minus1: u32, height_map_units_minus1: u32) -> Vec<u8> {
        let mut bits = BitWriter::new();
        bits.write_bits(66, 8);
        bits.write_bits(0, 8);
        bits.write_bits(10, 8);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(1);
        bits.write_bit(false);
        bits.write_ue(width_mbs_minus1);
        bits.write_ue(height_map_units_minus1);
        bits.write_bit(true);
        bits.write_bit(true);
        bits.write_bit(false);
        bits.write_bit(false);
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
