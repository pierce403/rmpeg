use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlacPicture {
    codec_name: &'static str,
    width: u32,
    height: u32,
}

pub fn parse_flac(bytes: &[u8]) -> Result<ProbeDocument> {
    if !bytes.starts_with(b"fLaC") {
        return Err(RmpegError::InvalidData("missing FLAC marker".to_string()));
    }

    let mut pos = 4;
    let mut streaminfo = None;
    let mut pictures = Vec::new();
    while pos + 4 <= bytes.len() {
        let header = bytes[pos];
        let is_last = header & 0x80 != 0;
        let block_type = header & 0x7f;
        let len = (usize::from(bytes[pos + 1]) << 16)
            | (usize::from(bytes[pos + 2]) << 8)
            | usize::from(bytes[pos + 3]);
        let start = pos + 4;
        let end = start
            .checked_add(len)
            .ok_or_else(|| RmpegError::InvalidData("FLAC metadata size overflow".to_string()))?;
        if end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: end,
                remaining: bytes.len(),
            });
        }
        if block_type == 0 {
            streaminfo = Some(&bytes[start..end]);
        } else if block_type == 6 {
            if let Some(picture) = parse_picture(&bytes[start..end])? {
                pictures.push(picture);
            }
        }
        if is_last {
            break;
        }
        pos = end;
    }

    let streaminfo = streaminfo
        .ok_or_else(|| RmpegError::InvalidData("missing FLAC STREAMINFO block".to_string()))?;
    let audio = parse_streaminfo(streaminfo)?;
    let duration_seconds = audio.duration_seconds;
    let mut streams = vec![audio];
    for picture in pictures {
        streams.push(StreamMetadata::video(
            streams.len(),
            picture.codec_name,
            picture.width,
            picture.height,
            duration_seconds,
            None,
        ));
    }

    Ok(ProbeDocument {
        format: "flac".to_string(),
        streams,
    })
}

fn parse_streaminfo(data: &[u8]) -> Result<StreamMetadata> {
    if data.len() < 34 {
        return Err(RmpegError::UnexpectedEof {
            needed: 34,
            remaining: data.len(),
        });
    }
    let packed = u64::from_be_bytes([
        data[10], data[11], data[12], data[13], data[14], data[15], data[16], data[17],
    ]);
    let sample_rate = ((packed >> 44) & 0x000f_ffff) as u32;
    let channels = (((packed >> 41) & 0x07) + 1) as u16;
    let bits_per_sample = (((packed >> 36) & 0x1f) + 1) as u16;
    let total_samples = packed & 0x000f_ffff_ffff;
    if sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "FLAC STREAMINFO has zero sample rate".to_string(),
        ));
    }
    let duration_seconds = total_samples as f64 / sample_rate as f64;

    Ok(StreamMetadata::audio(
        0,
        "flac",
        sample_rate,
        channels,
        bits_per_sample,
        duration_seconds,
    ))
}

fn parse_picture(data: &[u8]) -> Result<Option<FlacPicture>> {
    if data.len() < 32 {
        return Err(RmpegError::UnexpectedEof {
            needed: 32,
            remaining: data.len(),
        });
    }
    let mime_len = usize::try_from(read_u32_be(data, 4)?)
        .map_err(|_| RmpegError::InvalidData("FLAC picture MIME is too large".to_string()))?;
    let mime_start = 8usize;
    let mime_end = mime_start
        .checked_add(mime_len)
        .ok_or_else(|| RmpegError::InvalidData("FLAC picture MIME overflow".to_string()))?;
    if mime_end > data.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: mime_end,
            remaining: data.len(),
        });
    }
    let codec_name = match &data[mime_start..mime_end] {
        b"image/jpeg" => "mjpeg",
        b"image/png" => "png",
        _ => return Ok(None),
    };

    let desc_len_pos = mime_end;
    let desc_len = usize::try_from(read_u32_be(data, desc_len_pos)?).map_err(|_| {
        RmpegError::InvalidData("FLAC picture description is too large".to_string())
    })?;
    let desc_start = desc_len_pos + 4;
    let desc_end = desc_start
        .checked_add(desc_len)
        .ok_or_else(|| RmpegError::InvalidData("FLAC picture description overflow".to_string()))?;
    let fields_end = desc_end
        .checked_add(20)
        .ok_or_else(|| RmpegError::InvalidData("FLAC picture fields overflow".to_string()))?;
    if fields_end > data.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: fields_end,
            remaining: data.len(),
        });
    }

    let mut width = read_u32_be(data, desc_end)?;
    let mut height = read_u32_be(data, desc_end + 4)?;
    let data_len = usize::try_from(read_u32_be(data, desc_end + 16)?)
        .map_err(|_| RmpegError::InvalidData("FLAC picture data is too large".to_string()))?;
    let image_end = fields_end
        .checked_add(data_len)
        .ok_or_else(|| RmpegError::InvalidData("FLAC picture data overflow".to_string()))?;
    if image_end > data.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: image_end,
            remaining: data.len(),
        });
    }
    let image_data = &data[fields_end..image_end];
    if width == 0 || height == 0 {
        let Some((parsed_width, parsed_height)) =
            parse_embedded_picture_dimensions(codec_name, image_data)
        else {
            return Ok(None);
        };
        width = parsed_width;
        height = parsed_height;
    }

    Ok(Some(FlacPicture {
        codec_name,
        width,
        height,
    }))
}

fn parse_embedded_picture_dimensions(codec_name: &str, image_data: &[u8]) -> Option<(u32, u32)> {
    let document = match codec_name {
        "mjpeg" => crate::jpeg::parse_jpeg(image_data).ok()?,
        "png" => crate::png::parse_png(image_data).ok()?,
        _ => return None,
    };
    let stream = document.streams.first()?;
    Some((stream.width?, stream.height?))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn streaminfo(sample_rate: u32, channels: u16, bits_per_sample: u16, samples: u64) -> Vec<u8> {
        let mut data = vec![0; 34];
        let packed = (u64::from(sample_rate) << 44)
            | (u64::from(channels - 1) << 41)
            | (u64::from(bits_per_sample - 1) << 36)
            | samples;
        data[10..18].copy_from_slice(&packed.to_be_bytes());
        data
    }

    fn push_block(bytes: &mut Vec<u8>, block_type: u8, is_last: bool, data: &[u8]) {
        bytes.push(block_type | if is_last { 0x80 } else { 0 });
        bytes.push(((data.len() >> 16) & 0xff) as u8);
        bytes.push(((data.len() >> 8) & 0xff) as u8);
        bytes.push((data.len() & 0xff) as u8);
        bytes.extend_from_slice(data);
    }

    fn picture_block(mime: &[u8], width: u32, height: u32) -> Vec<u8> {
        picture_block_with_payload(mime, width, height, &[0xff, 0xd8])
    }

    fn picture_block_with_payload(mime: &[u8], width: u32, height: u32, payload: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&3_u32.to_be_bytes());
        data.extend_from_slice(&(mime.len() as u32).to_be_bytes());
        data.extend_from_slice(mime);
        data.extend_from_slice(&0_u32.to_be_bytes());
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&24_u32.to_be_bytes());
        data.extend_from_slice(&0_u32.to_be_bytes());
        data.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        data.extend_from_slice(payload);
        data
    }

    fn minimal_jpeg(width: u16, height: u16) -> Vec<u8> {
        vec![
            0xff,
            0xd8,
            0xff,
            0xc0,
            0x00,
            0x0b,
            8,
            (height >> 8) as u8,
            height as u8,
            (width >> 8) as u8,
            width as u8,
            3,
            1,
            0x11,
            0,
            0xff,
            0xd9,
        ]
    }

    #[test]
    fn parses_streaminfo_only_flac() {
        let mut bytes = b"fLaC".to_vec();
        push_block(&mut bytes, 0, true, &streaminfo(44_100, 2, 16, 44_100));

        let doc = parse_flac(&bytes).expect("flac");

        assert_eq!(doc.format, "flac");
        assert_eq!(doc.streams.len(), 1);
        assert_eq!(doc.streams[0].codec_name, "flac");
        assert_eq!(doc.streams[0].duration_seconds, Some(1.0));
    }

    #[test]
    fn parses_flac_picture_block_as_attached_video_stream() {
        let mut bytes = b"fLaC".to_vec();
        push_block(&mut bytes, 0, false, &streaminfo(44_100, 2, 16, 88_200));
        push_block(&mut bytes, 6, true, &picture_block(b"image/jpeg", 350, 351));

        let doc = parse_flac(&bytes).expect("flac with picture");

        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[0].codec_name, "flac");
        assert_eq!(doc.streams[1].index, 1);
        assert_eq!(doc.streams[1].codec_name, "mjpeg");
        assert_eq!(doc.streams[1].width, Some(350));
        assert_eq!(doc.streams[1].height, Some(351));
        assert_eq!(doc.streams[1].duration_seconds, Some(2.0));
    }

    #[test]
    fn parses_flac_picture_dimensions_from_embedded_jpeg_when_block_dimensions_are_zero() {
        let mut bytes = b"fLaC".to_vec();
        push_block(&mut bytes, 0, false, &streaminfo(44_100, 2, 16, 88_200));
        push_block(
            &mut bytes,
            6,
            true,
            &picture_block_with_payload(b"image/jpeg", 0, 0, &minimal_jpeg(350, 352)),
        );

        let doc = parse_flac(&bytes).expect("flac with zero-dimension picture block");

        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[1].codec_name, "mjpeg");
        assert_eq!(doc.streams[1].width, Some(350));
        assert_eq!(doc.streams[1].height, Some(352));
    }
}
