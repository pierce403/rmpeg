use openh264::decoder::{Decoder, DecoderConfig, Flush};
use openh264::formats::YUVSource;
use openh264::OpenH264API;
use rmpeg_core::{AudioFrameHash, Result, RmpegError};
use rmpeg_format::extract_mp4_h264_samples;

use crate::md5::md5_hex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrameHashDocument {
    pub width: u32,
    pub height: u32,
    pub frame_rate_num: u32,
    pub frame_rate_den: u32,
    pub frames: Vec<AudioFrameHash>,
}

pub fn mp4_h264_frame_hashes(input: &[u8]) -> Result<VideoFrameHashDocument> {
    let sample_data = extract_mp4_h264_samples(input)?.ok_or_else(|| {
        RmpegError::Unsupported("MP4 H264 sample extraction is not supported".to_string())
    })?;
    let api = openh264_api()?;
    let config = DecoderConfig::new()
        .debug(false)
        .flush_after_decode(Flush::NoFlush);
    let mut decoder = Decoder::with_api_config(api, config).map_err(map_openh264_error)?;
    let mut converter = Mp4H264BitstreamConverter::new(
        sample_data.length_size,
        sample_data.sps.clone(),
        sample_data.pps.clone(),
    )?;
    let mut packet = Vec::new();
    let mut frames = Vec::new();

    for sample in &sample_data.samples {
        converter.convert_packet(sample, &mut packet)?;
        if packet.is_empty() {
            continue;
        }
        if let Some(image) = decoder.decode(&packet).map_err(map_openh264_error)? {
            push_yuv420p_frame_hash(&image, sample_data.width, sample_data.height, &mut frames)?;
        }
    }
    for image in decoder.flush_remaining().map_err(map_openh264_error)? {
        push_yuv420p_frame_hash(&image, sample_data.width, sample_data.height, &mut frames)?;
    }

    if frames.is_empty() {
        return Err(RmpegError::InvalidData(
            "OpenH264 did not produce decoded frames".to_string(),
        ));
    }

    Ok(VideoFrameHashDocument {
        width: sample_data.width,
        height: sample_data.height,
        frame_rate_num: sample_data.frame_rate_num,
        frame_rate_den: sample_data.frame_rate_den,
        frames,
    })
}

fn openh264_api() -> Result<OpenH264API> {
    let candidates = [
        "/usr/lib/x86_64-linux-gnu/libopenh264.so.8",
        "/usr/lib/x86_64-linux-gnu/libopenh264.so",
        "/usr/local/lib/libopenh264.so.8",
        "/usr/local/lib/libopenh264.so",
        "libopenh264.so.8",
        "libopenh264.so",
    ];
    let mut last_error = String::new();
    for candidate in candidates {
        match OpenH264API::from_blob_path(candidate) {
            Ok(api) => return Ok(api),
            Err(error) => last_error = error.to_string(),
        }
    }
    for candidate in candidates {
        // Distro OpenH264 builds are not in openh264-sys2's Cisco blob hash list,
        // but they expose the stable WelsCreate/WelsDestroy C ABI used here.
        match unsafe { OpenH264API::from_blob_path_unchecked(candidate) } {
            Ok(api) => return Ok(api),
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(RmpegError::Unsupported(format!(
        "OpenH264 shared library was not loadable: {last_error}"
    )))
}

fn map_openh264_error(error: openh264::Error) -> RmpegError {
    RmpegError::InvalidData(format!("OpenH264 decode failed: {error}"))
}

struct Mp4H264BitstreamConverter {
    length_size: usize,
    sps: Vec<Vec<u8>>,
    pps: Vec<Vec<u8>>,
}

impl Mp4H264BitstreamConverter {
    fn new(length_size: usize, sps: Vec<Vec<u8>>, pps: Vec<Vec<u8>>) -> Result<Self> {
        if !matches!(length_size, 1 | 2 | 4) {
            return Err(RmpegError::InvalidData(
                "invalid H264 MP4 NAL length size".to_string(),
            ));
        }
        if sps.is_empty() || pps.is_empty() {
            return Err(RmpegError::InvalidData(
                "missing H264 MP4 parameter sets".to_string(),
            ));
        }
        Ok(Self {
            length_size,
            sps,
            pps,
        })
    }

    fn convert_packet(&mut self, packet: &[u8], out: &mut Vec<u8>) -> Result<()> {
        out.clear();
        let mut pos = 0_usize;
        let mut saw_sps = false;
        let mut saw_pps = false;
        let mut injected_parameter_sets = false;
        while pos < packet.len() {
            let (nal, next_pos) = read_length_prefixed_nal(packet, pos, self.length_size)?;
            pos = next_pos;
            if nal.is_empty() {
                continue;
            }
            match nal[0] & 0x1f {
                5 if !(injected_parameter_sets || saw_sps && saw_pps) => {
                    for sps in &self.sps {
                        append_annex_b_nal(out, sps);
                    }
                    for pps in &self.pps {
                        append_annex_b_nal(out, pps);
                    }
                    injected_parameter_sets = true;
                    append_annex_b_nal(out, nal);
                }
                7 => {
                    saw_sps = true;
                    append_annex_b_nal(out, nal);
                }
                8 => {
                    saw_pps = true;
                    append_annex_b_nal(out, nal);
                }
                _ => append_annex_b_nal(out, nal),
            }
        }
        Ok(())
    }
}

fn read_length_prefixed_nal(
    packet: &[u8],
    pos: usize,
    length_size: usize,
) -> Result<(&[u8], usize)> {
    let len_end = pos
        .checked_add(length_size)
        .ok_or_else(|| RmpegError::InvalidData("H264 packet length offset overflow".to_string()))?;
    if len_end > packet.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: len_end,
            remaining: packet.len(),
        });
    }
    let mut nal_len = 0_usize;
    for byte in &packet[pos..len_end] {
        nal_len = nal_len
            .checked_shl(8)
            .and_then(|value| value.checked_add(usize::from(*byte)))
            .ok_or_else(|| RmpegError::InvalidData("H264 NAL length overflow".to_string()))?;
    }
    let nal_end = len_end
        .checked_add(nal_len)
        .ok_or_else(|| RmpegError::InvalidData("H264 NAL offset overflow".to_string()))?;
    if nal_end > packet.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: nal_end,
            remaining: packet.len(),
        });
    }
    Ok((&packet[len_end..nal_end], nal_end))
}

fn append_annex_b_nal(out: &mut Vec<u8>, nal: &[u8]) {
    out.extend_from_slice(&[0, 0, 1]);
    out.extend_from_slice(nal);
}

fn push_yuv420p_frame_hash(
    image: &impl YUVSource,
    expected_width: u32,
    expected_height: u32,
    frames: &mut Vec<AudioFrameHash>,
) -> Result<()> {
    let frame = pack_yuv420p(image)?;
    let (width, height) = image.dimensions();
    if width != expected_width as usize || height != expected_height as usize {
        return Err(RmpegError::Unsupported(format!(
            "decoded H264 dimensions changed from {expected_width}x{expected_height} to {width}x{height}"
        )));
    }
    let pts = frames.len() as u64;
    frames.push(AudioFrameHash {
        stream_index: 0,
        dts: pts,
        pts,
        duration: 1,
        size: frame.len(),
        hash: md5_hex(&frame),
    });
    Ok(())
}

fn pack_yuv420p(image: &impl YUVSource) -> Result<Vec<u8>> {
    let (width, height) = image.dimensions();
    if width % 2 != 0 || height % 2 != 0 {
        return Err(RmpegError::Unsupported(
            "decoded H264 frame dimensions are not YUV420-compatible".to_string(),
        ));
    }
    let (y_stride, u_stride, v_stride) = image.strides();
    let plane_len = width
        .checked_mul(height)
        .ok_or_else(|| RmpegError::InvalidData("video frame dimensions overflow".to_string()))?;
    let mut out = Vec::with_capacity(plane_len * 3 / 2);
    append_plane_rows(image.y(), width, height, y_stride, &mut out)?;
    append_plane_rows(image.u(), width / 2, height / 2, u_stride, &mut out)?;
    append_plane_rows(image.v(), width / 2, height / 2, v_stride, &mut out)?;
    Ok(out)
}

fn append_plane_rows(
    plane: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    out: &mut Vec<u8>,
) -> Result<()> {
    for row in 0..height {
        let start = row
            .checked_mul(stride)
            .ok_or_else(|| RmpegError::InvalidData("video plane offset overflow".to_string()))?;
        let end = start
            .checked_add(width)
            .ok_or_else(|| RmpegError::InvalidData("video plane row overflow".to_string()))?;
        if end > plane.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: end,
                remaining: plane.len(),
            });
        }
        out.extend_from_slice(&plane[start..end]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestYuv420 {
        dimensions: (usize, usize),
        strides: (usize, usize, usize),
        y: Vec<u8>,
        u: Vec<u8>,
        v: Vec<u8>,
    }

    impl YUVSource for TestYuv420 {
        fn dimensions(&self) -> (usize, usize) {
            self.dimensions
        }

        fn strides(&self) -> (usize, usize, usize) {
            self.strides
        }

        fn y(&self) -> &[u8] {
            &self.y
        }

        fn u(&self) -> &[u8] {
            &self.u
        }

        fn v(&self) -> &[u8] {
            &self.v
        }
    }

    #[test]
    fn converts_mp4_h264_packet_to_annex_b_and_injects_parameter_sets() {
        let mut converter =
            Mp4H264BitstreamConverter::new(4, vec![vec![0x67, 0x42]], vec![vec![0x68, 0xce]])
                .unwrap();
        let packet = [0, 0, 0, 2, 0x06, 0x05, 0, 0, 0, 3, 0x65, 0x88, 0x84];
        let mut out = Vec::new();

        converter.convert_packet(&packet, &mut out).unwrap();

        assert_eq!(
            out,
            vec![
                0, 0, 1, 0x06, 0x05, 0, 0, 1, 0x67, 0x42, 0, 0, 1, 0x68, 0xce, 0, 0, 1, 0x65, 0x88,
                0x84,
            ]
        );
    }

    #[test]
    fn packs_yuv420p_with_strides_to_contiguous_planes() {
        let image = TestYuv420 {
            dimensions: (4, 2),
            strides: (6, 3, 3),
            y: vec![1, 2, 3, 4, 99, 99, 5, 6, 7, 8, 99, 99],
            u: vec![9, 10, 99],
            v: vec![11, 12, 99],
        };

        let packed = pack_yuv420p(&image).unwrap();

        assert_eq!(packed, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    }
}
