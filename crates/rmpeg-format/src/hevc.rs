use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_hevc_annex_b(bytes: &[u8]) -> Result<ProbeDocument> {
    for nal in AnnexBNalUnits::new(bytes) {
        if nal_type(nal) == Some(33) {
            let metadata = parse_sps(&nal[2..])?;
            let dimensions = metadata.visible_dimensions();
            return Ok(ProbeDocument {
                format: "hevc".to_string(),
                streams: vec![StreamMetadata::video(
                    0,
                    "hevc",
                    dimensions.width,
                    dimensions.height,
                    Some(0.0),
                    None,
                )],
            });
        }
    }
    Err(RmpegError::InvalidData("missing HEVC SPS".to_string()))
}

pub fn looks_like_hevc_annex_b(bytes: &[u8]) -> bool {
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
        if nal_type == 33 {
            return parse_sps(&nal[2..]).is_ok();
        }
        if !matches!(nal_type, 32 | 34 | 35 | 36 | 37 | 38 | 39 | 40) {
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

#[derive(Debug, Clone, Copy)]
struct SpsMetadata {
    dimensions: Dimensions,
    usable_dimensions: bool,
}

impl SpsMetadata {
    fn visible_dimensions(self) -> Dimensions {
        if self.usable_dimensions {
            self.dimensions
        } else {
            Dimensions {
                width: 0,
                height: 0,
            }
        }
    }
}

fn nal_type(nal: &[u8]) -> Option<u8> {
    if nal.len() < 2 {
        return None;
    }
    Some((nal[0] >> 1) & 0x3f)
}

fn parse_sps(nal_payload: &[u8]) -> Result<SpsMetadata> {
    let rbsp = rbsp_from_ebsp(nal_payload);
    let mut bits = BitReader::new(&rbsp);
    bits.read_bits(4)?;
    let max_sub_layers_minus1 = bits.read_bits(3)? as usize;
    if max_sub_layers_minus1 > 6 {
        return Err(RmpegError::InvalidData(
            "invalid HEVC sub-layer count".to_string(),
        ));
    }
    bits.read_bool()?;
    skip_profile_tier_level(&mut bits, max_sub_layers_minus1)?;
    bits.read_ue()?;
    let chroma_format_idc = bits.read_ue()?;
    if chroma_format_idc > 3 {
        return Err(RmpegError::InvalidData(format!(
            "invalid HEVC chroma_format_idc {chroma_format_idc}"
        )));
    }
    let separate_colour_plane_flag = chroma_format_idc == 3 && bits.read_bool()?;
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

    let dimensions = dimensions_from_sps(
        width,
        height,
        chroma_format_idc,
        separate_colour_plane_flag,
        Cropping {
            left: crop_left,
            right: crop_right,
            top: crop_top,
            bottom: crop_bottom,
        },
    )?;
    let usable_dimensions = validate_sps_tail(&mut bits, max_sub_layers_minus1).is_ok();
    Ok(SpsMetadata {
        dimensions,
        usable_dimensions,
    })
}

fn validate_sps_tail(bits: &mut BitReader<'_>, max_sub_layers_minus1: usize) -> Result<()> {
    let bit_depth_luma_minus8 = bits.read_ue()?;
    let bit_depth_chroma_minus8 = bits.read_ue()?;
    if bit_depth_luma_minus8 != bit_depth_chroma_minus8 {
        return Err(RmpegError::InvalidData(
            "HEVC luma/chroma bit depth mismatch".to_string(),
        ));
    }
    let log2_max_pic_order_cnt_lsb_minus4 = bits.read_ue()?;

    let ordering_info_starts_at = if bits.read_bool()? {
        0
    } else {
        max_sub_layers_minus1
    };
    for _ in ordering_info_starts_at..=max_sub_layers_minus1 {
        bits.read_ue()?;
        bits.read_ue()?;
        bits.read_ue()?;
    }
    bits.read_ue()?;
    bits.read_ue()?;
    bits.read_ue()?;
    bits.read_ue()?;
    bits.read_ue()?;
    bits.read_ue()?;
    if bits.read_bool()? && bits.read_bool()? {
        return Ok(());
    }
    bits.read_bool()?;
    bits.read_bool()?;
    if bits.read_bool()? {
        bits.skip_bits(4)?;
        bits.skip_bits(4)?;
        bits.read_ue()?;
        bits.read_ue()?;
        bits.read_bool()?;
    }

    let num_short_term_ref_pic_sets = bits.read_ue()?;
    if num_short_term_ref_pic_sets != 0 {
        return Ok(());
    }
    if bits.read_bool()? {
        let num_long_term_ref_pics_sps = bits.read_ue()?;
        let poc_lsb_bits = usize::try_from(
            log2_max_pic_order_cnt_lsb_minus4
                .checked_add(4)
                .ok_or_else(|| RmpegError::InvalidData("HEVC POC width overflow".to_string()))?,
        )
        .map_err(|_| RmpegError::InvalidData("HEVC POC width is too large".to_string()))?;
        for _ in 0..num_long_term_ref_pics_sps {
            bits.skip_bits(poc_lsb_bits)?;
            bits.read_bool()?;
        }
    }
    bits.read_bool()?;
    bits.read_bool()?;
    if bits.read_bool()? && !skip_vui_parameters(bits)? {
        return Ok(());
    }
    if bits.read_bool()? {
        let range_extension = bits.read_bool()?;
        let multilayer_extension = bits.read_bool()?;
        let three_d_extension = bits.read_bool()?;
        let scc_extension = bits.read_bool()?;
        let extension_bits = bits.read_bits(4)?;
        if (range_extension
            || multilayer_extension
            || three_d_extension
            || scc_extension
            || extension_bits != 0)
            && bits.remaining_bits_look_like_trailing()
        {
            return Err(RmpegError::InvalidData(
                "HEVC SPS extension has no payload".to_string(),
            ));
        }
    }
    Ok(())
}

fn skip_vui_parameters(bits: &mut BitReader<'_>) -> Result<bool> {
    if bits.read_bool()? {
        let aspect_ratio_idc = bits.read_bits(8)?;
        if aspect_ratio_idc == 255 {
            bits.skip_bits(16)?;
            bits.skip_bits(16)?;
        }
    }
    if bits.read_bool()? {
        bits.read_bool()?;
    }
    if bits.read_bool()? {
        bits.skip_bits(3)?;
        bits.read_bool()?;
        if bits.read_bool()? {
            bits.skip_bits(8)?;
            bits.skip_bits(8)?;
            bits.skip_bits(8)?;
        }
    }
    if bits.read_bool()? {
        bits.read_ue()?;
        bits.read_ue()?;
    }
    bits.read_bool()?;
    bits.read_bool()?;
    bits.read_bool()?;
    if bits.read_bool()? {
        bits.read_ue()?;
        bits.read_ue()?;
        bits.read_ue()?;
        bits.read_ue()?;
    }
    if bits.read_bool()? {
        bits.skip_bits(32)?;
        bits.skip_bits(32)?;
        if bits.read_bool()? {
            bits.read_ue()?;
        }
        if bits.read_bool()? {
            return Ok(false);
        }
    }
    if bits.read_bool()? {
        bits.read_bool()?;
        bits.read_bool()?;
        bits.read_bool()?;
        bits.read_ue()?;
        bits.read_ue()?;
        bits.read_ue()?;
        bits.read_ue()?;
        bits.read_ue()?;
    }
    Ok(true)
}

fn skip_profile_tier_level(bits: &mut BitReader<'_>, max_sub_layers_minus1: usize) -> Result<()> {
    bits.skip_bits(2 + 1 + 5)?;
    bits.skip_bits(32)?;
    bits.skip_bits(48)?;
    bits.skip_bits(8)?;

    let mut sub_layer_profile_present = [false; 6];
    let mut sub_layer_level_present = [false; 6];
    for index in 0..max_sub_layers_minus1 {
        sub_layer_profile_present[index] = bits.read_bool()?;
        sub_layer_level_present[index] = bits.read_bool()?;
    }
    if max_sub_layers_minus1 > 0 {
        for _ in max_sub_layers_minus1..8 {
            bits.skip_bits(2)?;
        }
    }
    for index in 0..max_sub_layers_minus1 {
        if sub_layer_profile_present[index] {
            bits.skip_bits(2 + 1 + 5)?;
            bits.skip_bits(32)?;
            bits.skip_bits(48)?;
        }
        if sub_layer_level_present[index] {
            bits.skip_bits(8)?;
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
    width: u32,
    height: u32,
    chroma_format_idc: u32,
    separate_colour_plane_flag: bool,
    crop: Cropping,
) -> Result<Dimensions> {
    let (crop_unit_x, crop_unit_y) = crop_units(chroma_format_idc, separate_colour_plane_flag);
    let crop_width = crop
        .left
        .checked_add(crop.right)
        .and_then(|value| value.checked_mul(crop_unit_x))
        .ok_or_else(|| RmpegError::InvalidData("HEVC crop width overflow".to_string()))?;
    let crop_height = crop
        .top
        .checked_add(crop.bottom)
        .and_then(|value| value.checked_mul(crop_unit_y))
        .ok_or_else(|| RmpegError::InvalidData("HEVC crop height overflow".to_string()))?;

    let width = width
        .checked_sub(crop_width)
        .ok_or_else(|| RmpegError::InvalidData("HEVC crop exceeds width".to_string()))?;
    let height = height
        .checked_sub(crop_height)
        .ok_or_else(|| RmpegError::InvalidData("HEVC crop exceeds height".to_string()))?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "HEVC dimensions must be nonzero".to_string(),
        ));
    }
    Ok(Dimensions { width, height })
}

fn crop_units(chroma_format_idc: u32, separate_colour_plane_flag: bool) -> (u32, u32) {
    if chroma_format_idc == 0 || separate_colour_plane_flag {
        return (1, 1);
    }
    match chroma_format_idc {
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
                "HEVC bit read is too large".to_string(),
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
                    "HEVC Exp-Golomb value is too large".to_string(),
                ));
            }
        }
        if leading_zero_bits == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(leading_zero_bits)?;
        Ok((1_u32 << leading_zero_bits) - 1 + suffix)
    }

    fn remaining_bits_look_like_trailing(&self) -> bool {
        let total_bits = self.bytes.len() * 8;
        if self.bit_pos >= total_bits {
            return true;
        }
        let mut seen_stop_bit = false;
        for bit_pos in self.bit_pos..total_bits {
            let byte = self.bytes[bit_pos / 8];
            let bit = (byte >> (7 - (bit_pos % 8))) & 1;
            if seen_stop_bit {
                if bit != 0 {
                    return false;
                }
            } else if bit == 1 {
                seen_stop_bit = true;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_sps_dimensions() {
        let mut bytes = Vec::from(&b"\x00\x00\x00\x01\x40\x01"[..]);
        bytes.extend([0x01, 0x02, 0x03]);
        bytes.extend_from_slice(&b"\x00\x00\x00\x01\x42\x01"[..]);
        bytes.extend(minimal_sps_rbsp(1920, 1080));

        let doc = parse_hevc_annex_b(&bytes).expect("valid HEVC Annex B");
        let stream = &doc.streams[0];
        assert_eq!(doc.format, "hevc");
        assert_eq!(stream.codec_name, "hevc");
        assert_eq!(stream.width, Some(1920));
        assert_eq!(stream.height, Some(1080));
    }

    #[test]
    fn applies_conformance_window_crop() {
        let rbsp = minimal_sps_rbsp_with_crop(1920, 1088, 0, 0, 0, 4);
        let dimensions = parse_sps(&rbsp)
            .expect("cropped HEVC SPS")
            .visible_dimensions();
        assert_eq!(dimensions.width, 1920);
        assert_eq!(dimensions.height, 1080);
    }

    #[test]
    fn reports_zero_dimensions_for_unsupported_sps_tail() {
        let mut bytes = Vec::from(&b"\x00\x00\x00\x01\x42\x01"[..]);
        bytes.extend(minimal_sps_rbsp_with_bit_depths(1024, 768, 2, 1));

        let doc = parse_hevc_annex_b(&bytes).expect("accepted HEVC stream");
        let stream = &doc.streams[0];
        assert_eq!(stream.width, Some(0));
        assert_eq!(stream.height, Some(0));
    }

    #[test]
    fn reports_zero_dimensions_for_empty_sps_extension_payload() {
        let mut bytes = Vec::from(&b"\x00\x00\x00\x01\x42\x01"[..]);
        bytes.extend(minimal_sps_rbsp_with_empty_extension_payload(416, 240));

        let doc = parse_hevc_annex_b(&bytes).expect("accepted HEVC stream");
        let stream = &doc.streams[0];
        assert_eq!(stream.width, Some(0));
        assert_eq!(stream.height, Some(0));
    }

    #[test]
    fn rejects_annex_b_without_sps() {
        let bytes = b"\x00\x00\x00\x01\x40\x01\x01\x02\x03\x00\x00\x00\x01\x44\x01";
        assert!(!looks_like_hevc_annex_b(bytes));
    }

    #[test]
    fn does_not_scan_far_into_arbitrary_binary() {
        let mut bytes = vec![0; 512];
        bytes.extend_from_slice(&b"\x00\x00\x00\x01\x42\x01"[..]);
        bytes.extend(minimal_sps_rbsp(640, 360));
        assert!(!looks_like_hevc_annex_b(&bytes));
    }

    fn minimal_sps_rbsp(width: u32, height: u32) -> Vec<u8> {
        minimal_sps_rbsp_with_crop_and_bit_depths(width, height, (0, 0, 0, 0), 0, 0)
    }

    fn minimal_sps_rbsp_with_crop(
        width: u32,
        height: u32,
        crop_left: u32,
        crop_right: u32,
        crop_top: u32,
        crop_bottom: u32,
    ) -> Vec<u8> {
        minimal_sps_rbsp_with_crop_and_bit_depths(
            width,
            height,
            (crop_left, crop_right, crop_top, crop_bottom),
            0,
            0,
        )
    }

    fn minimal_sps_rbsp_with_bit_depths(
        width: u32,
        height: u32,
        luma_minus8: u32,
        chroma_minus8: u32,
    ) -> Vec<u8> {
        minimal_sps_rbsp_with_crop_and_bit_depths(
            width,
            height,
            (0, 0, 0, 0),
            luma_minus8,
            chroma_minus8,
        )
    }

    fn minimal_sps_rbsp_with_crop_and_bit_depths(
        width: u32,
        height: u32,
        crop: (u32, u32, u32, u32),
        luma_minus8: u32,
        chroma_minus8: u32,
    ) -> Vec<u8> {
        let (crop_left, crop_right, crop_top, crop_bottom) = crop;
        let mut bits = BitWriter::new();
        bits.write_bits(0, 4);
        bits.write_bits(0, 3);
        bits.write_bit(true);
        bits.write_bits(0, 2);
        bits.write_bit(false);
        bits.write_bits(1, 5);
        bits.write_bits(0, 32);
        bits.write_bits(0, 48);
        bits.write_bits(120, 8);
        bits.write_ue(0);
        bits.write_ue(1);
        bits.write_ue(width);
        bits.write_ue(height);
        let has_crop = crop_left != 0 || crop_right != 0 || crop_top != 0 || crop_bottom != 0;
        bits.write_bit(has_crop);
        if has_crop {
            bits.write_ue(crop_left);
            bits.write_ue(crop_right);
            bits.write_ue(crop_top);
            bits.write_ue(crop_bottom);
        }
        bits.write_ue(luma_minus8);
        bits.write_ue(chroma_minus8);
        bits.write_ue(4);
        bits.write_bit(false);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(0);
        write_minimal_sps_tail_after_ordering(&mut bits);
        bits.finish()
    }

    fn minimal_sps_rbsp_with_empty_extension_payload(width: u32, height: u32) -> Vec<u8> {
        let mut bits = BitWriter::new();
        bits.write_bits(0, 4);
        bits.write_bits(0, 3);
        bits.write_bit(true);
        bits.write_bits(0, 2);
        bits.write_bit(false);
        bits.write_bits(1, 5);
        bits.write_bits(0, 32);
        bits.write_bits(0, 48);
        bits.write_bits(120, 8);
        bits.write_ue(0);
        bits.write_ue(1);
        bits.write_ue(width);
        bits.write_ue(height);
        bits.write_bit(false);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(4);
        bits.write_bit(false);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(0);
        write_minimal_sps_tail_prefix_after_ordering(&mut bits);
        bits.write_bit(false);
        bits.write_bit(true);
        bits.write_bit(true);
        bits.write_bit(false);
        bits.write_bit(false);
        bits.write_bit(false);
        bits.write_bits(0, 4);
        bits.finish()
    }

    fn write_minimal_sps_tail_after_ordering(bits: &mut BitWriter) {
        write_minimal_sps_tail_prefix_after_ordering(bits);
        bits.write_bit(false);
        bits.write_bit(false);
    }

    fn write_minimal_sps_tail_prefix_after_ordering(bits: &mut BitWriter) {
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_ue(0);
        bits.write_bit(false);
        bits.write_bit(false);
        bits.write_bit(true);
        bits.write_bit(false);
        bits.write_ue(0);
        bits.write_bit(false);
        bits.write_bit(false);
        bits.write_bit(true);
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

        fn write_bits(&mut self, value: u64, count: usize) {
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
            self.write_bits(u64::from(code_num), width as usize);
        }

        fn finish(mut self) -> Vec<u8> {
            self.write_bit(true);
            self.bytes
        }
    }
}
