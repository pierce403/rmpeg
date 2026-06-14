use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Default)]
struct IffState {
    width: Option<u32>,
    height: Option<u32>,
    sample_rate: Option<u32>,
    sample_count: Option<u32>,
    channels: Option<u16>,
    compression: Option<u8>,
}

pub fn parse_iff(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_iff(bytes) {
        return Err(RmpegError::InvalidData(
            "missing IFF FORM header".to_string(),
        ));
    }
    let form_type = &bytes[8..12];
    let mut state = IffState::default();
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = usize::try_from(read_u32_be(bytes, pos + 4)?)
            .map_err(|_| RmpegError::InvalidData("IFF chunk size is too large".to_string()))?;
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("IFF chunk size overflow".to_string()))?;
        if data_end > bytes.len() {
            break;
        }
        match id {
            b"BMHD" => parse_bmhd(&bytes[data_start..data_end], &mut state)?,
            b"VHDR" => parse_vhdr(&bytes[data_start..data_end], &mut state)?,
            b"CHAN" => parse_chan(&bytes[data_start..data_end], &mut state)?,
            _ => {}
        }
        pos = data_end + (size % 2);
    }

    match form_type {
        b"ILBM" | b"PBM " => image_document(state),
        b"8SVX" => audio_document(state),
        b"ANIM" => anim_document(bytes),
        _ => Err(RmpegError::InvalidData(
            "unsupported IFF FORM type".to_string(),
        )),
    }
}

pub fn looks_like_iff(bytes: &[u8]) -> bool {
    bytes.len() >= 12
        && bytes.starts_with(b"FORM")
        && matches!(&bytes[8..12], b"ILBM" | b"PBM " | b"8SVX" | b"ANIM")
}

fn parse_bmhd(data: &[u8], state: &mut IffState) -> Result<()> {
    if data.len() < 4 {
        return Err(RmpegError::UnexpectedEof {
            needed: 4,
            remaining: data.len(),
        });
    }
    state.width = Some(u32::from(read_u16_be(data, 0)?));
    state.height = Some(u32::from(read_u16_be(data, 2)?));
    Ok(())
}

fn parse_vhdr(data: &[u8], state: &mut IffState) -> Result<()> {
    if data.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: data.len(),
        });
    }
    let one_shot = read_u32_be(data, 0)?;
    let repeat = read_u32_be(data, 4)?;
    state.sample_count = Some(one_shot.saturating_add(repeat));
    state.sample_rate = Some(u32::from(read_u16_be(data, 12)?));
    state.compression = Some(data[15]);
    Ok(())
}

fn parse_chan(data: &[u8], state: &mut IffState) -> Result<()> {
    if data.len() < 4 {
        return Err(RmpegError::UnexpectedEof {
            needed: 4,
            remaining: data.len(),
        });
    }
    state.channels = Some(match read_u32_be(data, 0)? {
        6 => 2,
        _ => 1,
    });
    Ok(())
}

fn image_document(state: IffState) -> Result<ProbeDocument> {
    let width = state
        .width
        .filter(|width| *width != 0)
        .ok_or_else(|| RmpegError::InvalidData("IFF image width missing".to_string()))?;
    let height = state
        .height
        .filter(|height| *height != 0)
        .ok_or_else(|| RmpegError::InvalidData("IFF image height missing".to_string()))?;
    Ok(ProbeDocument {
        format: "iff".to_string(),
        streams: vec![StreamMetadata::video(
            0,
            "iff_ilbm",
            width,
            height,
            Some(0.0),
            None,
        )],
    })
}

fn audio_document(state: IffState) -> Result<ProbeDocument> {
    let sample_rate = state
        .sample_rate
        .filter(|sample_rate| *sample_rate != 0)
        .ok_or_else(|| RmpegError::InvalidData("IFF sample rate missing".to_string()))?;
    let sample_count = state.sample_count.unwrap_or(0);
    let codec = match state.compression {
        Some(1) => "8svx_fib",
        Some(0) | None => "pcm_s8",
        Some(other) => {
            return Err(RmpegError::InvalidData(format!(
                "unsupported 8SVX compression {other}"
            )));
        }
    };
    Ok(ProbeDocument {
        format: "iff".to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec,
            sample_rate,
            state.channels.unwrap_or(1),
            if codec == "8svx_fib" { 4 } else { 8 },
            sample_count as f64 / sample_rate as f64,
        )],
    })
}

fn anim_document(bytes: &[u8]) -> Result<ProbeDocument> {
    let mut width = None;
    let mut height = None;
    let mut sample_rate = None;
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = usize::try_from(read_u32_be(bytes, pos + 4)?)
            .map_err(|_| RmpegError::InvalidData("IFF chunk size is too large".to_string()))?;
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("IFF chunk size overflow".to_string()))?;
        if data_end > bytes.len() {
            break;
        }
        if id == b"FORM" && bytes.get(data_start..data_start + 4) == Some(b"ILBM") {
            let mut nested = IffState::default();
            parse_nested_anim_form(bytes, data_start + 4, data_end, &mut nested)?;
            width = nested.width.or(width);
            height = nested.height.or(height);
            sample_rate = nested.sample_rate.or(sample_rate);
        } else if id == b"SXHD" && data_end - data_start >= 20 {
            sample_rate = Some(u32::from(read_u16_be(bytes, data_start + 18)?));
        }
        pos = data_end + (size % 2);
    }
    let width =
        width.ok_or_else(|| RmpegError::InvalidData("IFF ANIM width missing".to_string()))?;
    let height =
        height.ok_or_else(|| RmpegError::InvalidData("IFF ANIM height missing".to_string()))?;
    let sample_rate = sample_rate
        .ok_or_else(|| RmpegError::InvalidData("IFF ANIM sample rate missing".to_string()))?;
    Ok(ProbeDocument {
        format: "iff".to_string(),
        streams: vec![
            StreamMetadata::video(0, "iff_ilbm", width, height, Some(0.0), None),
            StreamMetadata::audio(1, "pcm_s8_planar", sample_rate, 2, 8, 0.0),
        ],
    })
}

fn parse_nested_anim_form(
    bytes: &[u8],
    start: usize,
    end: usize,
    state: &mut IffState,
) -> Result<()> {
    let mut pos = start;
    while pos + 8 <= end {
        let id = &bytes[pos..pos + 4];
        let size = usize::try_from(read_u32_be(bytes, pos + 4)?)
            .map_err(|_| RmpegError::InvalidData("IFF chunk size is too large".to_string()))?;
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("IFF chunk size overflow".to_string()))?;
        if data_end > end {
            break;
        }
        match id {
            b"BMHD" => parse_bmhd(&bytes[data_start..data_end], state)?,
            b"SXHD" if data_end - data_start >= 20 => {
                state.sample_rate = Some(u32::from(read_u16_be(bytes, data_start + 18)?));
            }
            _ => {}
        }
        pos = data_end + (size % 2);
    }
    Ok(())
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

    #[test]
    fn parses_iff_image_dimensions() {
        let mut bytes = b"FORM\0\0\0\x20ILBMBMHD\0\0\0\x14".to_vec();
        bytes.extend_from_slice(&320_u16.to_be_bytes());
        bytes.extend_from_slice(&200_u16.to_be_bytes());
        bytes.extend_from_slice(&[0; 16]);
        let doc = parse_iff(&bytes).expect("valid iff");
        assert_eq!(doc.streams[0].width, Some(320));
        assert_eq!(doc.streams[0].height, Some(200));
    }

    #[test]
    fn parses_8svx_audio_metadata() {
        let mut bytes = b"FORM\0\0\0\x308SVXVHDR\0\0\0\x14".to_vec();
        bytes.extend_from_slice(&100_u32.to_be_bytes());
        bytes.extend_from_slice(&20_u32.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&12_000_u16.to_be_bytes());
        bytes.extend_from_slice(&[1, 1, 0, 0, 0, 0]);
        bytes.extend_from_slice(b"CHAN\0\0\0\x04");
        bytes.extend_from_slice(&6_u32.to_be_bytes());
        let doc = parse_iff(&bytes).expect("valid 8svx");
        assert_eq!(doc.streams[0].codec_name, "8svx_fib");
        assert_eq!(doc.streams[0].channels, Some(2));
        assert_eq!(doc.streams[0].duration_seconds, Some(0.01));
    }

    #[test]
    fn parses_anim_ilbm_with_sxhd_audio_metadata() {
        let mut bytes = b"FORM\0\0\0\0ANIMFORM\0\0\0\x3eILBMBMHD\0\0\0\x14".to_vec();
        bytes.extend_from_slice(&320_u16.to_be_bytes());
        bytes.extend_from_slice(&200_u16.to_be_bytes());
        bytes.extend_from_slice(&[0; 16]);
        bytes.extend_from_slice(b"SXHD\0\0\0\x16");
        bytes.extend_from_slice(&[0; 18]);
        bytes.extend_from_slice(&14_977_u16.to_be_bytes());
        bytes.extend_from_slice(&[0; 2]);

        let doc = parse_iff(&bytes).expect("anim");

        assert_eq!(doc.streams[0].codec_name, "iff_ilbm");
        assert_eq!(doc.streams[0].width, Some(320));
        assert_eq!(doc.streams[1].codec_name, "pcm_s8_planar");
        assert_eq!(doc.streams[1].sample_rate, Some(14_977));
    }
}
