use std::io::Cursor;

use rmpeg_core::{AudioFrameHash, Result, RmpegError};

use crate::md5::md5_hex;

const ALIAS_PIX_HEADER_LEN: usize = 10;
const BRENDER_PIX_DATA_OFFSET: usize = 54;
const BRENDER_PIX_INDEX_MARKER: &[u8; 13] = b"\x21\x00\x01\x00\x08\x00\x01\x00\x00\x00\x00\x00\x01";
const BRENDER_PIX_PALETTE_MARKER: &[u8; 14] =
    b"\x21\x00\x00\x04\x08\x00\x00\x01\x00\x00\x00\x00\x04\x00";
const BRENDER_PIX_GRAY64: [u8; 64] = [
    0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 30, 33, 36, 39, 42, 45, 49, 52, 55, 58, 61, 64, 67, 70, 73,
    76, 79, 82, 85, 88, 91, 94, 98, 101, 104, 107, 110, 113, 116, 119, 122, 125, 128, 131, 134,
    137, 140, 143, 147, 153, 160, 167, 174, 180, 187, 194, 201, 207, 214, 221, 228, 234, 241, 248,
];
const BRENDER_PIX_RAMP32: [u8; 32] = [
    0, 9, 19, 29, 39, 49, 59, 69, 79, 89, 99, 109, 119, 128, 138, 148, 158, 168, 178, 188, 198,
    208, 218, 228, 238, 240, 242, 244, 246, 248, 250, 252,
];
const BRENDER_PIX_DIM32: [u8; 32] = [
    0, 2, 5, 7, 10, 12, 15, 17, 20, 22, 24, 27, 30, 32, 34, 37, 40, 42, 45, 47, 49, 52, 55, 57, 60,
    84, 108, 133, 157, 181, 206, 230,
];
const BMP_FILE_HEADER_LEN: usize = 14;
const BMP_PALETTE_BYTES: usize = 256 * 4;
const BI_RGB: u32 = 0;
const BI_RLE8: u32 = 1;
const BI_RLE4: u32 = 2;
const BI_BITFIELDS: u32 = 3;
const DDS_HEADER_LEN: usize = 128;
const DDS_PALETTE_BYTES: usize = 256 * 4;
const DDS_PIXEL_FORMAT_LEN: u32 = 32;
const DDS_EXPAND_5_TO_8: [u8; 32] = [
    0, 8, 16, 25, 33, 41, 49, 58, 66, 74, 82, 90, 99, 107, 115, 123, 132, 140, 148, 156, 164, 173,
    181, 189, 197, 205, 214, 222, 230, 238, 247, 255,
];
const DDS_EXPAND_6_TO_8: [u8; 64] = [
    0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 45, 49, 53, 57, 61, 65, 69, 73, 77, 81, 85, 89, 93,
    97, 101, 105, 109, 113, 117, 121, 125, 130, 134, 138, 142, 146, 150, 154, 158, 162, 166, 170,
    174, 178, 182, 186, 190, 194, 198, 202, 206, 210, 214, 219, 223, 227, 231, 235, 239, 243, 247,
    251, 255,
];
const DDPF_ALPHAPIXELS: u32 = 0x0000_0001;
const DDPF_ALPHA: u32 = 0x0000_0002;
const DDPF_FOURCC: u32 = 0x0000_0004;
const DDPF_PALETTEINDEXED8: u32 = 0x0000_0020;
const DDPF_RGB: u32 = 0x0000_0040;
const DDPF_LUMINANCE: u32 = 0x0002_0000;
const DPX_HEADER_MIN_LEN: usize = 812;
const DPX_DESCRIPTOR_RGB: u8 = 50;
const FITS_CARD_LEN: usize = 80;
const FITS_BLOCK_LEN: usize = 2880;
const PTX_HEADER_LEN: usize = 44;
const TGA_HEADER_LEN: usize = 18;
const TGA_FOOTER_LEN: usize = 26;
const TGA_FOOTER_SIGNATURE: &[u8; 18] = b"TRUEVISION-XFILE.\0";
const SUNRAST_HEADER_LEN: usize = 32;
const SUNRAST_MAGIC: &[u8; 4] = b"\x59\xa6\x6a\x95";
const SUNRAST_TYPE_OLD: u32 = 0;
const SUNRAST_TYPE_STANDARD: u32 = 1;
const SUNRAST_TYPE_BYTE_ENCODED: u32 = 2;
const SUNRAST_MAP_NONE: u32 = 0;
const SUNRAST_MAP_RGB: u32 = 1;
const SGI_HEADER_LEN: usize = 512;
const SGI_STORAGE_VERBATIM: u8 = 0;
const SGI_STORAGE_RLE: u8 = 1;

pub fn png_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let decoder = png::Decoder::new(Cursor::new(input));
    let mut reader = decoder
        .read_info()
        .map_err(|error| RmpegError::InvalidData(error.to_string()))?;
    let output_size = reader
        .output_buffer_size()
        .ok_or_else(|| RmpegError::InvalidData("PNG output size is unavailable".to_string()))?;
    let mut output = vec![0; output_size];
    let info = reader
        .next_frame(&mut output)
        .map_err(|error| RmpegError::InvalidData(error.to_string()))?;

    let frame = &output[..info.buffer_size()];
    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(frame),
    }])
}

pub fn pnm_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let header = pnm_header(input)?;
    let frame = pnm_frame(input, &header)?;

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    }])
}

pub fn ptx_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let header = ptx_header(input)?;
    let data_end = PTX_HEADER_LEN
        .checked_add(header.frame_size)
        .ok_or_else(|| RmpegError::InvalidData("PTX frame range overflow".to_string()))?;
    let frame = &input[PTX_HEADER_LEN..data_end];

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(frame),
    }])
}

pub fn brender_pix_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let header = brender_pix_header(input)?;
    let data_end = header
        .frame_offset
        .checked_add(header.frame_size)
        .ok_or_else(|| RmpegError::InvalidData("BRender PIX frame range overflow".to_string()))?;
    let mut frame = input[header.frame_offset..data_end].to_vec();
    if let Some(palette) = header.palette {
        frame.extend_from_slice(&palette);
    }

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    }])
}

pub fn dds_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let header = dds_header(input)?;
    let data_end = header
        .frame_offset
        .checked_add(header.frame_size)
        .ok_or_else(|| RmpegError::InvalidData("DDS frame range overflow".to_string()))?;
    let frame = match header.postprocess {
        DdsPostprocess::None => input[header.frame_offset..data_end].to_vec(),
        DdsPostprocess::Palette { palette_offset } => {
            let palette_end = palette_offset
                .checked_add(DDS_PALETTE_BYTES)
                .ok_or_else(|| RmpegError::InvalidData("DDS palette range overflow".to_string()))?;
            let mut frame = input[header.frame_offset..data_end].to_vec();
            for rgba in input[palette_offset..palette_end].chunks_exact(4) {
                frame.extend_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
            }
            frame
        }
        DdsPostprocess::Aexp => dds_aexp_frame(&input[header.frame_offset..data_end]),
        DdsPostprocess::Ycocg => dds_ycocg_frame(&input[header.frame_offset..data_end]),
        DdsPostprocess::Bc1 { normal_map } => dds_bc1_frame(
            &input[header.frame_offset..data_end],
            header.width,
            header.height,
            normal_map,
        )?,
        DdsPostprocess::Bc2 { premultiplied } => dds_bc2_frame(
            &input[header.frame_offset..data_end],
            header.width,
            header.height,
            premultiplied,
        )?,
        DdsPostprocess::Bc3 {
            premultiplied,
            transform,
        } => dds_bc3_frame(
            &input[header.frame_offset..data_end],
            header.width,
            header.height,
            premultiplied,
            transform,
        )?,
        DdsPostprocess::Bc4 { signed } => dds_bc4_frame(
            &input[header.frame_offset..data_end],
            header.width,
            header.height,
            signed,
        )?,
        DdsPostprocess::Bc5 { signed, swap_xy } => dds_bc5_frame(
            &input[header.frame_offset..data_end],
            header.width,
            header.height,
            signed,
            swap_xy,
        )?,
    };

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    }])
}

pub fn alias_pix_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    if input.len() < ALIAS_PIX_HEADER_LEN {
        return Err(RmpegError::UnexpectedEof {
            needed: ALIAS_PIX_HEADER_LEN,
            remaining: input.len(),
        });
    }
    if input[4..8] != [0, 0, 0, 0] {
        return Err(RmpegError::InvalidData(
            "missing Alias PIX reserved header bytes".to_string(),
        ));
    }

    let width = u32::from(read_u16_be(input, 0)?);
    let height = u32::from(read_u16_be(input, 2)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "Alias PIX dimensions must be nonzero".to_string(),
        ));
    }

    let bits_per_pixel = read_u16_be(input, 8)?;
    let bytes_per_pixel = match bits_per_pixel {
        8 => 1,
        24 => 3,
        _ => {
            return Err(RmpegError::Unsupported(format!(
                "unsupported Alias PIX bit depth {bits_per_pixel}"
            )));
        }
    };
    let pixels = checked_pixels(width, height, "Alias PIX")?;
    let frame_size = pixels
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| RmpegError::InvalidData("Alias PIX frame size overflow".to_string()))?;
    let frame = alias_pix_rle_frame(
        &input[ALIAS_PIX_HEADER_LEN..],
        pixels,
        bytes_per_pixel,
        frame_size,
    )?;

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    }])
}

pub fn bmp_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let mut frames = Vec::new();
    let mut pos = 0_usize;
    while pos < input.len() {
        let file_size = usize::try_from(read_u32_le(input, pos + 2)?)
            .map_err(|_| RmpegError::Unsupported("BMP file size is too large".to_string()))?;
        let pixel_offset = usize::try_from(read_u32_le(input, pos + 10)?)
            .map_err(|_| RmpegError::Unsupported("BMP pixel offset is too large".to_string()))?;
        let end = if file_size >= pixel_offset && pos + file_size <= input.len() {
            pos + file_size
        } else if pos == 0 {
            input.len()
        } else {
            return Err(RmpegError::InvalidData(
                "truncated concatenated BMP frame".to_string(),
            ));
        };
        let frame = bmp_frame(&input[pos..end])?;
        let frame_index = frames.len() as u64;
        frames.push(AudioFrameHash {
            stream_index: 0,
            dts: frame_index,
            pts: frame_index,
            duration: 1,
            size: frame.len(),
            hash: md5_hex(&frame),
        });
        pos = end;
    }
    if frames.is_empty() {
        return Err(RmpegError::InvalidData("empty BMP input".to_string()));
    }
    Ok(frames)
}

pub fn dpx_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let mut frames = Vec::new();
    let mut stream_shape = None;
    let mut pos = 0_usize;
    while pos < input.len() {
        let header = dpx_header(input, pos)?;
        let shape = (
            header.width,
            header.height,
            header.bit_depth,
            header.packing,
        );
        if let Some(stream_shape) = stream_shape {
            if stream_shape != shape {
                return Err(RmpegError::Unsupported(
                    "variable-dimension DPX sequences require exact compatibility backend"
                        .to_string(),
                ));
            }
        } else {
            stream_shape = Some(shape);
        }
        let frame = dpx_frame(input, pos, &header)?;
        let frame_index = frames.len() as u64;
        frames.push(AudioFrameHash {
            stream_index: 0,
            dts: frame_index,
            pts: frame_index,
            duration: 1,
            size: frame.len(),
            hash: md5_hex(&frame),
        });
        let required_end = header
            .data_offset
            .checked_add(header.data_bytes)
            .ok_or_else(|| RmpegError::InvalidData("DPX frame range overflow".to_string()))?;
        let next_pos = if header.file_size >= required_end && pos + header.file_size <= input.len()
        {
            pos + header.file_size
        } else {
            pos + required_end
        };
        if next_pos <= pos {
            return Err(RmpegError::InvalidData(
                "DPX frame did not advance".to_string(),
            ));
        }
        pos = next_pos;
    }
    if frames.is_empty() {
        return Err(RmpegError::InvalidData("empty DPX input".to_string()));
    }
    Ok(frames)
}

pub fn fits_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let header = fits_header(input)?;
    let samples = fits_samples(input, &header)?;
    let (min_value, max_value) = fits_scale_range(&samples, &header)?;
    let frame_size = header
        .width
        .checked_mul(header.height)
        .and_then(|pixels| pixels.checked_mul(2))
        .ok_or_else(|| RmpegError::InvalidData("FITS frame size overflow".to_string()))?;
    let mut frame = Vec::with_capacity(frame_size);
    for out_y in 0..header.height {
        let src_y = header.height - 1 - out_y;
        for x in 0..header.width {
            let value = samples[src_y * header.width + x]
                .map(|sample| fits_gray16(sample, min_value, max_value))
                .unwrap_or(0);
            frame.extend_from_slice(&value.to_le_bytes());
        }
    }

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    }])
}

fn bmp_frame(input: &[u8]) -> Result<Vec<u8>> {
    let header = bmp_header(input)?;
    Ok(match (header.bits_per_pixel, header.compression) {
        (1 | 4 | 8, BI_RGB) => bmp_pal8_frame(
            bmp_uncompressed_indexes(input, &header)?,
            bmp_palette(input, &header)?,
        ),
        (4, BI_RLE4) => bmp_pal8_frame(
            bmp_rle4_indexes(input, &header)?,
            bmp_palette(input, &header)?,
        ),
        (8, BI_RLE8) => bmp_pal8_frame(
            bmp_rle8_indexes(input, &header)?,
            bmp_palette(input, &header)?,
        ),
        (16, BI_RGB | BI_BITFIELDS) => bmp_packed_frame(input, &header, 2)?,
        (24, BI_RGB) => bmp_packed_frame(input, &header, 3)?,
        (32, BI_RGB | BI_BITFIELDS) => bmp_bgr0_frame(input, &header)?,
        _ => {
            return Err(RmpegError::Unsupported(format!(
                "unsupported BMP bit depth {} with compression {}",
                header.bits_per_pixel, header.compression
            )));
        }
    })
}

pub fn tga_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let header = tga_header(input)?;
    let frame = match header.image_type {
        1 | 9 => tga_pal8_frame(input, &header)?,
        2 | 10 | 3 | 11 => tga_packed_frame(input, &header)?,
        _ => {
            return Err(RmpegError::Unsupported(format!(
                "unsupported TGA image type {}",
                header.image_type
            )));
        }
    };

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    }])
}

pub fn sunrast_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let header = sunrast_header(input)?;
    let frame = sunrast_frame(input, &header)?;

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    }])
}

pub fn sgi_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let header = sgi_header(input)?;
    let planes = sgi_planes(input, &header)?;
    let frame = sgi_frame(&planes, &header)?;

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    }])
}

pub fn xbm_image_frame_hashes(input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let text = std::str::from_utf8(input)
        .map_err(|_| RmpegError::InvalidData("XBM file is not valid UTF-8".to_string()))?;
    let width = xbm_define(text, "_width")?;
    let height = xbm_define(text, "_height")?;
    let data = xbm_data_block(text)?;
    let values = parse_c_integer_tokens(data)?;
    let frame = if xbm_uses_short_storage(text) {
        xbm_short_frame(width, height, &values)?
    } else {
        xbm_byte_frame(width, height, &values)?
    };

    Ok(vec![AudioFrameHash {
        stream_index: 0,
        dts: 0,
        pts: 0,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    }])
}

fn alias_pix_rle_frame(
    packets: &[u8],
    pixels: usize,
    bytes_per_pixel: usize,
    frame_size: usize,
) -> Result<Vec<u8>> {
    let mut frame = Vec::with_capacity(frame_size);
    let mut decoded_pixels = 0_usize;
    let mut pos = 0_usize;
    while decoded_pixels < pixels {
        let needed = pos + 1 + bytes_per_pixel;
        if needed > packets.len() {
            return Err(RmpegError::UnexpectedEof {
                needed,
                remaining: packets.len(),
            });
        }

        let run = usize::from(packets[pos]);
        pos += 1;
        if run == 0 {
            return Err(RmpegError::InvalidData(
                "Alias PIX RLE packet has zero run length".to_string(),
            ));
        }
        let next_pixels = decoded_pixels
            .checked_add(run)
            .ok_or_else(|| RmpegError::InvalidData("Alias PIX run overflow".to_string()))?;
        if next_pixels > pixels {
            return Err(RmpegError::InvalidData(
                "Alias PIX RLE expands past frame size".to_string(),
            ));
        }

        let value = &packets[pos..pos + bytes_per_pixel];
        for _ in 0..run {
            frame.extend_from_slice(value);
        }
        pos += bytes_per_pixel;
        decoded_pixels = next_pixels;
    }

    Ok(frame)
}

fn checked_pixels(width: u32, height: u32, context: &str) -> Result<usize> {
    let width = usize::try_from(width)
        .map_err(|_| RmpegError::Unsupported(format!("{context} width is too large")))?;
    let height = usize::try_from(height)
        .map_err(|_| RmpegError::Unsupported(format!("{context} height is too large")))?;
    width
        .checked_mul(height)
        .ok_or_else(|| RmpegError::InvalidData(format!("{context} pixel count overflow")))
}

fn read_u16_be(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_i16_be(bytes: &[u8], pos: usize) -> Result<i16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(i16::from_be_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_i32_be(bytes: &[u8], pos: usize) -> Result<i32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(i32::from_be_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

fn read_u16_at(bytes: &[u8], pos: usize, big_endian: bool) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    let raw = [bytes[pos], bytes[pos + 1]];
    Ok(if big_endian {
        u16::from_be_bytes(raw)
    } else {
        u16::from_le_bytes(raw)
    })
}

fn read_u32_at(bytes: &[u8], pos: usize, big_endian: bool) -> Result<u32> {
    if big_endian {
        read_u32_be(bytes, pos)
    } else {
        read_u32_le(bytes, pos)
    }
}

fn read_f32_be(bytes: &[u8], pos: usize) -> Result<f32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(f32::from_be_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

fn read_f64_be(bytes: &[u8], pos: usize) -> Result<f64> {
    let end = pos + 8;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(f64::from_be_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
        bytes[pos + 4],
        bytes[pos + 5],
        bytes[pos + 6],
        bytes[pos + 7],
    ]))
}

struct DpxHeader {
    big_endian: bool,
    file_size: usize,
    data_offset: usize,
    width: usize,
    height: usize,
    bit_depth: u8,
    packing: u16,
    data_bytes: usize,
}

fn dpx_header(bytes: &[u8], base: usize) -> Result<DpxHeader> {
    let header_end = base
        .checked_add(DPX_HEADER_MIN_LEN)
        .ok_or_else(|| RmpegError::InvalidData("DPX header range overflow".to_string()))?;
    if header_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: header_end,
            remaining: bytes.len(),
        });
    }
    let magic = &bytes[base..base + 4];
    let big_endian = match magic {
        b"SDPX" => true,
        b"XPDS" => false,
        _ => return Err(RmpegError::InvalidData("missing DPX magic".to_string())),
    };
    let file_size = usize::try_from(read_u32_at(bytes, base + 16, big_endian)?)
        .map_err(|_| RmpegError::Unsupported("DPX file size is too large".to_string()))?;
    let image_offset = usize::try_from(read_u32_at(bytes, base + 4, big_endian)?)
        .map_err(|_| RmpegError::Unsupported("DPX image offset is too large".to_string()))?;
    let element_count = read_u16_at(bytes, base + 770, big_endian)?;
    if element_count != 1 {
        return Err(RmpegError::Unsupported(format!(
            "unsupported DPX image element count {element_count}"
        )));
    }
    let width = usize::try_from(read_u32_at(bytes, base + 772, big_endian)?)
        .map_err(|_| RmpegError::Unsupported("DPX width is too large".to_string()))?;
    let height = usize::try_from(read_u32_at(bytes, base + 776, big_endian)?)
        .map_err(|_| RmpegError::Unsupported("DPX height is too large".to_string()))?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "DPX dimensions must be nonzero".to_string(),
        ));
    }
    let descriptor = bytes[base + 800];
    if descriptor != DPX_DESCRIPTOR_RGB {
        return Err(RmpegError::Unsupported(format!(
            "unsupported DPX descriptor {descriptor}"
        )));
    }
    let bit_depth = bytes[base + 803];
    let packing = read_u16_at(bytes, base + 804, big_endian)?;
    let encoding = read_u16_at(bytes, base + 806, big_endian)?;
    if encoding != 0 {
        return Err(RmpegError::Unsupported(format!(
            "unsupported DPX encoding {encoding}"
        )));
    }

    let row_bytes = dpx_stored_row_bytes(width, bit_depth, packing)?;
    let data_bytes = row_bytes
        .checked_mul(height)
        .ok_or_else(|| RmpegError::InvalidData("DPX image data size overflow".to_string()))?;
    Ok(DpxHeader {
        big_endian,
        file_size,
        data_offset: image_offset,
        width,
        height,
        bit_depth,
        packing,
        data_bytes,
    })
}

fn dpx_stored_row_bytes(width: usize, bit_depth: u8, packing: u16) -> Result<usize> {
    match (bit_depth, packing) {
        (8, 0) => dpx_align4(
            width
                .checked_mul(3)
                .ok_or_else(|| RmpegError::InvalidData("DPX row size overflow".to_string()))?,
        ),
        (10, 1) => width
            .checked_mul(4)
            .ok_or_else(|| RmpegError::InvalidData("DPX row size overflow".to_string())),
        (16, 0) => dpx_align4(
            width
                .checked_mul(6)
                .ok_or_else(|| RmpegError::InvalidData("DPX row size overflow".to_string()))?,
        ),
        _ => Err(RmpegError::Unsupported(format!(
            "unsupported DPX bit depth {bit_depth} with packing {packing}"
        ))),
    }
}

fn dpx_align4(value: usize) -> Result<usize> {
    value
        .checked_add(3)
        .map(|value| value & !3)
        .ok_or_else(|| RmpegError::InvalidData("DPX row alignment overflow".to_string()))
}

fn dpx_frame(bytes: &[u8], base: usize, header: &DpxHeader) -> Result<Vec<u8>> {
    let data_start = base
        .checked_add(header.data_offset)
        .ok_or_else(|| RmpegError::InvalidData("DPX data offset overflow".to_string()))?;
    let data_end = data_start
        .checked_add(header.data_bytes)
        .ok_or_else(|| RmpegError::InvalidData("DPX data range overflow".to_string()))?;
    if data_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_end,
            remaining: bytes.len(),
        });
    }
    match (header.bit_depth, header.packing) {
        (8, 0) => dpx_frame_8(bytes, data_start, header),
        (10, 1) => dpx_frame_10(bytes, data_start, header),
        (16, 0) => dpx_frame_16(bytes, data_start, header),
        _ => unreachable!("DPX format was validated"),
    }
}

fn dpx_frame_8(bytes: &[u8], data_start: usize, header: &DpxHeader) -> Result<Vec<u8>> {
    dpx_frame_unpadded_rows(bytes, data_start, header, 3)
}

fn dpx_frame_16(bytes: &[u8], data_start: usize, header: &DpxHeader) -> Result<Vec<u8>> {
    dpx_frame_unpadded_rows(bytes, data_start, header, 6)
}

fn dpx_frame_unpadded_rows(
    bytes: &[u8],
    data_start: usize,
    header: &DpxHeader,
    bytes_per_pixel: usize,
) -> Result<Vec<u8>> {
    let row_bytes = header
        .width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| RmpegError::InvalidData("DPX output row size overflow".to_string()))?;
    let stored_row_bytes = dpx_stored_row_bytes(header.width, header.bit_depth, header.packing)?;
    let mut frame =
        Vec::with_capacity(row_bytes.checked_mul(header.height).ok_or_else(|| {
            RmpegError::InvalidData("DPX output frame size overflow".to_string())
        })?);
    for row in 0..header.height {
        let start = data_start + row * stored_row_bytes;
        frame.extend_from_slice(&bytes[start..start + row_bytes]);
    }
    Ok(frame)
}

fn dpx_frame_10(bytes: &[u8], data_start: usize, header: &DpxHeader) -> Result<Vec<u8>> {
    let pixels = header
        .width
        .checked_mul(header.height)
        .ok_or_else(|| RmpegError::InvalidData("DPX pixel count overflow".to_string()))?;
    let mut planes = [
        Vec::with_capacity(pixels * 2),
        Vec::with_capacity(pixels * 2),
        Vec::with_capacity(pixels * 2),
    ];
    let stored_row_bytes = dpx_stored_row_bytes(header.width, header.bit_depth, header.packing)?;
    for row in 0..header.height {
        let mut pos = data_start + row * stored_row_bytes;
        for _ in 0..header.width {
            let word = read_u32_at(bytes, pos, header.big_endian)?;
            pos += 4;
            let components = [
                ((word >> 22) & 0x03ff) as u16,
                ((word >> 12) & 0x03ff) as u16,
                ((word >> 2) & 0x03ff) as u16,
            ];
            for (plane, component) in planes.iter_mut().zip(components) {
                plane.extend_from_slice(&component.to_le_bytes());
            }
        }
    }

    let mut frame = Vec::with_capacity(pixels * 6);
    frame.extend_from_slice(&planes[1]);
    frame.extend_from_slice(&planes[2]);
    frame.extend_from_slice(&planes[0]);
    Ok(frame)
}

struct FitsHeader {
    width: usize,
    height: usize,
    bitpix: i32,
    data_offset: usize,
    bscale: f64,
    bzero: f64,
    blank: Option<i64>,
    datamin: Option<f64>,
    datamax: Option<f64>,
}

fn fits_header(bytes: &[u8]) -> Result<FitsHeader> {
    if !bytes.starts_with(b"SIMPLE  ") {
        return Err(RmpegError::InvalidData("missing FITS header".to_string()));
    }

    let mut bitpix = None;
    let mut naxis = None;
    let mut width = None;
    let mut height = None;
    let mut bscale = 1.0;
    let mut bzero = 0.0;
    let mut blank = None;
    let mut datamin = None;
    let mut datamax = None;
    let mut data_offset = None;

    for card_start in (0..bytes.len()).step_by(FITS_CARD_LEN) {
        let card_end = card_start + FITS_CARD_LEN;
        if card_end > bytes.len() {
            break;
        }
        let card = &bytes[card_start..card_end];
        let key = std::str::from_utf8(&card[..8])
            .map_err(|_| RmpegError::InvalidData("FITS card key is not ASCII".to_string()))?
            .trim();
        if key == "END" {
            data_offset = Some(align_to_fits_block(card_end)?);
            break;
        }
        let Some(value) = fits_card_value(card)? else {
            continue;
        };
        match key {
            "BITPIX" => {
                bitpix =
                    Some(i32::try_from(fits_i64_value(value, key)?).map_err(|_| {
                        RmpegError::Unsupported("FITS BITPIX is too large".to_string())
                    })?)
            }
            "NAXIS" => naxis = Some(fits_i64_value(value, key)?),
            "NAXIS1" => width = Some(fits_positive_usize(value, key)?),
            "NAXIS2" => height = Some(fits_positive_usize(value, key)?),
            "BSCALE" => bscale = fits_f64_value(value, key)?,
            "BZERO" => bzero = fits_f64_value(value, key)?,
            "BLANK" => blank = Some(fits_i64_value(value, key)?),
            "DATAMIN" => datamin = Some(fits_f64_value(value, key)?),
            "DATAMAX" => datamax = Some(fits_f64_value(value, key)?),
            _ => {}
        }
    }

    if naxis != Some(2) {
        return Err(RmpegError::Unsupported(
            "only two-dimensional FITS images are supported".to_string(),
        ));
    }
    let bitpix =
        bitpix.ok_or_else(|| RmpegError::InvalidData("FITS BITPIX was not found".to_string()))?;
    if !matches!(bitpix, 16 | 32 | -32 | -64) {
        return Err(RmpegError::Unsupported(format!(
            "unsupported FITS BITPIX {bitpix}"
        )));
    }
    let width =
        width.ok_or_else(|| RmpegError::InvalidData("FITS NAXIS1 was not found".to_string()))?;
    let height =
        height.ok_or_else(|| RmpegError::InvalidData("FITS NAXIS2 was not found".to_string()))?;
    let data_offset = data_offset
        .ok_or_else(|| RmpegError::InvalidData("FITS END card was not found".to_string()))?;
    if data_offset > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_offset,
            remaining: bytes.len(),
        });
    }

    Ok(FitsHeader {
        width,
        height,
        bitpix,
        data_offset,
        bscale,
        bzero,
        blank,
        datamin,
        datamax,
    })
}

fn align_to_fits_block(offset: usize) -> Result<usize> {
    offset
        .checked_add(FITS_BLOCK_LEN - 1)
        .map(|value| value / FITS_BLOCK_LEN * FITS_BLOCK_LEN)
        .ok_or_else(|| RmpegError::InvalidData("FITS header offset overflow".to_string()))
}

fn fits_card_value(card: &[u8]) -> Result<Option<&str>> {
    if card.get(8) != Some(&b'=') {
        return Ok(None);
    }
    let value = std::str::from_utf8(&card[10..])
        .map_err(|_| RmpegError::InvalidData("FITS card value is not ASCII".to_string()))?;
    Ok(Some(value.split('/').next().unwrap_or("").trim()))
}

fn fits_i64_value(value: &str, key: &str) -> Result<i64> {
    value
        .parse::<i64>()
        .map_err(|_| RmpegError::InvalidData(format!("FITS {key} value is invalid")))
}

fn fits_positive_usize(value: &str, key: &str) -> Result<usize> {
    let value = fits_i64_value(value, key)?;
    if value <= 0 {
        return Err(RmpegError::InvalidData(format!(
            "FITS {key} must be positive"
        )));
    }
    usize::try_from(value).map_err(|_| RmpegError::Unsupported(format!("FITS {key} is too large")))
}

fn fits_f64_value(value: &str, key: &str) -> Result<f64> {
    let normalized = value.replace(['D', 'd'], "E");
    normalized
        .parse::<f64>()
        .map_err(|_| RmpegError::InvalidData(format!("FITS {key} value is invalid")))
}

fn fits_samples(bytes: &[u8], header: &FitsHeader) -> Result<Vec<Option<f64>>> {
    let pixels = header
        .width
        .checked_mul(header.height)
        .ok_or_else(|| RmpegError::InvalidData("FITS pixel count overflow".to_string()))?;
    let bytes_per_sample = usize::try_from(header.bitpix.unsigned_abs() / 8)
        .map_err(|_| RmpegError::Unsupported("FITS sample size is too large".to_string()))?;
    let data_bytes = pixels
        .checked_mul(bytes_per_sample)
        .ok_or_else(|| RmpegError::InvalidData("FITS data size overflow".to_string()))?;
    let data_end = header
        .data_offset
        .checked_add(data_bytes)
        .ok_or_else(|| RmpegError::InvalidData("FITS data range overflow".to_string()))?;
    if data_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_end,
            remaining: bytes.len(),
        });
    }

    let mut samples = Vec::with_capacity(pixels);
    for index in 0..pixels {
        let pos = header.data_offset + index * bytes_per_sample;
        let value = match header.bitpix {
            16 => {
                let raw = i64::from(read_i16_be(bytes, pos)?);
                if header.blank == Some(raw) {
                    None
                } else {
                    Some(raw as f64 * header.bscale + header.bzero)
                }
            }
            32 => {
                let raw = i64::from(read_i32_be(bytes, pos)?);
                if header.blank == Some(raw) {
                    None
                } else {
                    Some(raw as f64 * header.bscale + header.bzero)
                }
            }
            -32 => Some(f64::from(read_f32_be(bytes, pos)?)),
            -64 => Some(read_f64_be(bytes, pos)?),
            _ => unreachable!("FITS BITPIX was validated"),
        };
        samples.push(value.filter(|sample| sample.is_finite()));
    }
    Ok(samples)
}

fn fits_scale_range(samples: &[Option<f64>], header: &FitsHeader) -> Result<(f64, f64)> {
    if let (Some(min), Some(max)) = (header.datamin, header.datamax) {
        if min.is_finite() && max.is_finite() && max > min {
            return Ok((min, max));
        }
    }

    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &sample in samples.iter().flatten() {
        min = min.min(sample);
        max = max.max(sample);
    }
    if min.is_finite() && max.is_finite() && max > min {
        Ok((min, max))
    } else {
        Err(RmpegError::InvalidData(
            "FITS image has no usable sample range".to_string(),
        ))
    }
}

fn fits_gray16(value: f64, min: f64, max: f64) -> u16 {
    let scaled = ((value - min) * 65535.0 / (max - min)).clamp(0.0, 65535.0);
    (scaled + 0.5).floor() as u16
}

struct PnmHeader {
    magic: [u8; 2],
    width: usize,
    height: usize,
    max_value: u32,
    data_offset: usize,
}

struct PtxHeader {
    frame_size: usize,
}

struct BRenderPixHeader {
    frame_offset: usize,
    frame_size: usize,
    palette: Option<[u8; BMP_PALETTE_BYTES]>,
}

struct DdsHeader {
    frame_offset: usize,
    frame_size: usize,
    width: usize,
    height: usize,
    postprocess: DdsPostprocess,
}

#[derive(Clone, Copy)]
enum DdsPostprocess {
    None,
    Palette {
        palette_offset: usize,
    },
    Aexp,
    Ycocg,
    Bc1 {
        normal_map: bool,
    },
    Bc2 {
        premultiplied: bool,
    },
    Bc3 {
        premultiplied: bool,
        transform: DdsBc3Transform,
    },
    Bc4 {
        signed: bool,
    },
    Bc5 {
        signed: bool,
        swap_xy: bool,
    },
}

#[derive(Clone, Copy)]
enum DdsBc3Transform {
    None,
    Aexp,
    NormalAg,
    Swizzle(DdsBc3Swizzle),
    Ycocg { scaled: bool },
}

#[derive(Clone, Copy)]
enum DdsBc3Swizzle {
    Rbxg,
    Rgxb,
    Rxbg,
    Rxgb,
    Xgbr,
    Xgxr,
    Xrbg,
}

fn ptx_header(bytes: &[u8]) -> Result<PtxHeader> {
    if bytes.len() < PTX_HEADER_LEN {
        return Err(RmpegError::UnexpectedEof {
            needed: PTX_HEADER_LEN,
            remaining: bytes.len(),
        });
    }
    let header_len = read_u32_le(bytes, 0)?;
    if header_len != PTX_HEADER_LEN as u32 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported PTX header length {header_len}"
        )));
    }
    let width = usize::from(read_u16_le(bytes, 8)?);
    let height = usize::from(read_u16_le(bytes, 10)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "PTX dimensions must be nonzero".to_string(),
        ));
    }
    let frame_size = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(2))
        .ok_or_else(|| RmpegError::InvalidData("PTX frame size overflow".to_string()))?;
    let data_end = PTX_HEADER_LEN
        .checked_add(frame_size)
        .ok_or_else(|| RmpegError::InvalidData("PTX frame range overflow".to_string()))?;
    if data_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_end,
            remaining: bytes.len(),
        });
    }
    Ok(PtxHeader { frame_size })
}

fn brender_pix_header(bytes: &[u8]) -> Result<BRenderPixHeader> {
    if bytes.len() < BRENDER_PIX_DATA_OFFSET {
        return Err(RmpegError::UnexpectedEof {
            needed: BRENDER_PIX_DATA_OFFSET,
            remaining: bytes.len(),
        });
    }
    if bytes[0..4] != [0, 0, 0, 0x12]
        || bytes[4..8] != [0, 0, 0, 8]
        || bytes[8..12] != [0, 0, 0, 2]
        || bytes[12..16] != [0, 0, 0, 2]
    {
        return Err(RmpegError::InvalidData(
            "missing BRender PIX header".to_string(),
        ));
    }

    let marker = &bytes[24..28];
    let (width, height, frame_offset, frame_size, palette) = match marker {
        [0x12, 0x00, 0x20, 0x00] | [0x05, 0x01, 0x00, 0x00] => {
            let width = u32::from(read_u16_le(bytes, 28)?);
            let height = u32::from(read_u16_le(bytes, 30)?);
            let pixels = checked_brender_pix_pixels(width, height)?;
            (
                width,
                height,
                BRENDER_PIX_DATA_OFFSET,
                pixels.checked_mul(2).ok_or_else(|| {
                    RmpegError::InvalidData("BRender PIX frame size overflow".to_string())
                })?,
                None,
            )
        }
        [0x03, 0x01, _, _] => {
            let width = u32::from(read_u16_le(bytes, 28)?);
            let height = u32::from(read_u16_le(bytes, 26)?);
            let pixels = checked_brender_pix_pixels(width, height)?;
            let frame_offset = brender_pix_data_offset(bytes)?;
            let palette = Some(brender_pix_palette(bytes)?);
            (width, height, frame_offset, pixels, palette)
        }
        marker => {
            return Err(RmpegError::Unsupported(format!(
                "unsupported BRender PIX pixel format {:02x}{:02x}{:02x}{:02x}",
                marker[0], marker[1], marker[2], marker[3]
            )));
        }
    };
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "BRender PIX dimensions must be nonzero".to_string(),
        ));
    }
    let data_end = frame_offset
        .checked_add(frame_size)
        .ok_or_else(|| RmpegError::InvalidData("BRender PIX frame range overflow".to_string()))?;
    if data_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_end,
            remaining: bytes.len(),
        });
    }

    Ok(BRenderPixHeader {
        frame_offset,
        frame_size,
        palette,
    })
}

fn checked_brender_pix_pixels(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "BRender PIX dimensions must be nonzero".to_string(),
        ));
    }
    checked_pixels(width, height, "BRender PIX")
}

fn brender_pix_data_offset(bytes: &[u8]) -> Result<usize> {
    find_bytes(bytes, BRENDER_PIX_INDEX_MARKER)
        .map(|pos| pos + BRENDER_PIX_INDEX_MARKER.len())
        .ok_or_else(|| RmpegError::InvalidData("missing BRender PIX index marker".to_string()))
}

fn brender_pix_palette(bytes: &[u8]) -> Result<[u8; BMP_PALETTE_BYTES]> {
    if let Some(pos) = find_bytes(bytes, BRENDER_PIX_PALETTE_MARKER) {
        let start = pos + BRENDER_PIX_PALETTE_MARKER.len();
        let end = start
            .checked_add(BMP_PALETTE_BYTES)
            .ok_or_else(|| RmpegError::InvalidData("BRender PIX palette overflow".to_string()))?;
        if end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: end,
                remaining: bytes.len(),
            });
        }
        let mut palette = [0_u8; BMP_PALETTE_BYTES];
        for entry in 0..256 {
            let src = start + entry * 4;
            let dst = entry * 4;
            palette[dst] = bytes[src + 2];
            palette[dst + 1] = bytes[src + 1];
            palette[dst + 2] = bytes[src];
            palette[dst + 3] = 0xff;
        }
        return Ok(palette);
    }

    Ok(brender_pix_default_palette())
}

fn brender_pix_default_palette() -> [u8; BMP_PALETTE_BYTES] {
    let mut palette = [0_u8; BMP_PALETTE_BYTES];
    for (entry, &gray) in BRENDER_PIX_GRAY64.iter().enumerate() {
        set_palette_entry(&mut palette, entry, gray, gray, gray);
    }
    for idx in 0..32 {
        let bright = BRENDER_PIX_RAMP32[idx];
        let dim = BRENDER_PIX_DIM32[idx];
        set_palette_entry(&mut palette, 64 + idx, bright, dim, dim);
        set_palette_entry(&mut palette, 96 + idx, dim, bright, dim);
        set_palette_entry(&mut palette, 128 + idx, bright, bright, dim);
        set_palette_entry(&mut palette, 160 + idx, dim, dim, bright);
        set_palette_entry(&mut palette, 192 + idx, bright, dim, bright);
        set_palette_entry(&mut palette, 224 + idx, dim, bright, bright);
    }
    palette
}

fn set_palette_entry(palette: &mut [u8; BMP_PALETTE_BYTES], entry: usize, b: u8, g: u8, r: u8) {
    let dst = entry * 4;
    palette[dst] = b;
    palette[dst + 1] = g;
    palette[dst + 2] = r;
    palette[dst + 3] = 0xff;
}

fn find_bytes(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > bytes.len() {
        return None;
    }
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
}

fn dds_header(bytes: &[u8]) -> Result<DdsHeader> {
    if bytes.len() < DDS_HEADER_LEN {
        return Err(RmpegError::UnexpectedEof {
            needed: DDS_HEADER_LEN,
            remaining: bytes.len(),
        });
    }
    if &bytes[0..4] != b"DDS " {
        return Err(RmpegError::InvalidData("missing DDS signature".to_string()));
    }
    let header_size = read_u32_le(bytes, 4)?;
    if header_size != 124 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported DDS header size {header_size}"
        )));
    }
    let height = usize_from_u32(read_u32_le(bytes, 12)?, "DDS height")?;
    let width = usize_from_u32(read_u32_le(bytes, 16)?, "DDS width")?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "DDS dimensions must be nonzero".to_string(),
        ));
    }
    let pitch_or_linear_size = read_u32_le(bytes, 20)?;
    let pixel_format_size = read_u32_le(bytes, 76)?;
    if pixel_format_size != DDS_PIXEL_FORMAT_LEN {
        return Err(RmpegError::InvalidData(format!(
            "unsupported DDS pixel format size {pixel_format_size}"
        )));
    }
    let flags = read_u32_le(bytes, 80)?;
    let fourcc = bytes[84..88]
        .try_into()
        .expect("DDS header length was checked");
    let bits_per_pixel = read_u32_le(bytes, 88)?;
    let masks = [
        read_u32_le(bytes, 92)?,
        read_u32_le(bytes, 96)?,
        read_u32_le(bytes, 100)?,
        read_u32_le(bytes, 104)?,
    ];
    let special_tag = bytes[44..48]
        .try_into()
        .expect("DDS header length was checked");
    let header = if let Some(frame_size) =
        dds_palette_frame_size(width, height, flags, fourcc, bits_per_pixel, masks)?
    {
        let frame_offset = DDS_HEADER_LEN
            .checked_add(DDS_PALETTE_BYTES)
            .ok_or_else(|| RmpegError::InvalidData("DDS palette range overflow".to_string()))?;
        DdsHeader {
            frame_offset,
            frame_size,
            width,
            height,
            postprocess: DdsPostprocess::Palette {
                palette_offset: DDS_HEADER_LEN,
            },
        }
    } else if let Some((frame_size, postprocess)) = dds_special_transform_frame(
        width,
        height,
        pitch_or_linear_size,
        flags,
        bits_per_pixel,
        masks,
        special_tag,
    )? {
        DdsHeader {
            frame_offset: DDS_HEADER_LEN,
            frame_size,
            width,
            height,
            postprocess,
        }
    } else if let Some((frame_offset, frame_size, postprocess)) = dds_block_compressed_frame(
        bytes,
        width,
        height,
        flags,
        fourcc,
        bits_per_pixel,
        special_tag,
    )? {
        DdsHeader {
            frame_offset,
            frame_size,
            width,
            height,
            postprocess,
        }
    } else {
        let frame_size = dds_direct_frame_size(
            width,
            height,
            pitch_or_linear_size,
            flags,
            fourcc,
            bits_per_pixel,
            masks,
        )?;
        DdsHeader {
            frame_offset: DDS_HEADER_LEN,
            frame_size,
            width,
            height,
            postprocess: DdsPostprocess::None,
        }
    };
    let data_end = header
        .frame_offset
        .checked_add(header.frame_size)
        .ok_or_else(|| RmpegError::InvalidData("DDS frame range overflow".to_string()))?;
    if data_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_end,
            remaining: bytes.len(),
        });
    }

    Ok(header)
}

fn dds_palette_frame_size(
    width: usize,
    height: usize,
    flags: u32,
    fourcc: &[u8; 4],
    bits_per_pixel: u32,
    masks: [u32; 4],
) -> Result<Option<usize>> {
    let indexed8 = flags & DDPF_PALETTEINDEXED8 != 0
        && flags & DDPF_FOURCC == 0
        && bits_per_pixel == 8
        && masks == [0, 0, 0, 0];
    let fourcc_p8 = flags & DDPF_FOURCC != 0
        && fourcc == b"P8  "
        && bits_per_pixel == 0
        && masks == [0, 0, 0, 0];
    if indexed8 || fourcc_p8 {
        return dds_packed_size(width, height, 1).map(Some);
    }
    Ok(None)
}

fn dds_special_transform_frame(
    width: usize,
    height: usize,
    pitch_or_linear_size: u32,
    flags: u32,
    bits_per_pixel: u32,
    masks: [u32; 4],
    special_tag: &[u8; 4],
) -> Result<Option<(usize, DdsPostprocess)>> {
    if flags != (DDPF_RGB | DDPF_ALPHAPIXELS)
        || bits_per_pixel != 32
        || masks != [0xff0000, 0x00ff00, 0x0000ff, 0xff000000]
    {
        return Ok(None);
    }
    let row_bytes = u32::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| RmpegError::InvalidData("DDS row size overflow".to_string()))?;
    if pitch_or_linear_size != row_bytes {
        return Ok(None);
    }
    let postprocess = match special_tag {
        b"AEXP" => DdsPostprocess::Aexp,
        b"YCG1" => DdsPostprocess::Ycocg,
        _ => return Ok(None),
    };
    dds_packed_size(width, height, 4).map(|size| Some((size, postprocess)))
}

fn dds_block_compressed_frame(
    bytes: &[u8],
    width: usize,
    height: usize,
    flags: u32,
    fourcc: &[u8; 4],
    bits_per_pixel: u32,
    special_tag: &[u8; 4],
) -> Result<Option<(usize, usize, DdsPostprocess)>> {
    let plain_fourcc = flags == DDPF_FOURCC && bits_per_pixel == 0 && special_tag == b"\0\0\0\0";
    let direct = if plain_fourcc {
        match fourcc {
            b"DXT1" => Some((DDS_HEADER_LEN, 8, DdsPostprocess::Bc1 { normal_map: false })),
            b"DXT2" => Some((
                DDS_HEADER_LEN,
                16,
                DdsPostprocess::Bc2 {
                    premultiplied: true,
                },
            )),
            b"DXT3" => Some((
                DDS_HEADER_LEN,
                16,
                DdsPostprocess::Bc2 {
                    premultiplied: false,
                },
            )),
            b"DXT4" => Some((
                DDS_HEADER_LEN,
                16,
                DdsPostprocess::Bc3 {
                    premultiplied: true,
                    transform: DdsBc3Transform::None,
                },
            )),
            b"DXT5" => Some((
                DDS_HEADER_LEN,
                16,
                DdsPostprocess::Bc3 {
                    premultiplied: false,
                    transform: DdsBc3Transform::None,
                },
            )),
            b"ATI1" => Some((DDS_HEADER_LEN, 8, DdsPostprocess::Bc4 { signed: false })),
            b"BC4S" => Some((DDS_HEADER_LEN, 8, DdsPostprocess::Bc4 { signed: true })),
            b"ATI2" => Some((
                DDS_HEADER_LEN,
                16,
                DdsPostprocess::Bc5 {
                    signed: false,
                    swap_xy: true,
                },
            )),
            b"BC5S" => Some((
                DDS_HEADER_LEN,
                16,
                DdsPostprocess::Bc5 {
                    signed: true,
                    swap_xy: false,
                },
            )),
            _ => None,
        }
    } else {
        None
    };
    if let Some((offset, block_bytes, postprocess)) = direct {
        return dds_block_data_size(width, height, block_bytes)
            .map(|size| Some((offset, size, postprocess)));
    }
    if flags == (DDPF_FOURCC | 0x8000_0000) && bits_per_pixel == 0 && special_tag == b"\0\0\0\0" {
        let normal = match fourcc {
            b"DXT1" => Some((DDS_HEADER_LEN, 8, DdsPostprocess::Bc1 { normal_map: true })),
            b"DXT5" => Some((
                DDS_HEADER_LEN,
                16,
                DdsPostprocess::Bc3 {
                    premultiplied: false,
                    transform: DdsBc3Transform::NormalAg,
                },
            )),
            b"RXGB" => Some((
                DDS_HEADER_LEN,
                16,
                DdsPostprocess::Bc3 {
                    premultiplied: false,
                    transform: DdsBc3Transform::Swizzle(DdsBc3Swizzle::Rxgb),
                },
            )),
            _ => None,
        };
        if let Some((offset, block_bytes, postprocess)) = normal {
            return dds_block_data_size(width, height, block_bytes)
                .map(|size| Some((offset, size, postprocess)));
        }
    }
    if flags == DDPF_FOURCC && fourcc == b"DXT5" {
        let bits_tag = bits_per_pixel.to_le_bytes();
        let transform = if bits_tag == [0, 0, 0, 0] {
            match special_tag {
                b"AEXP" => Some(DdsBc3Transform::Aexp),
                b"YCG1" => Some(DdsBc3Transform::Ycocg { scaled: false }),
                b"YCG2" => Some(DdsBc3Transform::Ycocg { scaled: true }),
                _ => None,
            }
        } else if special_tag == b"\0\0\0\0" {
            if bits_tag == *b"A2D5" {
                Some(DdsBc3Transform::NormalAg)
            } else if bits_tag == *b"RBxG" {
                Some(DdsBc3Transform::Swizzle(DdsBc3Swizzle::Rbxg))
            } else if bits_tag == *b"RGxB" {
                Some(DdsBc3Transform::Swizzle(DdsBc3Swizzle::Rgxb))
            } else if bits_tag == *b"RxBG" {
                Some(DdsBc3Transform::Swizzle(DdsBc3Swizzle::Rxbg))
            } else if bits_tag == *b"xGBR" {
                Some(DdsBc3Transform::Swizzle(DdsBc3Swizzle::Xgbr))
            } else if bits_tag == *b"xGxR" {
                Some(DdsBc3Transform::Swizzle(DdsBc3Swizzle::Xgxr))
            } else if bits_tag == *b"xRBG" {
                Some(DdsBc3Transform::Swizzle(DdsBc3Swizzle::Xrbg))
            } else {
                None
            }
        } else {
            None
        };
        if let Some(transform) = transform {
            return dds_block_data_size(width, height, 16).map(|size| {
                Some((
                    DDS_HEADER_LEN,
                    size,
                    DdsPostprocess::Bc3 {
                        premultiplied: false,
                        transform,
                    },
                ))
            });
        }
    }
    if flags == DDPF_FOURCC
        && fourcc == b"ATI2"
        && bits_per_pixel == u32::from_le_bytes(*b"A2XY")
        && special_tag == b"\0\0\0\0"
    {
        return dds_block_data_size(width, height, 16).map(|size| {
            Some((
                DDS_HEADER_LEN,
                size,
                DdsPostprocess::Bc5 {
                    signed: false,
                    swap_xy: false,
                },
            ))
        });
    }
    if fourcc != b"DX10" {
        return Ok(None);
    }
    let dx10_end = DDS_HEADER_LEN
        .checked_add(20)
        .ok_or_else(|| RmpegError::InvalidData("DDS DX10 header range overflow".to_string()))?;
    if bytes.len() < dx10_end {
        return Err(RmpegError::UnexpectedEof {
            needed: dx10_end,
            remaining: bytes.len(),
        });
    }
    let dxgi_format = read_u32_le(bytes, DDS_HEADER_LEN)?;
    let (block_bytes, postprocess) = match dxgi_format {
        71 => (8, DdsPostprocess::Bc1 { normal_map: false }),
        74 => (
            16,
            DdsPostprocess::Bc2 {
                premultiplied: false,
            },
        ),
        77 => (
            16,
            DdsPostprocess::Bc3 {
                premultiplied: false,
                transform: DdsBc3Transform::None,
            },
        ),
        80 => (8, DdsPostprocess::Bc4 { signed: false }),
        83 => (
            16,
            DdsPostprocess::Bc5 {
                signed: false,
                swap_xy: false,
            },
        ),
        _ => return Ok(None),
    };
    dds_block_data_size(width, height, block_bytes).map(|size| Some((dx10_end, size, postprocess)))
}

fn dds_direct_frame_size(
    width: usize,
    height: usize,
    pitch_or_linear_size: u32,
    flags: u32,
    fourcc: &[u8; 4],
    bits_per_pixel: u32,
    masks: [u32; 4],
) -> Result<usize> {
    if flags & DDPF_FOURCC != 0 {
        return match fourcc {
            b"YUY2" | b"UYVY" => dds_packed_size(width, height, 2),
            b"G1  " => {
                let row_bytes = width.checked_add(7).map(|value| value / 8).ok_or_else(|| {
                    RmpegError::InvalidData("DDS monob row size overflow".to_string())
                })?;
                row_bytes.checked_mul(height).ok_or_else(|| {
                    RmpegError::InvalidData("DDS monob frame size overflow".to_string())
                })
            }
            _ => Err(RmpegError::Unsupported(format!(
                "unsupported DDS FOURCC {}",
                String::from_utf8_lossy(fourcc)
            ))),
        };
    }

    if flags & DDPF_ALPHA != 0 && bits_per_pixel == 8 && masks == [0, 0, 0, 0xff] {
        return dds_packed_size(width, height, 1);
    }

    if flags & DDPF_LUMINANCE != 0 {
        if bits_per_pixel == 8 && masks == [0xff, 0, 0, 0] {
            return dds_packed_size(width, height, 1);
        }
        if flags & DDPF_ALPHAPIXELS != 0 && bits_per_pixel == 16 && masks == [0xff, 0, 0, 0xff00] {
            return dds_packed_size(width, height, 2);
        }
    }

    if flags & DDPF_RGB != 0 {
        match (bits_per_pixel, masks) {
            (16, [0x7c00, 0x03e0, 0x001f, 0])
            | (16, [0x7c00, 0x03e0, 0x001f, 0x8000])
            | (16, [0xf800, 0x07e0, 0x001f, 0]) => return dds_packed_size(width, height, 2),
            (24, [0xff0000, 0x00ff00, 0x0000ff, 0]) => return dds_packed_size(width, height, 3),
            (32, [0xff0000, 0x00ff00, 0x0000ff, 0]) | (32, [0x0000ff, 0x00ff00, 0xff0000, 0]) => {
                return dds_packed_size(width, height, 4);
            }
            (32, [0xff0000, 0x00ff00, 0x0000ff, 0xff000000]) => {
                let row_bytes = u32::try_from(width)
                    .ok()
                    .and_then(|width| width.checked_mul(4))
                    .ok_or_else(|| RmpegError::InvalidData("DDS row size overflow".to_string()))?;
                let frame_bytes = row_bytes
                    .checked_mul(u32::try_from(height).map_err(|_| {
                        RmpegError::Unsupported("DDS height is too large".to_string())
                    })?)
                    .ok_or_else(|| {
                        RmpegError::InvalidData("DDS frame size overflow".to_string())
                    })?;
                if pitch_or_linear_size == 0 || pitch_or_linear_size == frame_bytes {
                    return dds_packed_size(width, height, 4);
                }
            }
            _ => {}
        }
    }

    Err(RmpegError::Unsupported(format!(
        "unsupported DDS pixel format flags={flags:#x} fourcc={} bits={bits_per_pixel} masks={:x}/{:x}/{:x}/{:x}",
        String::from_utf8_lossy(fourcc),
        masks[0],
        masks[1],
        masks[2],
        masks[3]
    )))
}

fn dds_aexp_frame(data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(data.len());
    for pixel in data.chunks_exact(4) {
        let alpha = u16::from(pixel[3]);
        frame.push((u16::from(pixel[0]) * alpha / 255) as u8);
        frame.push((u16::from(pixel[1]) * alpha / 255) as u8);
        frame.push((u16::from(pixel[2]) * alpha / 255) as u8);
        frame.push(0xff);
    }
    frame
}

fn dds_ycocg_frame(data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(data.len());
    for pixel in data.chunks_exact(4) {
        let co = i32::from(pixel[2]) - 128;
        let cg = i32::from(pixel[1]) - 128;
        let y = i32::from(pixel[3]);
        let base = y - cg;
        frame.push(clamp_u8_i32(base + co));
        frame.push(clamp_u8_i32(y + cg));
        frame.push(clamp_u8_i32(base - co));
        frame.push(0xff);
    }
    frame
}

fn dds_bc1_frame(data: &[u8], width: usize, height: usize, normal_map: bool) -> Result<Vec<u8>> {
    dds_decode_blocks(data, width, height, 8, |block, rgba| {
        dds_decode_bc_color_block(&block[0..8], true, [0xff; 16], rgba);
        if normal_map {
            dds_normal_rg_block(rgba);
        }
    })
}

fn dds_bc2_frame(data: &[u8], width: usize, height: usize, premultiplied: bool) -> Result<Vec<u8>> {
    dds_decode_blocks(data, width, height, 16, |block, rgba| {
        let mut alpha = [0xff; 16];
        let alpha_bits =
            u64::from_le_bytes(block[0..8].try_into().expect("BC2 alpha block length"));
        for (index, value) in alpha.iter_mut().enumerate() {
            *value = (((alpha_bits >> (index * 4)) & 0x0f) as u8) * 17;
        }
        dds_decode_bc_color_block(&block[8..16], false, alpha, rgba);
        if premultiplied {
            dds_unpremultiply_block(rgba);
        }
    })
}

fn dds_bc3_frame(
    data: &[u8],
    width: usize,
    height: usize,
    premultiplied: bool,
    transform: DdsBc3Transform,
) -> Result<Vec<u8>> {
    dds_decode_blocks(data, width, height, 16, |block, rgba| {
        let alpha = dds_bc_alpha_values(&block[0..8]);
        dds_decode_bc_color_block(&block[8..16], false, alpha, rgba);
        if premultiplied {
            dds_unpremultiply_block(rgba);
        }
        dds_apply_bc3_transform(rgba, transform);
    })
}

fn dds_bc4_frame(data: &[u8], width: usize, height: usize, signed: bool) -> Result<Vec<u8>> {
    dds_decode_blocks(data, width, height, 8, |block, rgba| {
        let values = if signed {
            dds_bc_signed_values(block)
        } else {
            dds_bc_alpha_values(block)
        };
        for (pixel, output) in rgba.chunks_exact_mut(4).enumerate() {
            let value = values[pixel];
            output.copy_from_slice(&[value, value, value, 0xff]);
        }
    })
}

fn dds_bc5_frame(
    data: &[u8],
    width: usize,
    height: usize,
    signed: bool,
    swap_xy: bool,
) -> Result<Vec<u8>> {
    dds_decode_blocks(data, width, height, 16, |block, rgba| {
        let first = if signed {
            dds_bc_signed_values(&block[0..8])
        } else {
            dds_bc_alpha_values(&block[0..8])
        };
        let second = if signed {
            dds_bc_signed_values(&block[8..16])
        } else {
            dds_bc_alpha_values(&block[8..16])
        };
        for (pixel, output) in rgba.chunks_exact_mut(4).enumerate() {
            let (red, green) = if swap_xy {
                (second[pixel], first[pixel])
            } else {
                (first[pixel], second[pixel])
            };
            output.copy_from_slice(&[red, green, dds_bc5_blue(red, green), 0xff]);
        }
    })
}

fn dds_bc5_blue(red: u8, green: u8) -> u8 {
    let red = i32::from(red);
    let green = i32::from(green);
    let residual = (255 * 255 - red * red - green * green) / 2;
    if residual <= 0 {
        127
    } else {
        (f64::from(residual).sqrt().round() as i32).clamp(0, 255) as u8
    }
}

fn dds_normal_rg_block(rgba: &mut [u8; 64]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let red = pixel[0];
        let green = pixel[1];
        pixel.copy_from_slice(&[red, green, dds_bc5_blue(red, green), 0xff]);
    }
}

fn dds_apply_bc3_transform(rgba: &mut [u8; 64], transform: DdsBc3Transform) {
    match transform {
        DdsBc3Transform::None => {}
        DdsBc3Transform::Aexp => {
            for pixel in rgba.chunks_exact_mut(4) {
                let alpha = u16::from(pixel[3]);
                pixel[0] = (u16::from(pixel[0]) * alpha / 255) as u8;
                pixel[1] = (u16::from(pixel[1]) * alpha / 255) as u8;
                pixel[2] = (u16::from(pixel[2]) * alpha / 255) as u8;
                pixel[3] = 0xff;
            }
        }
        DdsBc3Transform::NormalAg => {
            for pixel in rgba.chunks_exact_mut(4) {
                let red = pixel[3];
                let green = pixel[1];
                pixel.copy_from_slice(&[red, green, dds_bc5_blue(red, green), 0xff]);
            }
        }
        DdsBc3Transform::Swizzle(swizzle) => dds_swizzle_bc3_block(rgba, swizzle),
        DdsBc3Transform::Ycocg { scaled } => dds_ycocg_bc3_block(rgba, scaled),
    }
}

fn dds_swizzle_bc3_block(rgba: &mut [u8; 64], swizzle: DdsBc3Swizzle) {
    for pixel in rgba.chunks_exact_mut(4) {
        let [red, green, blue, alpha] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        let output = match swizzle {
            DdsBc3Swizzle::Rbxg => [red, alpha, green, 0],
            DdsBc3Swizzle::Rgxb => [red, green, alpha, 0],
            DdsBc3Swizzle::Rxbg => [red, alpha, blue, 0],
            DdsBc3Swizzle::Rxgb => [alpha, green, blue, red],
            DdsBc3Swizzle::Xgbr => [blue, green, alpha, 0],
            DdsBc3Swizzle::Xgxr => [alpha, green, 0, 0],
            DdsBc3Swizzle::Xrbg => [green, alpha, blue, 0],
        };
        pixel.copy_from_slice(&output);
    }
}

fn dds_ycocg_bc3_block(rgba: &mut [u8; 64], scaled: bool) {
    for pixel in rgba.chunks_exact_mut(4) {
        let scale = if scaled {
            1_i32 << u32::from(pixel[2] >> 3)
        } else {
            1
        };
        let co = (i32::from(pixel[0]) - 128) / scale;
        let cg = (i32::from(pixel[1]) - 128) / scale;
        let y = i32::from(pixel[3]);
        let base = y - cg;
        pixel.copy_from_slice(&[
            clamp_u8_i32(base + co),
            clamp_u8_i32(y + cg),
            clamp_u8_i32(base - co),
            0xff,
        ]);
    }
}

fn dds_unpremultiply_block(rgba: &mut [u8; 64]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * 255) / alpha).min(255) as u8;
        }
    }
}

fn dds_decode_blocks<F>(
    data: &[u8],
    width: usize,
    height: usize,
    block_bytes: usize,
    mut decode: F,
) -> Result<Vec<u8>>
where
    F: FnMut(&[u8], &mut [u8; 64]),
{
    let output_size = dds_packed_size(width, height, 4)?;
    let mut frame = vec![0; output_size];
    let blocks_wide = dds_block_count(width)?;
    let blocks_high = dds_block_count(height)?;
    for block_y in 0..blocks_high {
        for block_x in 0..blocks_wide {
            let block_index = block_y
                .checked_mul(blocks_wide)
                .and_then(|value| value.checked_add(block_x))
                .and_then(|value| value.checked_mul(block_bytes))
                .ok_or_else(|| RmpegError::InvalidData("DDS block offset overflow".to_string()))?;
            let block = data
                .get(block_index..block_index + block_bytes)
                .ok_or_else(|| RmpegError::UnexpectedEof {
                    needed: block_index + block_bytes,
                    remaining: data.len(),
                })?;
            let mut rgba = [0; 64];
            decode(block, &mut rgba);
            for y in 0..4 {
                let out_y = block_y * 4 + y;
                if out_y >= height {
                    continue;
                }
                for x in 0..4 {
                    let out_x = block_x * 4 + x;
                    if out_x >= width {
                        continue;
                    }
                    let src = (y * 4 + x) * 4;
                    let dst = (out_y * width + out_x) * 4;
                    frame[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
                }
            }
        }
    }
    Ok(frame)
}

fn dds_decode_bc_color_block(
    block: &[u8],
    allow_bc1_alpha: bool,
    alpha: [u8; 16],
    rgba: &mut [u8; 64],
) {
    let color0 = u16::from_le_bytes(block[0..2].try_into().expect("BC color0 length"));
    let color1 = u16::from_le_bytes(block[2..4].try_into().expect("BC color1 length"));
    let mut colors = [[0; 4]; 4];
    colors[0] = dds_rgb565_to_rgba(color0, 0xff);
    colors[1] = dds_rgb565_to_rgba(color1, 0xff);
    if !allow_bc1_alpha || color0 > color1 {
        colors[2] = dds_lerp_rgb(colors[0], colors[1], 2, 1, 3, 0xff);
        colors[3] = dds_lerp_rgb(colors[0], colors[1], 1, 2, 3, 0xff);
    } else {
        colors[2] = dds_lerp_rgb(colors[0], colors[1], 1, 1, 2, 0xff);
        colors[3] = [0, 0, 0, 0];
    }
    let indices = u32::from_le_bytes(block[4..8].try_into().expect("BC color index length"));
    for (pixel, output) in rgba.chunks_exact_mut(4).enumerate() {
        let index = ((indices >> (pixel * 2)) & 0x03) as usize;
        output.copy_from_slice(&colors[index]);
        if !allow_bc1_alpha || index != 3 || color0 > color1 {
            output[3] = alpha[pixel];
        }
    }
}

fn dds_rgb565_to_rgba(value: u16, alpha: u8) -> [u8; 4] {
    let red = ((value >> 11) & 0x1f) as u8;
    let green = ((value >> 5) & 0x3f) as u8;
    let blue = (value & 0x1f) as u8;
    [
        DDS_EXPAND_5_TO_8[usize::from(red)],
        DDS_EXPAND_6_TO_8[usize::from(green)],
        DDS_EXPAND_5_TO_8[usize::from(blue)],
        alpha,
    ]
}

fn dds_lerp_rgb(
    first: [u8; 4],
    second: [u8; 4],
    first_weight: u16,
    second_weight: u16,
    divisor: u16,
    alpha: u8,
) -> [u8; 4] {
    [
        ((u16::from(first[0]) * first_weight + u16::from(second[0]) * second_weight) / divisor)
            as u8,
        ((u16::from(first[1]) * first_weight + u16::from(second[1]) * second_weight) / divisor)
            as u8,
        ((u16::from(first[2]) * first_weight + u16::from(second[2]) * second_weight) / divisor)
            as u8,
        alpha,
    ]
}

fn dds_bc_alpha_values(block: &[u8]) -> [u8; 16] {
    let alpha0 = block[0];
    let alpha1 = block[1];
    let mut table = [0; 8];
    table[0] = alpha0;
    table[1] = alpha1;
    if alpha0 > alpha1 {
        for i in 1..=6 {
            let weight = i as u16;
            table[i + 1] =
                ((u16::from(alpha0) * (7 - weight) + u16::from(alpha1) * weight) / 7) as u8;
        }
    } else {
        for i in 1..=4 {
            let weight = i as u16;
            table[i + 1] =
                ((u16::from(alpha0) * (5 - weight) + u16::from(alpha1) * weight) / 5) as u8;
        }
        table[6] = 0;
        table[7] = 255;
    }

    let mut index_bits = 0_u64;
    for (shift, byte) in block[2..8].iter().enumerate() {
        index_bits |= u64::from(*byte) << (shift * 8);
    }
    let mut alpha = [0; 16];
    for (pixel, value) in alpha.iter_mut().enumerate() {
        let index = ((index_bits >> (pixel * 3)) & 0x07) as usize;
        *value = table[index];
    }
    alpha
}

fn dds_bc_signed_values(block: &[u8]) -> [u8; 16] {
    let value0 = block[0] as i8 as i16;
    let value1 = block[1] as i8 as i16;
    let mut table = [0_i16; 8];
    table[0] = value0;
    table[1] = value1;
    if value0 > value1 {
        for i in 1..=6 {
            let weight = i as i16;
            table[i + 1] = (value0 * (7 - weight) + value1 * weight).div_euclid(7);
        }
    } else {
        for i in 1..=4 {
            let weight = i as i16;
            table[i + 1] = (value0 * (5 - weight) + value1 * weight).div_euclid(5);
        }
        table[6] = -128;
        table[7] = 127;
    }

    let mut index_bits = 0_u64;
    for (shift, byte) in block[2..8].iter().enumerate() {
        index_bits |= u64::from(*byte) << (shift * 8);
    }
    let mut values = [0; 16];
    for (pixel, value) in values.iter_mut().enumerate() {
        let index = ((index_bits >> (pixel * 3)) & 0x07) as usize;
        *value = dds_signed_to_unorm(table[index]);
    }
    values
}

fn dds_signed_to_unorm(value: i16) -> u8 {
    (value.clamp(-128, 127) + 128) as u8
}

fn clamp_u8_i32(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

fn dds_packed_size(width: usize, height: usize, bytes_per_pixel: usize) -> Result<usize> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .ok_or_else(|| RmpegError::InvalidData("DDS frame size overflow".to_string()))
}

fn dds_block_data_size(width: usize, height: usize, bytes_per_block: usize) -> Result<usize> {
    dds_block_count(width)?
        .checked_mul(dds_block_count(height)?)
        .and_then(|blocks| blocks.checked_mul(bytes_per_block))
        .ok_or_else(|| RmpegError::InvalidData("DDS block data size overflow".to_string()))
}

fn dds_block_count(value: usize) -> Result<usize> {
    value
        .checked_add(3)
        .map(|value| value / 4)
        .ok_or_else(|| RmpegError::InvalidData("DDS block count overflow".to_string()))
}

fn usize_from_u32(value: u32, label: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| RmpegError::Unsupported(format!("{label} is too large")))
}

fn pnm_header(bytes: &[u8]) -> Result<PnmHeader> {
    let magic = bytes.get(0..2).ok_or(RmpegError::UnexpectedEof {
        needed: 2,
        remaining: bytes.len(),
    })?;
    if !matches!(magic, b"P4" | b"P5" | b"P6") {
        return Err(RmpegError::InvalidData(
            "missing binary PNM signature".to_string(),
        ));
    }

    let mut reader = PnmHeaderReader::new(bytes, 2);
    let width = pnm_positive_usize(reader.next_u32("PNM width")?, "PNM width")?;
    let height = pnm_positive_usize(reader.next_u32("PNM height")?, "PNM height")?;
    let max_value = if magic == b"P4" {
        1
    } else {
        let max_value = reader.next_u32("PNM max value")?;
        if max_value == 0 || max_value > 65_535 {
            return Err(RmpegError::InvalidData(format!(
                "invalid PNM max value {max_value}"
            )));
        }
        max_value
    };
    let data_offset = reader.raster_offset()?;

    Ok(PnmHeader {
        magic: [magic[0], magic[1]],
        width,
        height,
        max_value,
        data_offset,
    })
}

fn pnm_positive_usize(value: u32, label: &str) -> Result<usize> {
    if value == 0 {
        return Err(RmpegError::InvalidData(format!("{label} must be nonzero")));
    }
    usize::try_from(value).map_err(|_| RmpegError::Unsupported(format!("{label} is too large")))
}

fn pnm_frame(bytes: &[u8], header: &PnmHeader) -> Result<Vec<u8>> {
    match &header.magic {
        b"P4" => pnm_packed_bitmap(bytes, header),
        b"P5" => pnm_component_frame(bytes, header, 1),
        b"P6" => pnm_component_frame(bytes, header, 3),
        _ => unreachable!("PNM magic was validated"),
    }
}

fn pnm_packed_bitmap(bytes: &[u8], header: &PnmHeader) -> Result<Vec<u8>> {
    let row_bytes = header
        .width
        .checked_add(7)
        .map(|bits| bits / 8)
        .ok_or_else(|| RmpegError::InvalidData("PNM row size overflow".to_string()))?;
    let frame_size = row_bytes
        .checked_mul(header.height)
        .ok_or_else(|| RmpegError::InvalidData("PNM frame size overflow".to_string()))?;
    let end = header
        .data_offset
        .checked_add(frame_size)
        .ok_or_else(|| RmpegError::InvalidData("PNM data range overflow".to_string()))?;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(bytes[header.data_offset..end].to_vec())
}

fn pnm_component_frame(bytes: &[u8], header: &PnmHeader, channels: usize) -> Result<Vec<u8>> {
    let components = header
        .width
        .checked_mul(header.height)
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or_else(|| RmpegError::InvalidData("PNM component count overflow".to_string()))?;
    let bytes_per_component = if header.max_value > 255 { 2 } else { 1 };
    let data_bytes = components
        .checked_mul(bytes_per_component)
        .ok_or_else(|| RmpegError::InvalidData("PNM data size overflow".to_string()))?;
    let end = header
        .data_offset
        .checked_add(data_bytes)
        .ok_or_else(|| RmpegError::InvalidData("PNM data range overflow".to_string()))?;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }

    if bytes_per_component == 1 {
        if header.max_value == 255 {
            return Ok(bytes[header.data_offset..end].to_vec());
        }
        let mut frame = Vec::with_capacity(components);
        for &sample in &bytes[header.data_offset..end] {
            frame.push(pnm_scale_u8(u32::from(sample), header.max_value));
        }
        return Ok(frame);
    }

    let mut frame = Vec::with_capacity(components * 2);
    for sample in bytes[header.data_offset..end].chunks_exact(2) {
        let value = u32::from(u16::from_be_bytes([sample[0], sample[1]]));
        frame.extend_from_slice(&pnm_scale_u16(value, header.max_value).to_le_bytes());
    }
    Ok(frame)
}

fn pnm_scale_u8(value: u32, max_value: u32) -> u8 {
    if max_value == 255 {
        value as u8
    } else {
        ((value as f64 * 255.0 / max_value as f64).round()).clamp(0.0, 255.0) as u8
    }
}

fn pnm_scale_u16(value: u32, max_value: u32) -> u16 {
    if max_value == 65_535 {
        value as u16
    } else {
        ((value as f64 * 65535.0 / max_value as f64).round()).clamp(0.0, 65535.0) as u16
    }
}

struct SgiHeader {
    storage: u8,
    bytes_per_channel: usize,
    width: usize,
    height: usize,
    channels: usize,
}

fn sgi_header(bytes: &[u8]) -> Result<SgiHeader> {
    if bytes.len() < SGI_HEADER_LEN {
        return Err(RmpegError::UnexpectedEof {
            needed: SGI_HEADER_LEN,
            remaining: bytes.len(),
        });
    }
    if &bytes[0..2] != b"\x01\xda" {
        return Err(RmpegError::InvalidData("missing SGI signature".to_string()));
    }

    let storage = bytes[2];
    if !matches!(storage, SGI_STORAGE_VERBATIM | SGI_STORAGE_RLE) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported SGI storage mode {storage}"
        )));
    }
    let bytes_per_channel = bytes[3];
    if !matches!(bytes_per_channel, 1 | 2) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported SGI bytes per channel {bytes_per_channel}"
        )));
    }
    let dimensions = read_u16_be(bytes, 4)?;
    if !(1..=3).contains(&dimensions) {
        return Err(RmpegError::InvalidData(format!(
            "unsupported SGI dimension count {dimensions}"
        )));
    }
    let width = usize::from(read_u16_be(bytes, 6)?);
    let height = usize::from(read_u16_be(bytes, 8)?);
    let channels = usize::from(read_u16_be(bytes, 10)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "SGI dimensions must be nonzero".to_string(),
        ));
    }
    if !(1..=4).contains(&channels) {
        return Err(RmpegError::Unsupported(format!(
            "unsupported SGI channel count {channels}"
        )));
    }

    Ok(SgiHeader {
        storage,
        bytes_per_channel: usize::from(bytes_per_channel),
        width,
        height,
        channels,
    })
}

fn sgi_planes(bytes: &[u8], header: &SgiHeader) -> Result<Vec<Vec<u8>>> {
    let row_bytes = sgi_row_bytes(header)?;
    let plane_size = row_bytes
        .checked_mul(header.height)
        .ok_or_else(|| RmpegError::InvalidData("SGI plane size overflow".to_string()))?;
    let mut planes = vec![vec![0_u8; plane_size]; header.channels];
    if header.storage == SGI_STORAGE_VERBATIM {
        let data_bytes = plane_size
            .checked_mul(header.channels)
            .ok_or_else(|| RmpegError::InvalidData("SGI data size overflow".to_string()))?;
        let end = SGI_HEADER_LEN
            .checked_add(data_bytes)
            .ok_or_else(|| RmpegError::InvalidData("SGI data range overflow".to_string()))?;
        if end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: end,
                remaining: bytes.len(),
            });
        }
        let mut pos = SGI_HEADER_LEN;
        for plane in &mut planes {
            plane.copy_from_slice(&bytes[pos..pos + plane_size]);
            pos += plane_size;
        }
        return Ok(planes);
    }

    let table_entries = header
        .height
        .checked_mul(header.channels)
        .ok_or_else(|| RmpegError::InvalidData("SGI row table size overflow".to_string()))?;
    let table_bytes = table_entries
        .checked_mul(8)
        .ok_or_else(|| RmpegError::InvalidData("SGI row table byte size overflow".to_string()))?;
    let table_end = SGI_HEADER_LEN
        .checked_add(table_bytes)
        .ok_or_else(|| RmpegError::InvalidData("SGI row table range overflow".to_string()))?;
    if table_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: table_end,
            remaining: bytes.len(),
        });
    }

    let length_table = SGI_HEADER_LEN + table_entries * 4;
    for (channel, plane) in planes.iter_mut().enumerate() {
        for row in 0..header.height {
            let entry = channel * header.height + row;
            let start = usize::try_from(read_u32_be(bytes, SGI_HEADER_LEN + entry * 4)?)
                .map_err(|_| RmpegError::Unsupported("SGI row offset is too large".to_string()))?;
            let _length = read_u32_be(bytes, length_table + entry * 4)?;
            let decoded = sgi_rle_row(bytes, start, header.bytes_per_channel, row_bytes)?;
            let dst = row * row_bytes;
            plane[dst..dst + row_bytes].copy_from_slice(&decoded);
        }
    }
    Ok(planes)
}

fn sgi_rle_row(
    bytes: &[u8],
    start: usize,
    bytes_per_channel: usize,
    row_bytes: usize,
) -> Result<Vec<u8>> {
    if start > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: start,
            remaining: bytes.len(),
        });
    }
    let mut pos = start;
    let mut row = Vec::with_capacity(row_bytes);
    loop {
        let control = if bytes_per_channel == 1 {
            if pos >= bytes.len() {
                return Err(RmpegError::UnexpectedEof {
                    needed: pos + 1,
                    remaining: bytes.len(),
                });
            }
            let control = u16::from(bytes[pos]);
            pos += 1;
            control
        } else {
            if pos + 2 > bytes.len() {
                return Err(RmpegError::UnexpectedEof {
                    needed: pos + 2,
                    remaining: bytes.len(),
                });
            }
            let control = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]);
            pos += 2;
            control
        };
        let count = usize::from(control & 0x7f);
        if count == 0 {
            break;
        }
        if control & 0x80 != 0 {
            let literal_bytes = count.checked_mul(bytes_per_channel).ok_or_else(|| {
                RmpegError::InvalidData("SGI literal packet size overflow".to_string())
            })?;
            if pos + literal_bytes > bytes.len() {
                return Err(RmpegError::UnexpectedEof {
                    needed: pos + literal_bytes,
                    remaining: bytes.len(),
                });
            }
            row.extend_from_slice(&bytes[pos..pos + literal_bytes]);
            pos += literal_bytes;
        } else {
            if pos + bytes_per_channel > bytes.len() {
                return Err(RmpegError::UnexpectedEof {
                    needed: pos + bytes_per_channel,
                    remaining: bytes.len(),
                });
            }
            let sample = &bytes[pos..pos + bytes_per_channel];
            pos += bytes_per_channel;
            for _ in 0..count {
                row.extend_from_slice(sample);
            }
        }
        if row.len() > row_bytes {
            return Err(RmpegError::InvalidData(
                "SGI RLE row expands past expected size".to_string(),
            ));
        }
    }
    if row.len() != row_bytes {
        return Err(RmpegError::InvalidData(format!(
            "SGI RLE row decoded to {} bytes, expected {row_bytes}",
            row.len()
        )));
    }
    Ok(row)
}

fn sgi_frame(planes: &[Vec<u8>], header: &SgiHeader) -> Result<Vec<u8>> {
    let row_bytes = sgi_row_bytes(header)?;
    let frame_size = row_bytes
        .checked_mul(header.height)
        .and_then(|plane_size| plane_size.checked_mul(header.channels))
        .ok_or_else(|| RmpegError::InvalidData("SGI frame size overflow".to_string()))?;
    let mut frame = Vec::with_capacity(frame_size);
    for &channel in sgi_output_order(header.channels) {
        let plane = &planes[channel];
        for out_y in 0..header.height {
            let src_y = header.height - 1 - out_y;
            let row_start = src_y * row_bytes;
            frame.extend_from_slice(&plane[row_start..row_start + row_bytes]);
        }
    }
    Ok(frame)
}

fn sgi_output_order(channels: usize) -> &'static [usize] {
    match channels {
        1 => &[0],
        2 => &[0, 1],
        3 => &[1, 2, 0],
        4 => &[1, 2, 0, 3],
        _ => unreachable!("SGI channel count was validated"),
    }
}

fn sgi_row_bytes(header: &SgiHeader) -> Result<usize> {
    header
        .width
        .checked_mul(header.bytes_per_channel)
        .ok_or_else(|| RmpegError::InvalidData("SGI row size overflow".to_string()))
}

struct PnmHeaderReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> PnmHeaderReader<'a> {
    fn new(bytes: &'a [u8], pos: usize) -> Self {
        Self { bytes, pos }
    }

    fn next_u32(&mut self, label: &str) -> Result<u32> {
        self.skip_spacing_and_comments();
        let start = self.pos;
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(RmpegError::InvalidData(format!("missing {label}")));
        }
        let token = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| RmpegError::InvalidData(format!("{label} is not valid ASCII digits")))?;
        token
            .parse()
            .map_err(|_| RmpegError::InvalidData(format!("{label} is too large")))
    }

    fn raster_offset(&mut self) -> Result<usize> {
        let Some(&separator) = self.bytes.get(self.pos) else {
            return Err(RmpegError::UnexpectedEof {
                needed: self.pos + 1,
                remaining: self.bytes.len(),
            });
        };
        if !separator.is_ascii_whitespace() {
            return Err(RmpegError::InvalidData(
                "PNM raster is missing header separator".to_string(),
            ));
        }
        self.pos += 1;
        if separator == b'\r' && self.bytes.get(self.pos) == Some(&b'\n') {
            self.pos += 1;
        }
        Ok(self.pos)
    }

    fn skip_spacing_and_comments(&mut self) {
        loop {
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                self.pos += 1;
            }
            if self.bytes.get(self.pos) != Some(&b'#') {
                return;
            }
            while self
                .bytes
                .get(self.pos)
                .is_some_and(|byte| *byte != b'\n' && *byte != b'\r')
            {
                self.pos += 1;
            }
        }
    }
}

struct BmpHeader {
    width: usize,
    height: usize,
    top_down: bool,
    bits_per_pixel: u16,
    compression: u32,
    pixel_offset: usize,
    palette_offset: usize,
    palette_entry_size: usize,
    palette_entries: usize,
}

fn bmp_header(bytes: &[u8]) -> Result<BmpHeader> {
    if bytes.len() < 26 {
        return Err(RmpegError::UnexpectedEof {
            needed: 26,
            remaining: bytes.len(),
        });
    }
    if &bytes[..2] != b"BM" {
        return Err(RmpegError::InvalidData("missing BMP signature".to_string()));
    }

    let pixel_offset = usize::try_from(read_u32_le(bytes, 10)?)
        .map_err(|_| RmpegError::Unsupported("BMP pixel offset is too large".to_string()))?;
    let dib_size = read_u32_le(bytes, 14)?;
    let dib_end = BMP_FILE_HEADER_LEN
        .checked_add(
            usize::try_from(dib_size)
                .map_err(|_| RmpegError::Unsupported("BMP DIB header is too large".to_string()))?,
        )
        .ok_or_else(|| RmpegError::InvalidData("BMP DIB header range overflow".to_string()))?;
    if dib_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: dib_end,
            remaining: bytes.len(),
        });
    }

    let (width, height, top_down, bits_per_pixel, compression, colors_used) = match dib_size {
        12 => {
            let width = u32::from(read_u16_le(bytes, 18)?);
            let height = u32::from(read_u16_le(bytes, 20)?);
            let planes = read_u16_le(bytes, 22)?;
            if planes != 1 {
                return Err(RmpegError::InvalidData(format!(
                    "invalid BMP plane count {planes}"
                )));
            }
            (width, height, false, read_u16_le(bytes, 24)?, BI_RGB, 0)
        }
        40 | 52 | 56 | 64 | 108 | 124 => {
            let width = read_i32_le(bytes, 18)?;
            let height = read_i32_le(bytes, 22)?;
            if width <= 0 {
                return Err(RmpegError::InvalidData(format!(
                    "invalid BMP width {width}"
                )));
            }
            let planes = read_u16_le(bytes, 26)?;
            if planes != 1 {
                return Err(RmpegError::InvalidData(format!(
                    "invalid BMP plane count {planes}"
                )));
            }
            let height_abs = height
                .checked_abs()
                .ok_or_else(|| RmpegError::InvalidData("invalid BMP height".to_string()))?;
            (
                width as u32,
                height_abs as u32,
                height < 0,
                read_u16_le(bytes, 28)?,
                read_u32_le(bytes, 30)?,
                read_u32_le(bytes, 46).unwrap_or(0),
            )
        }
        _ => {
            return Err(RmpegError::InvalidData(format!(
                "unsupported BMP DIB header size {dib_size}"
            )));
        }
    };
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "BMP dimensions must be nonzero".to_string(),
        ));
    }
    if pixel_offset > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: pixel_offset,
            remaining: bytes.len(),
        });
    }

    let palette_offset = if compression == BI_BITFIELDS && dib_size == 40 {
        dib_end
            .checked_add(12)
            .ok_or_else(|| RmpegError::InvalidData("BMP bitfield range overflow".to_string()))?
    } else {
        dib_end
    };
    if pixel_offset < palette_offset {
        return Err(RmpegError::InvalidData(
            "BMP pixel data overlaps metadata".to_string(),
        ));
    }

    let palette_entry_size = if dib_size == 12 { 3 } else { 4 };
    let palette_entries = if matches!(bits_per_pixel, 1 | 4 | 8) {
        let default_entries = 1_usize << bits_per_pixel;
        let declared_entries = usize::try_from(colors_used)
            .map_err(|_| RmpegError::Unsupported("BMP palette is too large".to_string()))?;
        let available_entries = (pixel_offset - palette_offset) / palette_entry_size;
        let requested_entries = if declared_entries == 0 {
            default_entries
        } else {
            declared_entries
        };
        requested_entries.min(available_entries).min(256)
    } else {
        0
    };

    Ok(BmpHeader {
        width: usize::try_from(width)
            .map_err(|_| RmpegError::Unsupported("BMP width is too large".to_string()))?,
        height: usize::try_from(height)
            .map_err(|_| RmpegError::Unsupported("BMP height is too large".to_string()))?,
        top_down,
        bits_per_pixel,
        compression,
        pixel_offset,
        palette_offset,
        palette_entry_size,
        palette_entries,
    })
}

fn bmp_pal8_frame(mut indexes: Vec<u8>, palette: [u8; BMP_PALETTE_BYTES]) -> Vec<u8> {
    indexes.extend_from_slice(&palette);
    indexes
}

fn bmp_palette(bytes: &[u8], header: &BmpHeader) -> Result<[u8; BMP_PALETTE_BYTES]> {
    let mut palette = [0_u8; BMP_PALETTE_BYTES];
    for entry in 0..header.palette_entries {
        let src = header
            .palette_offset
            .checked_add(entry * header.palette_entry_size)
            .ok_or_else(|| RmpegError::InvalidData("BMP palette offset overflow".to_string()))?;
        if src + 3 > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: src + 3,
                remaining: bytes.len(),
            });
        }
        let dst = entry * 4;
        palette[dst..dst + 3].copy_from_slice(&bytes[src..src + 3]);
        palette[dst + 3] = 0xff;
    }
    Ok(palette)
}

fn bmp_uncompressed_indexes(bytes: &[u8], header: &BmpHeader) -> Result<Vec<u8>> {
    let stride = bmp_stride(header.width, header.bits_per_pixel)?;
    let mut frame = vec![0_u8; bmp_pixel_count(header)?];
    for out_y in 0..header.height {
        let src_y = bmp_source_row(header, out_y);
        let row_start = header
            .pixel_offset
            .checked_add(src_y * stride)
            .ok_or_else(|| RmpegError::InvalidData("BMP row offset overflow".to_string()))?;
        if row_start + stride > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: row_start + stride,
                remaining: bytes.len(),
            });
        }
        let row = &bytes[row_start..row_start + stride];
        let dst = out_y * header.width;
        match header.bits_per_pixel {
            1 => {
                for x in 0..header.width {
                    let byte = row[x / 8];
                    frame[dst + x] = (byte >> (7 - (x % 8))) & 1;
                }
            }
            4 => {
                for x in 0..header.width {
                    let byte = row[x / 2];
                    frame[dst + x] = if x % 2 == 0 { byte >> 4 } else { byte & 0x0f };
                }
            }
            8 => frame[dst..dst + header.width].copy_from_slice(&row[..header.width]),
            _ => {
                return Err(RmpegError::Unsupported(format!(
                    "unsupported BMP indexed bit depth {}",
                    header.bits_per_pixel
                )));
            }
        }
    }
    Ok(frame)
}

fn bmp_packed_frame(bytes: &[u8], header: &BmpHeader, bytes_per_pixel: usize) -> Result<Vec<u8>> {
    let stride = bmp_stride(header.width, header.bits_per_pixel)?;
    let row_bytes = header
        .width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| RmpegError::InvalidData("BMP row size overflow".to_string()))?;
    let mut frame = Vec::with_capacity(
        row_bytes
            .checked_mul(header.height)
            .ok_or_else(|| RmpegError::InvalidData("BMP frame size overflow".to_string()))?,
    );
    for out_y in 0..header.height {
        let src_y = bmp_source_row(header, out_y);
        let row_start = header
            .pixel_offset
            .checked_add(src_y * stride)
            .ok_or_else(|| RmpegError::InvalidData("BMP row offset overflow".to_string()))?;
        if row_start + row_bytes > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: row_start + row_bytes,
                remaining: bytes.len(),
            });
        }
        frame.extend_from_slice(&bytes[row_start..row_start + row_bytes]);
    }
    Ok(frame)
}

fn bmp_bgr0_frame(bytes: &[u8], header: &BmpHeader) -> Result<Vec<u8>> {
    let source = bmp_packed_frame(bytes, header, 4)?;
    let mut frame = source;
    for pixel in frame.chunks_exact_mut(4) {
        pixel[3] = 0;
    }
    Ok(frame)
}

fn bmp_rle8_indexes(bytes: &[u8], header: &BmpHeader) -> Result<Vec<u8>> {
    bmp_rle_indexes(bytes, header, false)
}

fn bmp_rle4_indexes(bytes: &[u8], header: &BmpHeader) -> Result<Vec<u8>> {
    bmp_rle_indexes(bytes, header, true)
}

fn bmp_rle_indexes(bytes: &[u8], header: &BmpHeader, four_bit: bool) -> Result<Vec<u8>> {
    let mut frame = vec![0_u8; bmp_pixel_count(header)?];
    let mut pos = header.pixel_offset;
    let mut x = 0_usize;
    let mut y = if header.top_down {
        0_isize
    } else {
        isize::try_from(header.height - 1)
            .map_err(|_| RmpegError::Unsupported("BMP height is too large".to_string()))?
    };

    while pos + 2 <= bytes.len() && y >= 0 && (y as usize) < header.height {
        let count = bytes[pos];
        let value = bytes[pos + 1];
        pos += 2;
        if count != 0 {
            for pixel in 0..usize::from(count) {
                let index = if four_bit {
                    if pixel % 2 == 0 {
                        value >> 4
                    } else {
                        value & 0x0f
                    }
                } else {
                    value
                };
                bmp_put_index(&mut frame, header, x, y as usize, index)?;
                x += 1;
            }
            continue;
        }

        match value {
            0 => {
                x = 0;
                y = bmp_next_rle_row(header, y);
            }
            1 => return Ok(frame),
            2 => {
                if pos + 2 > bytes.len() {
                    return Err(RmpegError::UnexpectedEof {
                        needed: pos + 2,
                        remaining: bytes.len(),
                    });
                }
                x = x
                    .checked_add(usize::from(bytes[pos]))
                    .ok_or_else(|| RmpegError::InvalidData("BMP RLE x overflow".to_string()))?;
                let dy = isize::from(bytes[pos + 1]);
                y = if header.top_down { y + dy } else { y - dy };
                pos += 2;
            }
            literal_count => {
                let literal_count = usize::from(literal_count);
                if four_bit {
                    let byte_count = literal_count.div_ceil(2);
                    if pos + byte_count > bytes.len() {
                        return Err(RmpegError::UnexpectedEof {
                            needed: pos + byte_count,
                            remaining: bytes.len(),
                        });
                    }
                    for pixel in 0..literal_count {
                        let byte = bytes[pos + pixel / 2];
                        let index = if pixel % 2 == 0 {
                            byte >> 4
                        } else {
                            byte & 0x0f
                        };
                        bmp_put_index(&mut frame, header, x, y as usize, index)?;
                        x += 1;
                    }
                    pos += byte_count + (byte_count % 2);
                } else {
                    if pos + literal_count > bytes.len() {
                        return Err(RmpegError::UnexpectedEof {
                            needed: pos + literal_count,
                            remaining: bytes.len(),
                        });
                    }
                    for &index in &bytes[pos..pos + literal_count] {
                        bmp_put_index(&mut frame, header, x, y as usize, index)?;
                        x += 1;
                    }
                    pos += literal_count + (literal_count % 2);
                }
            }
        }
    }

    Ok(frame)
}

fn bmp_put_index(
    frame: &mut [u8],
    header: &BmpHeader,
    x: usize,
    y: usize,
    index: u8,
) -> Result<()> {
    if y >= header.height {
        return Err(RmpegError::InvalidData(
            "BMP RLE pixel is outside the frame".to_string(),
        ));
    }
    if x >= header.width {
        return Ok(());
    }
    frame[y * header.width + x] = index;
    Ok(())
}

fn bmp_next_rle_row(header: &BmpHeader, y: isize) -> isize {
    if header.top_down {
        y + 1
    } else {
        y - 1
    }
}

fn bmp_source_row(header: &BmpHeader, out_y: usize) -> usize {
    if header.top_down {
        out_y
    } else {
        header.height - 1 - out_y
    }
}

fn bmp_stride(width: usize, bits_per_pixel: u16) -> Result<usize> {
    let bits = width
        .checked_mul(usize::from(bits_per_pixel))
        .ok_or_else(|| RmpegError::InvalidData("BMP row size overflow".to_string()))?;
    bits.div_ceil(32)
        .checked_mul(4)
        .ok_or_else(|| RmpegError::InvalidData("BMP row stride overflow".to_string()))
}

fn bmp_pixel_count(header: &BmpHeader) -> Result<usize> {
    header
        .width
        .checked_mul(header.height)
        .ok_or_else(|| RmpegError::InvalidData("BMP pixel count overflow".to_string()))
}

struct TgaHeader {
    color_map_first: usize,
    color_map_len: usize,
    color_map_depth: u8,
    image_type: u8,
    width: usize,
    height: usize,
    pixel_depth: u8,
    descriptor: u8,
    color_map_offset: usize,
    pixel_offset: usize,
    image_end: usize,
}

fn tga_header(bytes: &[u8]) -> Result<TgaHeader> {
    if bytes.len() < TGA_HEADER_LEN + TGA_FOOTER_LEN {
        return Err(RmpegError::UnexpectedEof {
            needed: TGA_HEADER_LEN + TGA_FOOTER_LEN,
            remaining: bytes.len(),
        });
    }
    if &bytes[bytes.len() - TGA_FOOTER_SIGNATURE.len()..] != TGA_FOOTER_SIGNATURE {
        return Err(RmpegError::InvalidData(
            "missing TGA 2.0 footer signature".to_string(),
        ));
    }

    let id_len = usize::from(bytes[0]);
    let color_map_type = bytes[1];
    let image_type = bytes[2];
    let color_map_first = usize::from(read_u16_le(bytes, 3)?);
    let color_map_len = usize::from(read_u16_le(bytes, 5)?);
    let color_map_depth = bytes[7];
    let width = usize::from(read_u16_le(bytes, 12)?);
    let height = usize::from(read_u16_le(bytes, 14)?);
    let pixel_depth = bytes[16];
    let descriptor = bytes[17];
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "TGA dimensions must be nonzero".to_string(),
        ));
    }
    if descriptor & 0xc0 != 0 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported TGA descriptor {descriptor:#04x}"
        )));
    }

    let image_end = bytes.len() - TGA_FOOTER_LEN;
    let color_map_offset = TGA_HEADER_LEN
        .checked_add(id_len)
        .ok_or_else(|| RmpegError::InvalidData("TGA ID offset overflow".to_string()))?;
    let color_map_bytes = color_map_len
        .checked_mul(tga_bytes_per_pixel(color_map_depth))
        .ok_or_else(|| RmpegError::InvalidData("TGA color map is too large".to_string()))?;
    let pixel_offset = color_map_offset
        .checked_add(color_map_bytes)
        .ok_or_else(|| RmpegError::InvalidData("TGA pixel offset overflow".to_string()))?;
    if pixel_offset > image_end {
        return Err(RmpegError::UnexpectedEof {
            needed: pixel_offset,
            remaining: image_end,
        });
    }

    match image_type {
        1 | 9 => {
            if color_map_type != 1 || color_map_len == 0 {
                return Err(RmpegError::InvalidData(
                    "color-mapped TGA requires a color map".to_string(),
                ));
            }
            if !matches!(pixel_depth, 8 | 15 | 16) {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported TGA index depth {pixel_depth}"
                )));
            }
            if !matches!(color_map_depth, 15 | 16 | 24 | 32) {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported TGA color map depth {color_map_depth}"
                )));
            }
        }
        2 | 10 => {
            if color_map_type != 0 || color_map_len != 0 {
                return Err(RmpegError::InvalidData(
                    "true-color TGA must not declare a color map".to_string(),
                ));
            }
            if !matches!(pixel_depth, 15 | 16 | 24 | 32) {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported TGA pixel depth {pixel_depth}"
                )));
            }
        }
        3 | 11 => {
            if color_map_type != 0 || color_map_len != 0 {
                return Err(RmpegError::InvalidData(
                    "grayscale TGA must not declare a color map".to_string(),
                ));
            }
            if !matches!(pixel_depth, 8 | 16) {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported TGA grayscale depth {pixel_depth}"
                )));
            }
        }
        _ => {
            return Err(RmpegError::Unsupported(format!(
                "unsupported TGA image type {image_type}"
            )));
        }
    }

    Ok(TgaHeader {
        color_map_first,
        color_map_len,
        color_map_depth,
        image_type,
        width,
        height,
        pixel_depth,
        descriptor,
        color_map_offset,
        pixel_offset,
        image_end,
    })
}

fn tga_pal8_frame(bytes: &[u8], header: &TgaHeader) -> Result<Vec<u8>> {
    let mut frame = tga_pixels(bytes, header, tga_bytes_per_pixel(header.pixel_depth))?;
    frame.extend_from_slice(&tga_palette(bytes, header)?);
    Ok(frame)
}

fn tga_packed_frame(bytes: &[u8], header: &TgaHeader) -> Result<Vec<u8>> {
    tga_pixels(bytes, header, tga_bytes_per_pixel(header.pixel_depth))
}

fn tga_pixels(bytes: &[u8], header: &TgaHeader, bytes_per_pixel: usize) -> Result<Vec<u8>> {
    let pixels = header
        .width
        .checked_mul(header.height)
        .ok_or_else(|| RmpegError::InvalidData("TGA pixel count overflow".to_string()))?;
    let frame_size = pixels
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| RmpegError::InvalidData("TGA frame size overflow".to_string()))?;
    let mut frame = vec![0_u8; frame_size];
    if matches!(header.image_type, 1..=3) {
        let needed = header
            .pixel_offset
            .checked_add(frame_size)
            .ok_or_else(|| RmpegError::InvalidData("TGA image data overflow".to_string()))?;
        if needed > header.image_end {
            return Err(RmpegError::UnexpectedEof {
                needed,
                remaining: header.image_end,
            });
        }
        let mut pos = header.pixel_offset;
        for storage_y in 0..header.height {
            for storage_x in 0..header.width {
                tga_put_pixel(
                    &mut frame,
                    header,
                    storage_x,
                    storage_y,
                    bytes_per_pixel,
                    &bytes[pos..pos + bytes_per_pixel],
                );
                pos += bytes_per_pixel;
            }
        }
        return Ok(frame);
    }

    let mut pos = header.pixel_offset;
    let mut storage_x = 0_usize;
    let mut storage_y = 0_usize;
    while storage_y < header.height {
        if pos >= header.image_end {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 1,
                remaining: header.image_end,
            });
        }
        let packet = bytes[pos];
        pos += 1;
        let count = usize::from(packet & 0x7f) + 1;
        if packet & 0x80 != 0 {
            let end = pos + bytes_per_pixel;
            if end > header.image_end {
                return Err(RmpegError::UnexpectedEof {
                    needed: end,
                    remaining: header.image_end,
                });
            }
            for _ in 0..count {
                tga_put_pixel(
                    &mut frame,
                    header,
                    storage_x,
                    storage_y,
                    bytes_per_pixel,
                    &bytes[pos..end],
                );
                tga_advance_pixel(header, &mut storage_x, &mut storage_y);
            }
            pos = end;
        } else {
            let end = pos
                .checked_add(count * bytes_per_pixel)
                .ok_or_else(|| RmpegError::InvalidData("TGA RLE packet overflow".to_string()))?;
            if end > header.image_end {
                return Err(RmpegError::UnexpectedEof {
                    needed: end,
                    remaining: header.image_end,
                });
            }
            for pixel in bytes[pos..end].chunks_exact(bytes_per_pixel) {
                tga_put_pixel(
                    &mut frame,
                    header,
                    storage_x,
                    storage_y,
                    bytes_per_pixel,
                    pixel,
                );
                tga_advance_pixel(header, &mut storage_x, &mut storage_y);
            }
            pos = end;
        }
    }
    Ok(frame)
}

fn tga_put_pixel(
    frame: &mut [u8],
    header: &TgaHeader,
    storage_x: usize,
    storage_y: usize,
    bytes_per_pixel: usize,
    pixel: &[u8],
) {
    if storage_y >= header.height {
        return;
    }
    let x = if header.descriptor & 0x10 != 0 {
        header.width - 1 - storage_x
    } else {
        storage_x
    };
    let y = if header.descriptor & 0x20 != 0 {
        storage_y
    } else {
        header.height - 1 - storage_y
    };
    let offset = (y * header.width + x) * bytes_per_pixel;
    frame[offset..offset + bytes_per_pixel].copy_from_slice(pixel);
}

fn tga_advance_pixel(header: &TgaHeader, storage_x: &mut usize, storage_y: &mut usize) {
    *storage_x += 1;
    if *storage_x >= header.width {
        *storage_x = 0;
        *storage_y += 1;
    }
}

fn tga_palette(bytes: &[u8], header: &TgaHeader) -> Result<[u8; BMP_PALETTE_BYTES]> {
    let mut palette = [0_u8; BMP_PALETTE_BYTES];
    let bytes_per_entry = tga_bytes_per_pixel(header.color_map_depth);
    for entry in 0..header.color_map_len {
        let src = header
            .color_map_offset
            .checked_add(entry * bytes_per_entry)
            .ok_or_else(|| RmpegError::InvalidData("TGA palette offset overflow".to_string()))?;
        if src + bytes_per_entry > header.image_end {
            return Err(RmpegError::UnexpectedEof {
                needed: src + bytes_per_entry,
                remaining: header.image_end,
            });
        }
        let dst_entry = header
            .color_map_first
            .checked_add(entry)
            .ok_or_else(|| RmpegError::InvalidData("TGA palette index overflow".to_string()))?;
        if dst_entry >= 256 {
            continue;
        }
        let dst = dst_entry * 4;
        match header.color_map_depth {
            15 | 16 => {
                let value = u16::from_le_bytes([bytes[src], bytes[src + 1]]);
                let b = (value & 0x1f) as u8;
                let g = ((value >> 5) & 0x1f) as u8;
                let r = ((value >> 10) & 0x1f) as u8;
                palette[dst] = expand_5_to_8(b);
                palette[dst + 1] = expand_5_to_8(g);
                palette[dst + 2] = expand_5_to_8(r);
                palette[dst + 3] = 0xff;
            }
            24 => {
                palette[dst..dst + 3].copy_from_slice(&bytes[src..src + 3]);
                palette[dst + 3] = 0xff;
            }
            32 => {
                palette[dst..dst + 4].copy_from_slice(&bytes[src..src + 4]);
            }
            _ => {
                return Err(RmpegError::Unsupported(format!(
                    "unsupported TGA color map depth {}",
                    header.color_map_depth
                )));
            }
        }
    }
    Ok(palette)
}

fn expand_5_to_8(value: u8) -> u8 {
    (value << 3) | (value >> 2)
}

fn tga_bytes_per_pixel(bits: u8) -> usize {
    usize::from(bits).div_ceil(8)
}

struct SunrastHeader {
    width: usize,
    height: usize,
    depth: u32,
    raster_type: u32,
    map_type: u32,
    map_len: usize,
    data_offset: usize,
}

fn sunrast_header(bytes: &[u8]) -> Result<SunrastHeader> {
    if bytes.len() < SUNRAST_HEADER_LEN {
        return Err(RmpegError::UnexpectedEof {
            needed: SUNRAST_HEADER_LEN,
            remaining: bytes.len(),
        });
    }
    if &bytes[..4] != SUNRAST_MAGIC {
        return Err(RmpegError::InvalidData(
            "missing Sun Raster signature".to_string(),
        ));
    }

    let width = usize::try_from(read_u32_be(bytes, 4)?)
        .map_err(|_| RmpegError::Unsupported("Sun Raster width is too large".to_string()))?;
    let height = usize::try_from(read_u32_be(bytes, 8)?)
        .map_err(|_| RmpegError::Unsupported("Sun Raster height is too large".to_string()))?;
    let depth = read_u32_be(bytes, 12)?;
    let raster_type = read_u32_be(bytes, 20)?;
    let map_type = read_u32_be(bytes, 24)?;
    let map_len = usize::try_from(read_u32_be(bytes, 28)?)
        .map_err(|_| RmpegError::Unsupported("Sun Raster map is too large".to_string()))?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "Sun Raster dimensions must be nonzero".to_string(),
        ));
    }
    if !matches!(depth, 1 | 8 | 24) {
        return Err(RmpegError::Unsupported(format!(
            "unsupported Sun Raster depth {depth}"
        )));
    }
    if !matches!(
        raster_type,
        SUNRAST_TYPE_OLD | SUNRAST_TYPE_STANDARD | SUNRAST_TYPE_BYTE_ENCODED
    ) {
        return Err(RmpegError::Unsupported(format!(
            "unsupported Sun Raster type {raster_type}"
        )));
    }
    if !matches!(map_type, SUNRAST_MAP_NONE | SUNRAST_MAP_RGB) {
        return Err(RmpegError::Unsupported(format!(
            "unsupported Sun Raster map type {map_type}"
        )));
    }
    let data_offset = SUNRAST_HEADER_LEN
        .checked_add(map_len)
        .ok_or_else(|| RmpegError::InvalidData("Sun Raster data offset overflow".to_string()))?;
    if data_offset > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_offset,
            remaining: bytes.len(),
        });
    }

    Ok(SunrastHeader {
        width,
        height,
        depth,
        raster_type,
        map_type,
        map_len,
        data_offset,
    })
}

fn sunrast_frame(bytes: &[u8], header: &SunrastHeader) -> Result<Vec<u8>> {
    let row_bytes = sunrast_row_bytes(header)?;
    let image_bytes = row_bytes
        .checked_mul(header.height)
        .ok_or_else(|| RmpegError::InvalidData("Sun Raster frame size overflow".to_string()))?;
    let mut frame = if header.raster_type == SUNRAST_TYPE_BYTE_ENCODED {
        sunrast_decode_rle(&bytes[header.data_offset..], image_bytes)?
    } else {
        let needed = header
            .data_offset
            .checked_add(image_bytes)
            .ok_or_else(|| RmpegError::InvalidData("Sun Raster data range overflow".to_string()))?;
        if needed > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed,
                remaining: bytes.len(),
            });
        }
        bytes[header.data_offset..needed].to_vec()
    };

    if header.depth == 8 && header.map_type == SUNRAST_MAP_RGB && header.map_len > 0 {
        frame.extend_from_slice(&sunrast_palette(bytes, header)?);
    }
    Ok(frame)
}

fn sunrast_row_bytes(header: &SunrastHeader) -> Result<usize> {
    header
        .width
        .checked_mul(
            usize::try_from(header.depth).map_err(|_| {
                RmpegError::Unsupported("Sun Raster depth is too large".to_string())
            })?,
        )
        .map(|bits| bits.div_ceil(16) * 2)
        .ok_or_else(|| RmpegError::InvalidData("Sun Raster row size overflow".to_string()))
}

fn sunrast_decode_rle(input: &[u8], expected_len: usize) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(expected_len);
    let mut pos = 0_usize;
    while output.len() < expected_len {
        if pos >= input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 1,
                remaining: input.len(),
            });
        }
        let value = input[pos];
        pos += 1;
        if value != 0x80 {
            output.push(value);
            continue;
        }
        if pos >= input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 1,
                remaining: input.len(),
            });
        }
        let count = input[pos];
        pos += 1;
        if count == 0 {
            output.push(0x80);
            continue;
        }
        if pos >= input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 1,
                remaining: input.len(),
            });
        }
        let repeated = input[pos];
        pos += 1;
        for _ in 0..=count {
            output.push(repeated);
            if output.len() > expected_len {
                return Err(RmpegError::InvalidData(
                    "Sun Raster RLE expands past frame size".to_string(),
                ));
            }
        }
    }
    Ok(output)
}

fn sunrast_palette(bytes: &[u8], header: &SunrastHeader) -> Result<[u8; BMP_PALETTE_BYTES]> {
    if !header.map_len.is_multiple_of(3) {
        return Err(RmpegError::InvalidData(
            "Sun Raster RGB map length is not divisible by three".to_string(),
        ));
    }
    let entries = (header.map_len / 3).min(256);
    let green = SUNRAST_HEADER_LEN + header.map_len / 3;
    let blue = SUNRAST_HEADER_LEN + 2 * (header.map_len / 3);
    let mut palette = [0_u8; BMP_PALETTE_BYTES];
    for entry in 0..entries {
        let dst = entry * 4;
        palette[dst] = bytes[blue + entry];
        palette[dst + 1] = bytes[green + entry];
        palette[dst + 2] = bytes[SUNRAST_HEADER_LEN + entry];
        palette[dst + 3] = 0xff;
    }
    Ok(palette)
}

fn read_u16_le(bytes: &[u8], pos: usize) -> Result<u16> {
    let end = pos + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]))
}

fn read_u32_be(bytes: &[u8], pos: usize) -> Result<u32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
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

fn read_i32_le(bytes: &[u8], pos: usize) -> Result<i32> {
    let end = pos + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(i32::from_le_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}

fn xbm_define(text: &str, suffix: &str) -> Result<u32> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() != Some("#define") {
            continue;
        }
        let Some(name) = parts.next() else {
            continue;
        };
        let Some(value) = parts.next() else {
            continue;
        };
        if name.ends_with(suffix) {
            let value = value
                .parse::<u32>()
                .map_err(|_| RmpegError::InvalidData(format!("XBM {suffix} is invalid")))?;
            if value == 0 {
                return Err(RmpegError::InvalidData(format!(
                    "XBM {suffix} must be nonzero"
                )));
            }
            return Ok(value);
        }
    }
    Err(RmpegError::InvalidData(format!(
        "XBM {suffix} was not found"
    )))
}

fn xbm_data_block(text: &str) -> Result<&str> {
    let bits = text
        .find("_bits")
        .ok_or_else(|| RmpegError::InvalidData("XBM bitmap array was not found".to_string()))?;
    let open = text[bits..]
        .find('{')
        .map(|offset| bits + offset + 1)
        .ok_or_else(|| RmpegError::InvalidData("XBM bitmap array was not opened".to_string()))?;
    let close = text[open..]
        .find('}')
        .map(|offset| open + offset)
        .ok_or_else(|| RmpegError::InvalidData("XBM bitmap array was not closed".to_string()))?;
    Ok(&text[open..close])
}

fn xbm_uses_short_storage(text: &str) -> bool {
    text.find("_bits")
        .is_some_and(|bits| text[..bits].contains("short"))
}

fn parse_c_integer_tokens(data: &str) -> Result<Vec<u32>> {
    let bytes = data.as_bytes();
    let mut values = Vec::new();
    let mut pos = 0_usize;
    while pos < bytes.len() {
        if !bytes[pos].is_ascii_digit() {
            pos += 1;
            continue;
        }
        let start = pos;
        if bytes.get(pos) == Some(&b'0') && matches!(bytes.get(pos + 1), Some(b'x' | b'X')) {
            pos += 2;
            let hex_start = pos;
            while bytes.get(pos).is_some_and(u8::is_ascii_hexdigit) {
                pos += 1;
            }
            if pos == hex_start {
                return Err(RmpegError::InvalidData(
                    "XBM hex literal has no digits".to_string(),
                ));
            }
            let token = std::str::from_utf8(&bytes[hex_start..pos])
                .map_err(|_| RmpegError::InvalidData("XBM token is invalid".to_string()))?;
            values.push(
                u32::from_str_radix(token, 16).map_err(|_| {
                    RmpegError::InvalidData("XBM hex literal is invalid".to_string())
                })?,
            );
        } else {
            while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
                pos += 1;
            }
            let token = std::str::from_utf8(&bytes[start..pos])
                .map_err(|_| RmpegError::InvalidData("XBM token is invalid".to_string()))?;
            values.push(
                token
                    .parse::<u32>()
                    .map_err(|_| RmpegError::InvalidData("XBM integer is invalid".to_string()))?,
            );
        }
    }
    if values.is_empty() {
        return Err(RmpegError::InvalidData(
            "XBM bitmap array is empty".to_string(),
        ));
    }
    Ok(values)
}

fn xbm_byte_frame(width: u32, height: u32, values: &[u32]) -> Result<Vec<u8>> {
    let row_bytes = xbm_row_bytes(width)?;
    let frame_size = checked_frame_size(row_bytes, height)?;
    if values.len() < frame_size {
        return Err(RmpegError::UnexpectedEof {
            needed: frame_size,
            remaining: values.len(),
        });
    }
    let mut frame = Vec::with_capacity(frame_size);
    for &value in values.iter().take(frame_size) {
        let byte = u8::try_from(value & 0xff)
            .map_err(|_| RmpegError::InvalidData("XBM byte overflow".to_string()))?;
        frame.push(byte.reverse_bits());
    }
    Ok(frame)
}

fn xbm_short_frame(width: u32, height: u32, values: &[u32]) -> Result<Vec<u8>> {
    let row_bytes = xbm_row_bytes(width)?;
    let words_per_row = usize::try_from(width.div_ceil(16))
        .map_err(|_| RmpegError::Unsupported("XBM width is too large".to_string()))?;
    let height = usize::try_from(height)
        .map_err(|_| RmpegError::Unsupported("XBM height is too large".to_string()))?;
    let needed_words = words_per_row
        .checked_mul(height)
        .ok_or_else(|| RmpegError::InvalidData("XBM word count overflow".to_string()))?;
    if values.len() < needed_words {
        return Err(RmpegError::UnexpectedEof {
            needed: needed_words,
            remaining: values.len(),
        });
    }
    let frame_size = row_bytes
        .checked_mul(height)
        .ok_or_else(|| RmpegError::InvalidData("XBM frame size overflow".to_string()))?;
    let mut frame = Vec::with_capacity(frame_size);
    for row in 0..height {
        let mut row_data = Vec::with_capacity(words_per_row * 2);
        for word in 0..words_per_row {
            let value = u16::try_from(values[row * words_per_row + word] & 0xffff)
                .map_err(|_| RmpegError::InvalidData("XBM short overflow".to_string()))?
                .reverse_bits();
            row_data.extend_from_slice(&value.to_le_bytes());
        }
        frame.extend_from_slice(&row_data[..row_bytes]);
    }
    Ok(frame)
}

fn xbm_row_bytes(width: u32) -> Result<usize> {
    usize::try_from(width.div_ceil(8))
        .map_err(|_| RmpegError::Unsupported("XBM width is too large".to_string()))
}

fn checked_frame_size(row_bytes: usize, height: u32) -> Result<usize> {
    row_bytes
        .checked_mul(
            usize::try_from(height)
                .map_err(|_| RmpegError::Unsupported("XBM height is too large".to_string()))?,
        )
        .ok_or_else(|| RmpegError::InvalidData("XBM frame size overflow".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_x11_byte_xbm_like_ffmpeg() {
        let input = b"#define image_width 8\n#define image_height 1\nstatic unsigned char image_bits[] = { 0x22 };\n";
        let frames = xbm_image_frame_hashes(input).expect("xbm frame hash");

        assert_eq!(frames[0].size, 1);
        assert_eq!(frames[0].hash, md5_hex(&[0x44]));
    }

    #[test]
    fn hashes_x10_short_xbm_like_ffmpeg() {
        let input = b"#define xlogo16_width 16\n#define xlogo16_height 1\nstatic unsigned short xlogo16_bits[] = { 0x0f80 };\n";
        let frames = xbm_image_frame_hashes(input).expect("xbm frame hash");

        assert_eq!(frames[0].size, 2);
        assert_eq!(frames[0].hash, md5_hex(&[0xf0, 0x01]));
    }

    #[test]
    fn hashes_24_bit_alias_pix_rle_like_ffmpeg() {
        let input = [
            0x00, 0x03, 0x00, 0x01, 0, 0, 0, 0, 0x00, 0x18, 0x02, 0x10, 0x20, 0x30, 0x01, 0x40,
            0x50, 0x60,
        ];
        let frames = alias_pix_image_frame_hashes(&input).expect("alias pix frame hash");

        assert_eq!(frames[0].size, 9);
        assert_eq!(
            frames[0].hash,
            md5_hex(&[0x10, 0x20, 0x30, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60])
        );
    }

    #[test]
    fn hashes_8_bit_alias_pix_rle_like_ffmpeg() {
        let input = [
            0x00, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0x00, 0x08, 0x01, 0xaa, 0x03, 0x55,
        ];
        let frames = alias_pix_image_frame_hashes(&input).expect("alias pix frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0xaa, 0x55, 0x55, 0x55]));
    }

    #[test]
    fn hashes_bottom_up_bgr24_bmp_like_ffmpeg() {
        let mut input = bmp_header(2, 2, 24, BI_RGB, 54);
        input.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0, 0]);
        input.extend_from_slice(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0, 0]);

        let frames = bmp_image_frame_hashes(&input).expect("bmp frame hash");

        assert_eq!(frames[0].size, 12);
        assert_eq!(
            frames[0].hash,
            md5_hex(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,])
        );
    }

    #[test]
    fn hashes_indexed_bmp_with_ffmpeg_palette_layout() {
        let mut input = bmp_header(2, 1, 8, BI_RGB, 62);
        input.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        input.extend_from_slice(&[0x11, 0x22, 0x33, 0x00]);
        input.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]);

        let frames = bmp_image_frame_hashes(&input).expect("bmp frame hash");
        let mut expected = vec![0x00, 0x01];
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0xff]);
        expected.extend_from_slice(&[0x11, 0x22, 0x33, 0xff]);
        expected.resize(2 + BMP_PALETTE_BYTES, 0);

        assert_eq!(frames[0].size, 2 + BMP_PALETTE_BYTES);
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_bottom_up_bgr24_tga_like_ffmpeg() {
        let mut input = tga_header(2, 2, 2, 24, 0);
        input.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        input.extend_from_slice(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60]);
        append_tga_footer(&mut input);

        let frames = tga_image_frame_hashes(&input).expect("tga frame hash");

        assert_eq!(frames[0].size, 12);
        assert_eq!(
            frames[0].hash,
            md5_hex(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,])
        );
    }

    #[test]
    fn hashes_rle_tga_like_ffmpeg() {
        let mut input = tga_header(3, 1, 11, 8, 0x20);
        input.extend_from_slice(&[0x81, 0x7f, 0x00, 0x20]);
        append_tga_footer(&mut input);

        let frames = tga_image_frame_hashes(&input).expect("tga frame hash");

        assert_eq!(frames[0].size, 3);
        assert_eq!(frames[0].hash, md5_hex(&[0x7f, 0x7f, 0x20]));
    }

    #[test]
    fn hashes_paletted_sunrast_like_ffmpeg() {
        let mut input = sunrast_header(2, 1, 8, SUNRAST_TYPE_STANDARD, SUNRAST_MAP_RGB, 6);
        input.extend_from_slice(&[0x30, 0x70, 0x20, 0x60, 0x10, 0x50]);
        input.extend_from_slice(&[0x00, 0x01]);

        let frames = sunrast_image_frame_hashes(&input).expect("sunrast frame hash");
        let mut expected = vec![0x00, 0x01];
        expected.extend_from_slice(&[0x10, 0x20, 0x30, 0xff]);
        expected.extend_from_slice(&[0x50, 0x60, 0x70, 0xff]);
        expected.resize(2 + BMP_PALETTE_BYTES, 0);

        assert_eq!(frames[0].size, 2 + BMP_PALETTE_BYTES);
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_rle_sunrast_like_ffmpeg() {
        let mut input = sunrast_header(4, 1, 8, SUNRAST_TYPE_BYTE_ENCODED, SUNRAST_MAP_NONE, 0);
        input.extend_from_slice(&[0x80, 0x02, 0x55, 0x80, 0x00]);

        let frames = sunrast_image_frame_hashes(&input).expect("sunrast frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x55, 0x55, 0x55, 0x80]));
    }

    #[test]
    fn hashes_scaled_int16_fits_bottom_up_like_ffmpeg() {
        let mut input = fits_header(&[
            ("BITPIX", "16"),
            ("NAXIS", "2"),
            ("NAXIS1", "2"),
            ("NAXIS2", "2"),
            ("BSCALE", "2.0"),
            ("BZERO", "10.0"),
            ("DATAMIN", "10.0"),
            ("DATAMAX", "70.0"),
        ]);
        for value in [0_i16, 10, 20, 30] {
            input.extend_from_slice(&value.to_be_bytes());
        }

        let frames = fits_image_frame_hashes(&input).expect("fits frame hash");
        let expected = gray16le(&[43690, 65535, 0, 21845]);

        assert_eq!(frames[0].size, 8);
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_float_fits_with_rounded_gray16_like_ffmpeg() {
        let mut input = fits_header(&[
            ("BITPIX", "-32"),
            ("NAXIS", "2"),
            ("NAXIS1", "3"),
            ("NAXIS2", "1"),
        ]);
        for value in [-1.0_f32, 0.0, 1.0] {
            input.extend_from_slice(&value.to_be_bytes());
        }

        let frames = fits_image_frame_hashes(&input).expect("fits frame hash");
        let expected = gray16le(&[0, 32768, 65535]);

        assert_eq!(frames[0].size, 6);
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_8_bit_dpx_without_row_padding_like_ffmpeg() {
        let mut input = dpx_header(false, 2, 1, 8, 0);
        input.extend_from_slice(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0xaa, 0xbb]);

        let frames = dpx_image_frame_hashes(&input).expect("dpx frame hash");

        assert_eq!(frames[0].size, 6);
        assert_eq!(
            frames[0].hash,
            md5_hex(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60])
        );
    }

    #[test]
    fn hashes_10_bit_dpx_as_planar_gbrp10le_like_ffmpeg() {
        let mut input = dpx_header(false, 1, 1, 10, 1);
        let word = (1_u32 << 22) | (2_u32 << 12) | (3_u32 << 2);
        input.extend_from_slice(&word.to_le_bytes());

        let frames = dpx_image_frame_hashes(&input).expect("dpx frame hash");

        assert_eq!(frames[0].size, 6);
        assert_eq!(
            frames[0].hash,
            md5_hex(&[0x02, 0x00, 0x03, 0x00, 0x01, 0x00])
        );
    }

    #[test]
    fn hashes_concatenated_dpx_frames_like_ffmpeg() {
        let mut input = dpx_header(false, 1, 1, 8, 0);
        input.extend_from_slice(&[0x10, 0x20, 0x30, 0x00]);
        input.extend_from_slice(&dpx_header(false, 1, 1, 8, 0));
        input.extend_from_slice(&[0x40, 0x50, 0x60, 0x00]);

        let frames = dpx_image_frame_hashes(&input).expect("dpx frame hash");

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].hash, md5_hex(&[0x10, 0x20, 0x30]));
        assert_eq!(frames[1].dts, 1);
        assert_eq!(frames[1].hash, md5_hex(&[0x40, 0x50, 0x60]));
    }

    #[test]
    fn hashes_binary_pbm_like_ffmpeg() {
        let input = b"P4\n8 1\n\x80";

        let frames = pnm_image_frame_hashes(input).expect("pbm frame hash");

        assert_eq!(frames[0].size, 1);
        assert_eq!(frames[0].hash, md5_hex(&[0x80]));
    }

    #[test]
    fn hashes_binary_pgm_like_ffmpeg() {
        let input = b"P5\n2 1\n255\n\x00\xff";

        let frames = pnm_image_frame_hashes(input).expect("pgm frame hash");

        assert_eq!(frames[0].size, 2);
        assert_eq!(frames[0].hash, md5_hex(&[0x00, 0xff]));
    }

    #[test]
    fn hashes_16_bit_pgm_as_little_endian_like_ffmpeg() {
        let input = b"P5\n2 1\n65535\n\x00\x01\xff\xff";

        let frames = pnm_image_frame_hashes(input).expect("pgm frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x01, 0x00, 0xff, 0xff]));
    }

    #[test]
    fn hashes_binary_ppm_like_ffmpeg() {
        let input = b"P6\n2 1\n255\n\x01\x02\x03\x04\x05\x06";

        let frames = pnm_image_frame_hashes(input).expect("ppm frame hash");

        assert_eq!(frames[0].size, 6);
        assert_eq!(
            frames[0].hash,
            md5_hex(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06])
        );
    }

    #[test]
    fn hashes_ptx_payload_like_ffmpeg() {
        let mut input = ptx_header(2, 1);
        input.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]);

        let frames = ptx_image_frame_hashes(&input).expect("ptx frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x11, 0x22, 0x33, 0x44]));
    }

    #[test]
    fn hashes_brender_pix_ya8_payload_like_ffmpeg() {
        let mut input = brender_pix_fixture([0x12, 0x00, 0x20, 0x00], 2, 1);
        input.extend_from_slice(&[0x60, 0x04, 0xf6, 0x30]);

        let frames = brender_pix_image_frame_hashes(&input).expect("brender pix frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x60, 0x04, 0xf6, 0x30]));
    }

    #[test]
    fn hashes_brender_pix_rgb565_payload_like_ffmpeg() {
        let mut input = brender_pix_fixture([0x05, 0x01, 0x00, 0x00], 1, 2);
        input.extend_from_slice(&[0x94, 0x91, 0x21, 0x24]);

        let frames = brender_pix_image_frame_hashes(&input).expect("brender pix frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x94, 0x91, 0x21, 0x24]));
    }

    #[test]
    fn hashes_brender_pix_default_pal8_payload_like_ffmpeg() {
        let mut input = brender_pix_paletted_fixture(2, 1);
        input.extend_from_slice(BRENDER_PIX_INDEX_MARKER);
        input.extend_from_slice(&[0x00, 0x41]);
        let mut expected = vec![0x00, 0x41];
        expected.extend_from_slice(&brender_pix_default_palette());

        let frames = brender_pix_image_frame_hashes(&input).expect("brender pix frame hash");

        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_brender_pix_embedded_palette_like_ffmpeg() {
        let mut input = brender_pix_paletted_fixture(1, 1);
        input.extend_from_slice(BRENDER_PIX_PALETTE_MARKER);
        let mut source_palette = [0_u8; BMP_PALETTE_BYTES];
        source_palette[4..8].copy_from_slice(&[0x20, 0x10, 0x08, 0x00]);
        input.extend_from_slice(&source_palette);
        input.extend_from_slice(BRENDER_PIX_INDEX_MARKER);
        input.push(0x01);
        let mut expected = vec![0x01];
        let mut expected_palette = [0_u8; BMP_PALETTE_BYTES];
        for entry in 0..256 {
            expected_palette[entry * 4 + 3] = 0xff;
        }
        expected_palette[4..8].copy_from_slice(&[0x08, 0x10, 0x20, 0xff]);
        expected.extend_from_slice(&expected_palette);

        let frames = brender_pix_image_frame_hashes(&input).expect("brender pix frame hash");

        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_alpha8_payload_like_ffmpeg() {
        let mut input = dds_fixture(2, 2, DDPF_ALPHA, *b"\0\0\0\0", 8, [0, 0, 0, 0xff], 4);
        input.extend_from_slice(&[0x10, 0x20, 0x30, 0x40]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x10, 0x20, 0x30, 0x40]));
    }

    #[test]
    fn hashes_dds_bgra_base_level_and_ignores_mips_like_ffmpeg() {
        let mut input = dds_fixture(
            1,
            1,
            DDPF_RGB | DDPF_ALPHAPIXELS,
            *b"\0\0\0\0",
            32,
            [0xff0000, 0x00ff00, 0x0000ff, 0xff000000],
            4,
        );
        input.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]);
        input.extend_from_slice(&[0xff; 16]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x01, 0x02, 0x03, 0x04]));
    }

    #[test]
    fn hashes_dds_monob_payload_like_ffmpeg() {
        let mut input = dds_fixture(9, 1, DDPF_FOURCC, *b"G1  ", 0, [0, 0, 0, 0], 2);
        input.extend_from_slice(&[0x80, 0x40]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        assert_eq!(frames[0].size, 2);
        assert_eq!(frames[0].hash, md5_hex(&[0x80, 0x40]));
    }

    #[test]
    fn hashes_dds_indexed_palette_like_ffmpeg() {
        let mut input = dds_fixture(2, 1, DDPF_PALETTEINDEXED8, *b"\0\0\0\0", 8, [0, 0, 0, 0], 2);
        let mut palette = vec![0; DDS_PALETTE_BYTES];
        palette[0..4].copy_from_slice(&[0x10, 0x20, 0x30, 0xff]);
        palette[4..8].copy_from_slice(&[0x40, 0x50, 0x60, 0x7f]);
        input.extend_from_slice(&palette);
        input.extend_from_slice(&[1, 0]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = vec![1, 0];
        expected.extend_from_slice(&[0x30, 0x20, 0x10, 0xff]);
        expected.extend_from_slice(&[0x60, 0x50, 0x40, 0x7f]);
        expected.resize(2 + DDS_PALETTE_BYTES, 0);
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_fourcc_p8_palette_like_ffmpeg() {
        let mut input = dds_fixture(1, 1, DDPF_FOURCC, *b"P8  ", 0, [0, 0, 0, 0], 1);
        let mut palette = vec![0; DDS_PALETTE_BYTES];
        palette[12..16].copy_from_slice(&[0x01, 0x02, 0x03, 0xff]);
        input.extend_from_slice(&palette);
        input.extend_from_slice(&[3]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = vec![3];
        expected.extend_from_slice(&[0; 12]);
        expected.extend_from_slice(&[0x03, 0x02, 0x01, 0xff]);
        expected.resize(1 + DDS_PALETTE_BYTES, 0);
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_aexp_like_ffmpeg() {
        let mut input = dds_fixture(
            1,
            1,
            DDPF_RGB | DDPF_ALPHAPIXELS,
            *b"\0\0\0\0",
            32,
            [0xff0000, 0x00ff00, 0x0000ff, 0xff000000],
            4,
        );
        input[44..48].copy_from_slice(b"AEXP");
        input.extend_from_slice(&[0x00, 0xff, 0x41, 0x79]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x00, 0x79, 0x1e, 0xff]));
    }

    #[test]
    fn hashes_dds_ycocg_like_ffmpeg() {
        let mut input = dds_fixture(
            1,
            1,
            DDPF_RGB | DDPF_ALPHAPIXELS,
            *b"\0\0\0\0",
            32,
            [0xff0000, 0x00ff00, 0x0000ff, 0xff000000],
            4,
        );
        input[44..48].copy_from_slice(b"YCG1");
        input.extend_from_slice(&[0xff, 0xb5, 0x90, 0x44]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x1f, 0x79, 0x00, 0xff]));
    }

    #[test]
    fn hashes_dds_bc1_block_like_ffmpeg() {
        let mut input = dds_fixture(4, 4, DDPF_FOURCC, *b"DXT1", 0, [0, 0, 0, 0], 8);
        let mut indices = 0_u32;
        for index in 0..16 {
            indices |= u32::try_from(index % 4).expect("small index") << (index * 2);
        }
        input.extend_from_slice(&0xf800_u16.to_le_bytes());
        input.extend_from_slice(&0x07e0_u16.to_le_bytes());
        input.extend_from_slice(&indices.to_le_bytes());

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let colors = [
            [0xff, 0x00, 0x00, 0xff],
            [0x00, 0xff, 0x00, 0xff],
            [0xaa, 0x55, 0x00, 0xff],
            [0x55, 0xaa, 0x00, 0xff],
        ];
        let mut expected = Vec::new();
        for index in 0..16 {
            expected.extend_from_slice(&colors[index % 4]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_dxt2_unpremultiplied_like_ffmpeg() {
        let mut input = dds_fixture(4, 4, DDPF_FOURCC, *b"DXT2", 0, [0, 0, 0, 0], 16);
        let mut alpha_bits = 0_u64;
        for pixel in 1..16 {
            alpha_bits |= 7_u64 << (pixel * 4);
        }
        input.extend_from_slice(&alpha_bits.to_le_bytes());
        input.extend_from_slice(&0x0080_u16.to_le_bytes());
        input.extend_from_slice(&0x0080_u16.to_le_bytes());
        input.extend_from_slice(&0_u32.to_le_bytes());

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = Vec::new();
        expected.extend_from_slice(&[0x00, 0x10, 0x00, 0x00]);
        for _ in 1..16 {
            expected.extend_from_slice(&[0x00, 0x22, 0x00, 0x77]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_dxt4_unpremultiplied_like_ffmpeg() {
        let mut input = dds_fixture(4, 4, DDPF_FOURCC, *b"DXT4", 0, [0, 0, 0, 0], 16);
        input.push(0x77);
        input.push(0x00);
        input.extend_from_slice(&1_u64.to_le_bytes()[..6]);
        input.extend_from_slice(&0x0080_u16.to_le_bytes());
        input.extend_from_slice(&0x0080_u16.to_le_bytes());
        input.extend_from_slice(&0_u32.to_le_bytes());

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = Vec::new();
        expected.extend_from_slice(&[0x00, 0x10, 0x00, 0x00]);
        for _ in 1..16 {
            expected.extend_from_slice(&[0x00, 0x22, 0x00, 0x77]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_dxt1_normal_map_like_ffmpeg() {
        let mut input = dds_fixture(
            4,
            4,
            DDPF_FOURCC | 0x8000_0000,
            *b"DXT1",
            0,
            [0, 0, 0, 0],
            8,
        );
        input.extend_from_slice(&0x0080_u16.to_le_bytes());
        input.extend_from_slice(&0x0080_u16.to_le_bytes());
        input.extend_from_slice(&0_u32.to_le_bytes());

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = Vec::new();
        for _ in 0..16 {
            expected.extend_from_slice(&[0x00, 0x10, 0xb4, 0xff]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_dxt5_xgbr_swizzle_like_ffmpeg() {
        let mut input = dds_fixture(
            4,
            4,
            DDPF_FOURCC,
            *b"DXT5",
            u32::from_le_bytes(*b"xGBR"),
            [0, 0, 0, 0],
            16,
        );
        input.extend_from_slice(&[0x82, 0x00, 0, 0, 0, 0, 0, 0]);
        input.extend_from_slice(&0x001f_u16.to_le_bytes());
        input.extend_from_slice(&0x001f_u16.to_le_bytes());
        input.extend_from_slice(&0_u32.to_le_bytes());

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = Vec::new();
        for _ in 0..16 {
            expected.extend_from_slice(&[0xff, 0x00, 0x82, 0x00]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_dxt5_ycocg_scaled_like_ffmpeg() {
        let mut input = dds_fixture(4, 4, DDPF_FOURCC, *b"DXT5", 0, [0, 0, 0, 0], 16);
        input[44..48].copy_from_slice(b"YCG2");
        input.extend_from_slice(&[0xff, 0x00, 0, 0, 0, 0, 0, 0]);
        input.extend_from_slice(&0x8421_u16.to_le_bytes());
        input.extend_from_slice(&0x8421_u16.to_le_bytes());
        input.extend_from_slice(&0_u32.to_le_bytes());

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = Vec::new();
        for _ in 0..16 {
            expected.extend_from_slice(&[0xfe, 0xff, 0xfa, 0xff]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_bc4_unsigned_grayscale_like_ffmpeg() {
        let mut input = dds_fixture(4, 4, DDPF_FOURCC, *b"ATI1", 0, [0, 0, 0, 0], 8);
        let mut indices = 0_u64;
        for index in 0..16 {
            indices |= u64::try_from(index % 8).expect("small index") << (index * 3);
        }
        input.push(0xff);
        input.push(0x00);
        input.extend_from_slice(&indices.to_le_bytes()[..6]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let values = [255, 0, 218, 182, 145, 109, 72, 36];
        let mut expected = Vec::new();
        for index in 0..16 {
            let value = values[index % 8];
            expected.extend_from_slice(&[value, value, value, 0xff]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_bc4_signed_floor_interpolation_like_ffmpeg() {
        let mut input = dds_fixture(4, 4, DDPF_FOURCC, *b"BC4S", 0, [0, 0, 0, 0], 8);
        let mut indices = 0_u64;
        for pixel in 0..16 {
            indices |= 3_u64 << (pixel * 3);
        }
        input.push(0x80);
        input.push(0x81);
        input.extend_from_slice(&indices.to_le_bytes()[..6]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = Vec::new();
        for _ in 0..16 {
            expected.extend_from_slice(&[0x00, 0x00, 0x00, 0xff]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_bc5_plain_ati2_swapped_like_ffmpeg() {
        let mut input = dds_fixture(4, 4, DDPF_FOURCC, *b"ATI2", 0, [0, 0, 0, 0], 16);
        input.extend_from_slice(&[0xff, 0x00, 0, 0, 0, 0, 0, 0]);
        input.extend_from_slice(&[0x00, 0x00, 0, 0, 0, 0, 0, 0]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = Vec::new();
        for _ in 0..16 {
            expected.extend_from_slice(&[0x00, 0xff, 0x7f, 0xff]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_bc5_a2xy_order_like_ffmpeg() {
        let mut input = dds_fixture(
            4,
            4,
            DDPF_FOURCC,
            *b"ATI2",
            u32::from_le_bytes(*b"A2XY"),
            [0, 0, 0, 0],
            16,
        );
        input.extend_from_slice(&[0x00, 0x00, 0, 0, 0, 0, 0, 0]);
        input.extend_from_slice(&[0x82, 0x00, 0, 0, 0, 0, 0, 0]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = Vec::new();
        for _ in 0..16 {
            expected.extend_from_slice(&[0x00, 0x82, 0x9b, 0xff]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn hashes_dds_bc5_signed_order_like_ffmpeg() {
        let mut input = dds_fixture(4, 4, DDPF_FOURCC, *b"BC5S", 0, [0, 0, 0, 0], 16);
        let mut red_indices = 0_u64;
        for pixel in 0..16 {
            red_indices |= 3_u64 << (pixel * 3);
        }
        input.push(0x80);
        input.push(0x81);
        input.extend_from_slice(&red_indices.to_le_bytes()[..6]);
        input.extend_from_slice(&[0x7f, 0x00, 0, 0, 0, 0, 0, 0]);

        let frames = dds_image_frame_hashes(&input).expect("dds frame hash");

        let mut expected = Vec::new();
        for _ in 0..16 {
            expected.extend_from_slice(&[0x00, 0xff, 0x7f, 0xff]);
        }
        assert_eq!(frames[0].size, expected.len());
        assert_eq!(frames[0].hash, md5_hex(&expected));
    }

    #[test]
    fn reconstructs_dds_bc5_blue_like_ffmpeg() {
        assert_eq!(dds_bc5_blue(0, 130), 155);
        assert_eq!(dds_bc5_blue(18, 106), 163);
        assert_eq!(dds_bc5_blue(200, 100), 87);
        assert_eq!(dds_bc5_blue(255, 255), 127);
    }

    #[test]
    fn rejects_dds_special_bgra_row_pitch() {
        let mut input = dds_fixture(
            1,
            2,
            DDPF_RGB | DDPF_ALPHAPIXELS,
            *b"\0\0\0\0",
            32,
            [0xff0000, 0x00ff00, 0x0000ff, 0xff000000],
            4,
        );
        input.extend_from_slice(&[0; 8]);

        let err = dds_image_frame_hashes(&input).expect_err("special pitch is unsupported");

        assert!(err.to_string().contains("unsupported DDS pixel format"));
    }

    #[test]
    fn hashes_uncompressed_sgi_rgb_as_flipped_gbrp_like_ffmpeg() {
        let mut input = sgi_header(SGI_STORAGE_VERBATIM, 1, 2, 2, 3);
        input.extend_from_slice(&[0x10, 0x11, 0x12, 0x13]);
        input.extend_from_slice(&[0x20, 0x21, 0x22, 0x23]);
        input.extend_from_slice(&[0x30, 0x31, 0x32, 0x33]);

        let frames = sgi_image_frame_hashes(&input).expect("sgi frame hash");

        assert_eq!(frames[0].size, 12);
        assert_eq!(
            frames[0].hash,
            md5_hex(&[0x22, 0x23, 0x20, 0x21, 0x32, 0x33, 0x30, 0x31, 0x12, 0x13, 0x10, 0x11])
        );
    }

    #[test]
    fn hashes_8_bit_rle_sgi_like_ffmpeg() {
        let mut input = sgi_header(SGI_STORAGE_RLE, 1, 4, 1, 1);
        input.extend_from_slice(&520_u32.to_be_bytes());
        input.extend_from_slice(&6_u32.to_be_bytes());
        input.extend_from_slice(&[0x84, 0x01, 0x02, 0x03, 0x04, 0x00]);

        let frames = sgi_image_frame_hashes(&input).expect("sgi frame hash");

        assert_eq!(frames[0].size, 4);
        assert_eq!(frames[0].hash, md5_hex(&[0x01, 0x02, 0x03, 0x04]));
    }

    #[test]
    fn hashes_16_bit_rle_sgi_like_ffmpeg() {
        let mut input = sgi_header(SGI_STORAGE_RLE, 2, 3, 1, 1);
        input.extend_from_slice(&520_u32.to_be_bytes());
        input.extend_from_slice(&6_u32.to_be_bytes());
        input.extend_from_slice(&3_u16.to_be_bytes());
        input.extend_from_slice(&[0x12, 0x34]);
        input.extend_from_slice(&0_u16.to_be_bytes());

        let frames = sgi_image_frame_hashes(&input).expect("sgi frame hash");

        assert_eq!(frames[0].size, 6);
        assert_eq!(
            frames[0].hash,
            md5_hex(&[0x12, 0x34, 0x12, 0x34, 0x12, 0x34])
        );
    }

    fn bmp_header(
        width: i32,
        height: i32,
        bits_per_pixel: u16,
        compression: u32,
        offset: u32,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&40_u32.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&bits_per_pixel.to_le_bytes());
        bytes.extend_from_slice(&compression.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes
    }

    fn tga_header(
        width: u16,
        height: u16,
        image_type: u8,
        pixel_depth: u8,
        descriptor: u8,
    ) -> Vec<u8> {
        let mut bytes = vec![0; 18];
        bytes[2] = image_type;
        bytes[12..14].copy_from_slice(&width.to_le_bytes());
        bytes[14..16].copy_from_slice(&height.to_le_bytes());
        bytes[16] = pixel_depth;
        bytes[17] = descriptor;
        bytes
    }

    fn append_tga_footer(bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(TGA_FOOTER_SIGNATURE);
    }

    fn sunrast_header(
        width: u32,
        height: u32,
        depth: u32,
        raster_type: u32,
        map_type: u32,
        map_len: u32,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(SUNRAST_MAGIC);
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&depth.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&raster_type.to_be_bytes());
        bytes.extend_from_slice(&map_type.to_be_bytes());
        bytes.extend_from_slice(&map_len.to_be_bytes());
        bytes
    }

    fn fits_header(cards: &[(&str, &str)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&fits_card("SIMPLE", "T"));
        for (key, value) in cards {
            bytes.extend_from_slice(&fits_card(key, value));
        }
        let mut end = [b' '; FITS_CARD_LEN];
        end[..3].copy_from_slice(b"END");
        bytes.extend_from_slice(&end);
        bytes.resize(bytes.len().div_ceil(FITS_BLOCK_LEN) * FITS_BLOCK_LEN, b' ');
        bytes
    }

    fn dpx_header(
        big_endian: bool,
        width: u32,
        height: u32,
        bit_depth: u8,
        packing: u16,
    ) -> Vec<u8> {
        let mut bytes = vec![0_u8; DPX_HEADER_MIN_LEN];
        if big_endian {
            bytes[0..4].copy_from_slice(b"SDPX");
            bytes[4..8].copy_from_slice(&(DPX_HEADER_MIN_LEN as u32).to_be_bytes());
            let stored_row_bytes =
                dpx_stored_row_bytes(width as usize, bit_depth, packing).expect("row bytes");
            let file_size = DPX_HEADER_MIN_LEN + stored_row_bytes * height as usize;
            bytes[16..20].copy_from_slice(&(file_size as u32).to_be_bytes());
            bytes[770..772].copy_from_slice(&1_u16.to_be_bytes());
            bytes[772..776].copy_from_slice(&width.to_be_bytes());
            bytes[776..780].copy_from_slice(&height.to_be_bytes());
            bytes[804..806].copy_from_slice(&packing.to_be_bytes());
        } else {
            bytes[0..4].copy_from_slice(b"XPDS");
            bytes[4..8].copy_from_slice(&(DPX_HEADER_MIN_LEN as u32).to_le_bytes());
            let stored_row_bytes =
                dpx_stored_row_bytes(width as usize, bit_depth, packing).expect("row bytes");
            let file_size = DPX_HEADER_MIN_LEN + stored_row_bytes * height as usize;
            bytes[16..20].copy_from_slice(&(file_size as u32).to_le_bytes());
            bytes[770..772].copy_from_slice(&1_u16.to_le_bytes());
            bytes[772..776].copy_from_slice(&width.to_le_bytes());
            bytes[776..780].copy_from_slice(&height.to_le_bytes());
            bytes[804..806].copy_from_slice(&packing.to_le_bytes());
        }
        bytes[800] = DPX_DESCRIPTOR_RGB;
        bytes[803] = bit_depth;
        bytes
    }

    fn ptx_header(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0_u8; PTX_HEADER_LEN];
        bytes[0..4].copy_from_slice(&(PTX_HEADER_LEN as u32).to_le_bytes());
        bytes[8..10].copy_from_slice(&width.to_le_bytes());
        bytes[10..12].copy_from_slice(&height.to_le_bytes());
        bytes
    }

    fn brender_pix_fixture(pixel_format: [u8; 4], width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0_u8; BRENDER_PIX_DATA_OFFSET];
        bytes[0..4].copy_from_slice(&[0, 0, 0, 0x12]);
        bytes[4..8].copy_from_slice(&[0, 0, 0, 8]);
        bytes[8..12].copy_from_slice(&[0, 0, 0, 2]);
        bytes[12..16].copy_from_slice(&[0, 0, 0, 2]);
        bytes[24..28].copy_from_slice(&pixel_format);
        bytes[28..30].copy_from_slice(&width.to_le_bytes());
        bytes[30..32].copy_from_slice(&height.to_le_bytes());
        bytes
    }

    fn brender_pix_paletted_fixture(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = brender_pix_fixture([0x03, 0x01, 0x00, 0x01], width, 0);
        bytes[26..28].copy_from_slice(&height.to_le_bytes());
        bytes
    }

    fn dds_fixture(
        width: u32,
        height: u32,
        pixel_flags: u32,
        fourcc: [u8; 4],
        bits_per_pixel: u32,
        masks: [u32; 4],
        pitch_or_linear_size: u32,
    ) -> Vec<u8> {
        let mut bytes = vec![0_u8; DDS_HEADER_LEN];
        bytes[0..4].copy_from_slice(b"DDS ");
        bytes[4..8].copy_from_slice(&124_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&height.to_le_bytes());
        bytes[16..20].copy_from_slice(&width.to_le_bytes());
        bytes[20..24].copy_from_slice(&pitch_or_linear_size.to_le_bytes());
        bytes[76..80].copy_from_slice(&DDS_PIXEL_FORMAT_LEN.to_le_bytes());
        bytes[80..84].copy_from_slice(&pixel_flags.to_le_bytes());
        bytes[84..88].copy_from_slice(&fourcc);
        bytes[88..92].copy_from_slice(&bits_per_pixel.to_le_bytes());
        bytes[92..96].copy_from_slice(&masks[0].to_le_bytes());
        bytes[96..100].copy_from_slice(&masks[1].to_le_bytes());
        bytes[100..104].copy_from_slice(&masks[2].to_le_bytes());
        bytes[104..108].copy_from_slice(&masks[3].to_le_bytes());
        bytes
    }

    fn fits_card(key: &str, value: &str) -> [u8; FITS_CARD_LEN] {
        let mut card = [b' '; FITS_CARD_LEN];
        card[..key.len()].copy_from_slice(key.as_bytes());
        card[8] = b'=';
        card[10..10 + value.len()].copy_from_slice(value.as_bytes());
        card
    }

    fn gray16le(values: &[u16]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(values.len() * 2);
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn sgi_header(
        storage: u8,
        bytes_per_channel: u8,
        width: u16,
        height: u16,
        channels: u16,
    ) -> Vec<u8> {
        let mut bytes = vec![0_u8; SGI_HEADER_LEN];
        bytes[0..2].copy_from_slice(b"\x01\xda");
        bytes[2] = storage;
        bytes[3] = bytes_per_channel;
        let dimensions = if channels == 1 { 2_u16 } else { 3_u16 };
        bytes[4..6].copy_from_slice(&dimensions.to_be_bytes());
        bytes[6..8].copy_from_slice(&width.to_be_bytes());
        bytes[8..10].copy_from_slice(&height.to_be_bytes());
        bytes[10..12].copy_from_slice(&channels.to_be_bytes());
        bytes
    }
}
