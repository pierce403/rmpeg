use rmpeg_core::{AudioFrameHash, Result, RmpegError};

use crate::md5::md5_hex;
use crate::video::VideoFrameHashDocument;

const GIF_HEADER_LEN: usize = 13;
const GIF_MAX_LZW_CODE: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GifColor {
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GifGraphicControl {
    delay_centiseconds: u16,
    transparent_index: Option<u8>,
    disposal_method: u8,
}

#[derive(Debug)]
struct GifFrame {
    pixels: Vec<u8>,
    delay_centiseconds: u16,
}

#[derive(Debug)]
struct GifDecoded {
    width: usize,
    height: usize,
    frames: Vec<GifFrame>,
}

#[derive(Clone, Copy)]
struct GifScreen<'a> {
    width: usize,
    height: usize,
    global_palette: Option<&'a [GifColor]>,
    background_index: u8,
}

pub fn gif_video_frame_hashes(input: &[u8]) -> Result<VideoFrameHashDocument> {
    let decoded = decode_gif(input)?;
    if decoded.frames.is_empty() {
        return Err(RmpegError::InvalidData(
            "GIF did not contain image frames".to_string(),
        ));
    }

    let tick_centiseconds = gif_tick_centiseconds(&decoded.frames);
    let (frame_rate_num, frame_rate_den) = gif_frame_rate(tick_centiseconds);
    let mut next_pts = 0_u64;
    let mut frames = Vec::with_capacity(decoded.frames.len());
    for frame in decoded.frames {
        let duration = gif_frame_duration(frame.delay_centiseconds, tick_centiseconds);
        frames.push(AudioFrameHash {
            stream_index: 0,
            dts: next_pts,
            pts: next_pts,
            duration,
            size: frame.pixels.len(),
            hash: md5_hex(&frame.pixels),
        });
        next_pts = next_pts.saturating_add(u64::from(duration));
    }

    Ok(VideoFrameHashDocument {
        width: decoded.width as u32,
        height: decoded.height as u32,
        frame_rate_num,
        frame_rate_den,
        frames,
    })
}

fn decode_gif(bytes: &[u8]) -> Result<GifDecoded> {
    if !(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return Err(RmpegError::InvalidData("missing GIF signature".to_string()));
    }
    if bytes.len() < GIF_HEADER_LEN {
        return Err(RmpegError::UnexpectedEof {
            needed: GIF_HEADER_LEN,
            remaining: bytes.len(),
        });
    }

    let width = usize::from(read_u16_le(bytes, 6)?);
    let height = usize::from(read_u16_le(bytes, 8)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "GIF dimensions must be nonzero".to_string(),
        ));
    }
    let frame_len = bgra_frame_len(width, height, "GIF canvas")?;
    let mut pos = GIF_HEADER_LEN;
    let packed = bytes[10];
    let background_index = bytes[11];
    let global_palette = if packed & 0x80 != 0 {
        let palette_len = palette_len(packed)?;
        let palette = read_palette(bytes, pos, palette_len)?;
        pos = pos
            .checked_add(palette_len * 3)
            .ok_or_else(|| RmpegError::InvalidData("GIF palette range overflow".to_string()))?;
        Some(palette)
    } else {
        None
    };

    let mut canvas =
        gif_initial_canvas(width, height, global_palette.as_deref(), background_index)?;
    let mut frames = Vec::new();
    let mut control = GifGraphicControl::default();
    let screen = GifScreen {
        width,
        height,
        global_palette: global_palette.as_deref(),
        background_index,
    };

    while let Some(&marker) = bytes.get(pos) {
        pos += 1;
        match marker {
            0x21 => {
                let Some(&label) = bytes.get(pos) else {
                    return Err(RmpegError::UnexpectedEof {
                        needed: pos + 1,
                        remaining: bytes.len(),
                    });
                };
                pos += 1;
                if label == 0xF9 {
                    let (next_pos, next_control) = parse_graphic_control(bytes, pos)?;
                    pos = next_pos;
                    control = next_control;
                } else {
                    pos = skip_sub_blocks(bytes, pos)?;
                }
            }
            0x2C => {
                let image = parse_image_descriptor(bytes, pos)?;
                pos = image.next_pos;
                let palette = image
                    .local_palette
                    .as_deref()
                    .or(global_palette.as_deref())
                    .ok_or_else(|| {
                        RmpegError::InvalidData("GIF image has no palette".to_string())
                    })?;
                let restore = if control.disposal_method == 3 {
                    Some(canvas.clone())
                } else {
                    None
                };
                composite_gif_image(&mut canvas, width, height, &image, palette, control)?;
                frames.push(GifFrame {
                    pixels: canvas.clone(),
                    delay_centiseconds: control.delay_centiseconds,
                });
                dispose_gif_image(&mut canvas, screen, &image, palette, control, restore)?;
                control = GifGraphicControl::default();
            }
            0x3B => break,
            _ => {
                return Err(RmpegError::InvalidData(format!(
                    "unsupported GIF block marker 0x{marker:02x}"
                )));
            }
        }
    }

    if canvas.len() != frame_len {
        return Err(RmpegError::InvalidData(
            "GIF canvas size changed during decode".to_string(),
        ));
    }
    Ok(GifDecoded {
        width,
        height,
        frames,
    })
}

#[derive(Debug)]
struct GifImage {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    indexes: Vec<u8>,
    local_palette: Option<Vec<GifColor>>,
    next_pos: usize,
}

fn parse_graphic_control(bytes: &[u8], pos: usize) -> Result<(usize, GifGraphicControl)> {
    let Some(&block_size) = bytes.get(pos) else {
        return Err(RmpegError::UnexpectedEof {
            needed: pos + 1,
            remaining: bytes.len(),
        });
    };
    if block_size != 4 {
        return Err(RmpegError::InvalidData(format!(
            "unexpected GIF graphic control block size {block_size}"
        )));
    }
    let data_start = pos + 1;
    let data_end = data_start + 4;
    if data_end + 1 > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: data_end + 1,
            remaining: bytes.len(),
        });
    }
    if bytes[data_end] != 0 {
        return Err(RmpegError::InvalidData(
            "GIF graphic control extension is not terminated".to_string(),
        ));
    }

    let packed = bytes[data_start];
    let transparent_index = if packed & 0x01 != 0 {
        Some(bytes[data_start + 3])
    } else {
        None
    };
    Ok((
        data_end + 1,
        GifGraphicControl {
            delay_centiseconds: read_u16_le(bytes, data_start + 1)?,
            transparent_index,
            disposal_method: (packed >> 2) & 0x07,
        },
    ))
}

fn parse_image_descriptor(bytes: &[u8], pos: usize) -> Result<GifImage> {
    let descriptor_end = pos + 9;
    if descriptor_end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: descriptor_end,
            remaining: bytes.len(),
        });
    }
    let left = usize::from(read_u16_le(bytes, pos)?);
    let top = usize::from(read_u16_le(bytes, pos + 2)?);
    let width = usize::from(read_u16_le(bytes, pos + 4)?);
    let height = usize::from(read_u16_le(bytes, pos + 6)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "GIF image dimensions must be nonzero".to_string(),
        ));
    }
    let packed = bytes[pos + 8];
    let interlaced = packed & 0x40 != 0;
    let mut data_pos = descriptor_end;
    let local_palette = if packed & 0x80 != 0 {
        let len = palette_len(packed)?;
        let palette = read_palette(bytes, data_pos, len)?;
        data_pos = data_pos.checked_add(len * 3).ok_or_else(|| {
            RmpegError::InvalidData("GIF local palette range overflow".to_string())
        })?;
        Some(palette)
    } else {
        None
    };
    let Some(&minimum_code_size) = bytes.get(data_pos) else {
        return Err(RmpegError::UnexpectedEof {
            needed: data_pos + 1,
            remaining: bytes.len(),
        });
    };
    let (compressed, next_pos) = collect_sub_blocks(bytes, data_pos + 1)?;
    let pixel_count = width
        .checked_mul(height)
        .ok_or_else(|| RmpegError::InvalidData("GIF image pixel count overflow".to_string()))?;
    let indexes = gif_lzw_decode(&compressed, minimum_code_size, pixel_count)?;
    let indexes = if interlaced {
        deinterlace_gif_indexes(&indexes, width, height)?
    } else {
        indexes
    };

    Ok(GifImage {
        left,
        top,
        width,
        height,
        indexes,
        local_palette,
        next_pos,
    })
}

fn composite_gif_image(
    canvas: &mut [u8],
    canvas_width: usize,
    canvas_height: usize,
    image: &GifImage,
    palette: &[GifColor],
    control: GifGraphicControl,
) -> Result<()> {
    validate_gif_region(canvas_width, canvas_height, image)?;
    for row in 0..image.height {
        for column in 0..image.width {
            let index = image.indexes[row * image.width + column];
            if Some(index) == control.transparent_index {
                continue;
            }
            let color = palette.get(usize::from(index)).ok_or_else(|| {
                RmpegError::InvalidData(format!("GIF palette index {index} is out of range"))
            })?;
            let pixel = ((image.top + row) * canvas_width + image.left + column) * 4;
            write_bgra_pixel(&mut canvas[pixel..pixel + 4], *color, 255);
        }
    }
    Ok(())
}

fn dispose_gif_image(
    canvas: &mut [u8],
    screen: GifScreen<'_>,
    image: &GifImage,
    palette: &[GifColor],
    control: GifGraphicControl,
    restore: Option<Vec<u8>>,
) -> Result<()> {
    match control.disposal_method {
        2 => clear_gif_region(canvas, screen, image, palette, control),
        3 => {
            if let Some(previous) = restore {
                canvas.copy_from_slice(&previous);
                Ok(())
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

fn clear_gif_region(
    canvas: &mut [u8],
    screen: GifScreen<'_>,
    image: &GifImage,
    palette: &[GifColor],
    control: GifGraphicControl,
) -> Result<()> {
    validate_gif_region(screen.width, screen.height, image)?;
    let transparent_color = control
        .transparent_index
        .and_then(|index| palette.get(usize::from(index)).copied());
    let background = screen
        .global_palette
        .and_then(|palette| palette.get(usize::from(screen.background_index)));
    for row in 0..image.height {
        for column in 0..image.width {
            let pixel = ((image.top + row) * screen.width + image.left + column) * 4;
            if let Some(color) = transparent_color {
                write_bgra_pixel(&mut canvas[pixel..pixel + 4], color, 0);
            } else if let Some(&color) = background {
                write_bgra_pixel(&mut canvas[pixel..pixel + 4], color, 255);
            } else {
                write_transparent_white_pixel(&mut canvas[pixel..pixel + 4]);
            }
        }
    }
    Ok(())
}

fn validate_gif_region(canvas_width: usize, canvas_height: usize, image: &GifImage) -> Result<()> {
    let x_end = image
        .left
        .checked_add(image.width)
        .ok_or_else(|| RmpegError::InvalidData("GIF image x range overflow".to_string()))?;
    let y_end = image
        .top
        .checked_add(image.height)
        .ok_or_else(|| RmpegError::InvalidData("GIF image y range overflow".to_string()))?;
    if x_end > canvas_width || y_end > canvas_height {
        return Err(RmpegError::InvalidData(
            "GIF image lies outside canvas".to_string(),
        ));
    }
    Ok(())
}

fn gif_initial_canvas(
    width: usize,
    height: usize,
    global_palette: Option<&[GifColor]>,
    background_index: u8,
) -> Result<Vec<u8>> {
    let len = bgra_frame_len(width, height, "GIF canvas")?;
    let mut canvas = vec![0; len];
    if let Some(color) =
        global_palette.and_then(|palette| palette.get(usize::from(background_index)))
    {
        for pixel in canvas.chunks_exact_mut(4) {
            write_bgra_pixel(pixel, *color, 255);
        }
    } else {
        for pixel in canvas.chunks_exact_mut(4) {
            write_transparent_white_pixel(pixel);
        }
    }
    Ok(canvas)
}

fn write_bgra_pixel(pixel: &mut [u8], color: GifColor, alpha: u8) {
    pixel[0] = color.blue;
    pixel[1] = color.green;
    pixel[2] = color.red;
    pixel[3] = alpha;
}

fn write_transparent_white_pixel(pixel: &mut [u8]) {
    pixel[0] = 255;
    pixel[1] = 255;
    pixel[2] = 255;
    pixel[3] = 0;
}

fn deinterlace_gif_indexes(indexes: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let expected = width
        .checked_mul(height)
        .ok_or_else(|| RmpegError::InvalidData("GIF interlace pixel count overflow".to_string()))?;
    if indexes.len() != expected {
        return Err(RmpegError::InvalidData(
            "GIF interlaced image size mismatch".to_string(),
        ));
    }

    let mut output = vec![0; expected];
    let mut src_row = 0_usize;
    for (first_row, step) in [(0_usize, 8_usize), (4, 8), (2, 4), (1, 2)] {
        let mut row = first_row;
        while row < height {
            let src = src_row.checked_mul(width).ok_or_else(|| {
                RmpegError::InvalidData("GIF interlace source overflow".to_string())
            })?;
            let dst = row.checked_mul(width).ok_or_else(|| {
                RmpegError::InvalidData("GIF interlace target overflow".to_string())
            })?;
            output[dst..dst + width].copy_from_slice(&indexes[src..src + width]);
            src_row += 1;
            row += step;
        }
    }
    Ok(output)
}

fn gif_lzw_decode(data: &[u8], minimum_code_size: u8, expected_len: usize) -> Result<Vec<u8>> {
    if !(1..=8).contains(&minimum_code_size) {
        return Err(RmpegError::Unsupported(format!(
            "unsupported GIF LZW minimum code size {minimum_code_size}"
        )));
    }
    let clear_code = 1_usize << minimum_code_size;
    let end_code = clear_code + 1;
    let mut dictionary = gif_lzw_initial_dictionary(clear_code);
    let mut code_size = usize::from(minimum_code_size) + 1;
    let mut next_code = end_code + 1;
    let mut previous: Option<Vec<u8>> = None;
    let mut reader = LsbBitReader::new(data);
    let mut output = Vec::with_capacity(expected_len);

    while let Some(code) = reader.read_bits(code_size)? {
        if code == clear_code {
            dictionary = gif_lzw_initial_dictionary(clear_code);
            code_size = usize::from(minimum_code_size) + 1;
            next_code = end_code + 1;
            previous = None;
            continue;
        }
        if code == end_code {
            break;
        }

        let entry = if code < dictionary.len() {
            dictionary[code].clone().ok_or_else(|| {
                RmpegError::InvalidData(format!("invalid GIF LZW dictionary code {code}"))
            })?
        } else if code == next_code {
            let previous = previous.as_ref().ok_or_else(|| {
                RmpegError::InvalidData("GIF LZW missing previous code".to_string())
            })?;
            let mut entry = previous.clone();
            entry.push(previous[0]);
            entry
        } else {
            return Err(RmpegError::InvalidData(format!(
                "invalid GIF LZW code {code}"
            )));
        };

        if let Some(previous) = previous.as_ref() {
            if next_code < GIF_MAX_LZW_CODE {
                let mut next_entry = previous.clone();
                next_entry.push(entry[0]);
                if dictionary.len() <= next_code {
                    dictionary.resize(next_code + 1, None);
                }
                dictionary[next_code] = Some(next_entry);
                next_code += 1;
                if next_code == (1_usize << code_size) && code_size < 12 {
                    code_size += 1;
                }
            }
        }

        output.extend_from_slice(&entry);
        if output.len() > expected_len {
            return Err(RmpegError::InvalidData(
                "GIF LZW output exceeds image size".to_string(),
            ));
        }
        previous = Some(entry);
    }

    if output.len() != expected_len {
        return Err(RmpegError::InvalidData(
            "GIF LZW output ended before image was complete".to_string(),
        ));
    }
    Ok(output)
}

fn gif_lzw_initial_dictionary(clear_code: usize) -> Vec<Option<Vec<u8>>> {
    let mut dictionary = Vec::with_capacity(clear_code + 2);
    for value in 0..clear_code {
        dictionary.push(Some(vec![value as u8]));
    }
    dictionary.push(None);
    dictionary.push(None);
    dictionary
}

struct LsbBitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
}

impl<'a> LsbBitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit_pos: 0 }
    }

    fn read_bits(&mut self, count: usize) -> Result<Option<usize>> {
        if count == 0 || count > 12 {
            return Err(RmpegError::InvalidData(
                "GIF LZW code size is invalid".to_string(),
            ));
        }
        let total_bits =
            self.bytes.len().checked_mul(8).ok_or_else(|| {
                RmpegError::InvalidData("GIF bitstream size overflow".to_string())
            })?;
        if self.bit_pos + count > total_bits {
            return Ok(None);
        }
        let mut value = 0_usize;
        for bit in 0..count {
            let absolute_bit = self.bit_pos + bit;
            let byte = self.bytes[absolute_bit / 8];
            let mask = 1 << (absolute_bit % 8);
            if byte & mask != 0 {
                value |= 1 << bit;
            }
        }
        self.bit_pos += count;
        Ok(Some(value))
    }
}

fn gif_tick_centiseconds(frames: &[GifFrame]) -> u32 {
    frames
        .iter()
        .map(|frame| u32::from(frame.delay_centiseconds))
        .filter(|delay| *delay > 0)
        .reduce(gcd_u32)
        .unwrap_or(10)
}

fn gif_frame_rate(tick_centiseconds: u32) -> (u32, u32) {
    let gcd = gcd_u32(100, tick_centiseconds);
    (100 / gcd, tick_centiseconds / gcd)
}

fn gif_frame_duration(delay_centiseconds: u16, tick_centiseconds: u32) -> u32 {
    if tick_centiseconds == 0 {
        return 1;
    }
    let delay = u32::from(delay_centiseconds);
    if delay == 0 {
        1
    } else {
        delay.div_ceil(tick_centiseconds).max(1)
    }
}

fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}

fn collect_sub_blocks(bytes: &[u8], mut pos: usize) -> Result<(Vec<u8>, usize)> {
    let mut output = Vec::new();
    loop {
        let Some(&len) = bytes.get(pos) else {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 1,
                remaining: bytes.len(),
            });
        };
        pos += 1;
        if len == 0 {
            return Ok((output, pos));
        }
        let len = usize::from(len);
        let end = pos
            .checked_add(len)
            .ok_or_else(|| RmpegError::InvalidData("GIF sub-block range overflow".to_string()))?;
        if end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: end,
                remaining: bytes.len(),
            });
        }
        output.extend_from_slice(&bytes[pos..end]);
        pos = end;
    }
}

fn skip_sub_blocks(bytes: &[u8], pos: usize) -> Result<usize> {
    let (_, next_pos) = collect_sub_blocks(bytes, pos)?;
    Ok(next_pos)
}

fn read_palette(bytes: &[u8], pos: usize, len: usize) -> Result<Vec<GifColor>> {
    let byte_len = len
        .checked_mul(3)
        .ok_or_else(|| RmpegError::InvalidData("GIF palette size overflow".to_string()))?;
    let end = pos
        .checked_add(byte_len)
        .ok_or_else(|| RmpegError::InvalidData("GIF palette range overflow".to_string()))?;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    let mut palette = Vec::with_capacity(len);
    for rgb in bytes[pos..end].chunks_exact(3) {
        palette.push(GifColor {
            red: rgb[0],
            green: rgb[1],
            blue: rgb[2],
        });
    }
    Ok(palette)
}

fn palette_len(packed: u8) -> Result<usize> {
    1_usize
        .checked_shl(u32::from((packed & 0x07) + 1))
        .ok_or_else(|| RmpegError::InvalidData("GIF palette length overflow".to_string()))
}

fn bgra_frame_len(width: usize, height: usize, context: &str) -> Result<usize> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| RmpegError::InvalidData(format!("{context} size overflow")))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn gif_header(width: u16, height: u16, palette: &[[u8; 3]]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&[0x80, 0, 0]);
        for color in palette {
            bytes.extend_from_slice(color);
        }
        bytes
    }

    fn graphic_control(delay: u16, packed: u8, transparent_index: u8) -> Vec<u8> {
        let mut bytes = vec![0x21, 0xF9, 4, packed];
        bytes.extend_from_slice(&delay.to_le_bytes());
        bytes.extend_from_slice(&[transparent_index, 0]);
        bytes
    }

    fn one_pixel_image(index: u8) -> Vec<u8> {
        let payload = match index {
            0 => [0x44, 0x01],
            1 => [0x4c, 0x01],
            _ => panic!("test fixture only supports palette indexes 0 and 1"),
        };
        let mut bytes = vec![0x2C];
        bytes.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0, 0]);
        bytes.extend_from_slice(&[2, 2]);
        bytes.extend_from_slice(&payload);
        bytes.extend_from_slice(&[0]);
        bytes
    }

    fn one_pixel_local_palette_image(index: u8, palette: &[[u8; 3]]) -> Vec<u8> {
        let payload = match index {
            0 => [0x44, 0x01],
            1 => [0x4c, 0x01],
            _ => panic!("test fixture only supports palette indexes 0 and 1"),
        };
        let mut bytes = vec![0x2C];
        bytes.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0, 0x80]);
        for color in palette {
            bytes.extend_from_slice(color);
        }
        bytes.extend_from_slice(&[2, 2]);
        bytes.extend_from_slice(&payload);
        bytes.extend_from_slice(&[0]);
        bytes
    }

    #[test]
    fn decodes_single_index_lzw_payload() {
        assert_eq!(gif_lzw_decode(&[0x4c, 0x01], 2, 1).unwrap(), vec![1]);
    }

    #[test]
    fn hashes_single_frame_as_bgra() {
        let mut bytes = gif_header(1, 1, &[[255, 0, 0], [0, 0, 0]]);
        bytes.extend_from_slice(&graphic_control(5, 0, 0));
        bytes.extend_from_slice(&one_pixel_image(0));
        bytes.push(0x3B);

        let decoded = decode_gif(&bytes).unwrap();
        assert_eq!(decoded.frames.len(), 1);
        assert_eq!(decoded.frames[0].pixels, vec![0, 0, 255, 255]);

        let document = gif_video_frame_hashes(&bytes).unwrap();
        assert_eq!(document.frame_rate_num, 20);
        assert_eq!(document.frame_rate_den, 1);
        assert_eq!(document.frames[0].size, 4);
        assert_eq!(document.frames[0].hash, md5_hex(&[0, 0, 255, 255]));
    }

    #[test]
    fn restore_previous_disposal_reinstates_canvas_after_output() {
        let mut bytes = gif_header(1, 1, &[[255, 0, 0], [0, 255, 0]]);
        bytes.extend_from_slice(&graphic_control(10, 0, 0));
        bytes.extend_from_slice(&one_pixel_image(0));
        bytes.extend_from_slice(&graphic_control(10, 3 << 2, 0));
        bytes.extend_from_slice(&one_pixel_image(1));
        bytes.extend_from_slice(&graphic_control(10, 0, 0));
        bytes.extend_from_slice(&one_pixel_image(0));
        bytes.push(0x3B);

        let decoded = decode_gif(&bytes).unwrap();
        assert_eq!(decoded.frames[0].pixels, vec![0, 0, 255, 255]);
        assert_eq!(decoded.frames[1].pixels, vec![0, 255, 0, 255]);
        assert_eq!(decoded.frames[2].pixels, vec![0, 0, 255, 255]);
    }

    #[test]
    fn transparent_index_leaves_canvas_unchanged() {
        let mut bytes = gif_header(1, 1, &[[255, 0, 0], [0, 255, 0]]);
        bytes.extend_from_slice(&graphic_control(10, 0, 0));
        bytes.extend_from_slice(&one_pixel_image(0));
        bytes.extend_from_slice(&graphic_control(10, 0x01, 1));
        bytes.extend_from_slice(&one_pixel_image(1));
        bytes.push(0x3B);

        let decoded = decode_gif(&bytes).unwrap();
        assert_eq!(decoded.frames[0].pixels, vec![0, 0, 255, 255]);
        assert_eq!(decoded.frames[1].pixels, vec![0, 0, 255, 255]);
    }

    #[test]
    fn untouched_canvas_without_global_palette_is_transparent_white() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&graphic_control(10, 0, 0));
        bytes.extend_from_slice(&one_pixel_local_palette_image(0, &[[255, 0, 0], [0, 0, 0]]));
        bytes.push(0x3B);

        let decoded = decode_gif(&bytes).unwrap();
        assert_eq!(
            decoded.frames[0].pixels,
            vec![0, 0, 255, 255, 255, 255, 255, 0]
        );
    }
}
