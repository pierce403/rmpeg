use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const ID_EBML: u64 = 0x1A45DFA3;
const ID_DOCTYPE: u64 = 0x4282;
const ID_SEGMENT: u64 = 0x18538067;
const ID_TRACKS: u64 = 0x1654AE6B;
const ID_TRACK_ENTRY: u64 = 0xAE;
const ID_CLUSTER: u64 = 0x1F43B675;
const ID_TRACK_TYPE: u64 = 0x83;
const ID_CODEC_ID: u64 = 0x86;
const ID_VIDEO: u64 = 0xE0;
const ID_PIXEL_WIDTH: u64 = 0xB0;
const ID_PIXEL_HEIGHT: u64 = 0xBA;
const ID_AUDIO: u64 = 0xE1;
const ID_SAMPLING_FREQUENCY: u64 = 0xB5;
const ID_CHANNELS: u64 = 0x9F;
const ID_BIT_DEPTH: u64 = 0x6264;

const TRACK_TYPE_VIDEO: u64 = 1;
const TRACK_TYPE_AUDIO: u64 = 2;

pub fn looks_like_matroska(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
}

pub fn parse_matroska(bytes: &[u8]) -> Result<ProbeDocument> {
    require_matroska_doctype(bytes)?;
    let segment = find_first(bytes, 0, bytes.len(), ID_SEGMENT)?
        .ok_or_else(|| RmpegError::InvalidData("missing Matroska Segment".to_string()))?;
    let segment_end = segment.end.unwrap_or(bytes.len());
    find_first(bytes, segment.data_start, segment_end, ID_CLUSTER)?.ok_or_else(|| {
        RmpegError::InvalidData("missing Matroska Cluster with media data".to_string())
    })?;
    let tracks = find_first(bytes, segment.data_start, segment_end, ID_TRACKS)?
        .ok_or_else(|| RmpegError::InvalidData("missing Matroska Tracks".to_string()))?;
    let tracks_end = tracks
        .end
        .ok_or_else(|| RmpegError::InvalidData("Matroska Tracks has unknown size".to_string()))?;

    let mut streams = Vec::new();
    let mut pos = tracks.data_start;
    while pos < tracks_end {
        let element = read_element(bytes, pos)?;
        let next = element.next_pos(bytes.len())?;
        if element.id == ID_TRACK_ENTRY {
            let entry_end = element.end.ok_or_else(|| {
                RmpegError::InvalidData("Matroska TrackEntry has unknown size".to_string())
            })?;
            if let Some(stream) = parse_track_entry(bytes, element.data_start, entry_end)? {
                streams.push(stream);
            }
        }
        pos = next;
    }

    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "Matroska file has no supported audio or video tracks".to_string(),
        ));
    }

    for (index, stream) in streams.iter_mut().enumerate() {
        stream.index = index;
    }

    Ok(ProbeDocument {
        format: "matroska".to_string(),
        streams,
    })
}

fn require_matroska_doctype(bytes: &[u8]) -> Result<()> {
    let ebml = read_element(bytes, 0)?;
    if ebml.id != ID_EBML {
        return Err(RmpegError::InvalidData(
            "missing EBML header for Matroska".to_string(),
        ));
    }
    let ebml_end = ebml
        .end
        .ok_or_else(|| RmpegError::InvalidData("EBML header has unknown size".to_string()))?;
    let doc_type = find_first(bytes, ebml.data_start, ebml_end, ID_DOCTYPE)?
        .ok_or_else(|| RmpegError::InvalidData("missing EBML DocType".to_string()))?;
    let doc_end = doc_type
        .end
        .ok_or_else(|| RmpegError::InvalidData("EBML DocType has unknown size".to_string()))?;
    let value = std::str::from_utf8(&bytes[doc_type.data_start..doc_end])
        .map_err(|_| RmpegError::InvalidData("EBML DocType is not UTF-8".to_string()))?;
    if value == "matroska" || value == "webm" {
        Ok(())
    } else {
        Err(RmpegError::InvalidData(format!(
            "unsupported EBML DocType {value}"
        )))
    }
}

fn parse_track_entry(bytes: &[u8], start: usize, end: usize) -> Result<Option<StreamMetadata>> {
    let mut track = TrackBuilder::default();
    let mut pos = start;
    while pos < end {
        let element = read_element(bytes, pos)?;
        let next = element.next_pos(end)?;
        let element_end = element.end.ok_or_else(|| {
            RmpegError::InvalidData("Matroska track child has unknown size".to_string())
        })?;
        match element.id {
            ID_TRACK_TYPE => track.track_type = Some(read_uint(bytes, element)?),
            ID_CODEC_ID => {
                track.codec_id = Some(read_ascii(bytes, element)?.to_string());
            }
            ID_VIDEO => parse_video(bytes, element.data_start, element_end, &mut track)?,
            ID_AUDIO => parse_audio(bytes, element.data_start, element_end, &mut track)?,
            _ => {}
        }
        pos = next;
    }
    track.into_stream()
}

fn parse_video(bytes: &[u8], start: usize, end: usize, track: &mut TrackBuilder) -> Result<()> {
    let mut pos = start;
    while pos < end {
        let element = read_element(bytes, pos)?;
        let next = element.next_pos(end)?;
        match element.id {
            ID_PIXEL_WIDTH => track.width = Some(read_uint(bytes, element)? as u32),
            ID_PIXEL_HEIGHT => track.height = Some(read_uint(bytes, element)? as u32),
            _ => {}
        }
        pos = next;
    }
    Ok(())
}

fn parse_audio(bytes: &[u8], start: usize, end: usize, track: &mut TrackBuilder) -> Result<()> {
    let mut pos = start;
    while pos < end {
        let element = read_element(bytes, pos)?;
        let next = element.next_pos(end)?;
        match element.id {
            ID_SAMPLING_FREQUENCY => {
                let rate = read_float(bytes, element)?;
                if rate.is_finite() && rate > 0.0 && rate <= u32::MAX as f64 {
                    track.sample_rate = Some(rate.round() as u32);
                }
            }
            ID_CHANNELS => track.channels = Some(read_uint(bytes, element)? as u16),
            ID_BIT_DEPTH => track.bits_per_sample = Some(read_uint(bytes, element)? as u16),
            _ => {}
        }
        pos = next;
    }
    Ok(())
}

#[derive(Default)]
struct TrackBuilder {
    track_type: Option<u64>,
    codec_id: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    bits_per_sample: Option<u16>,
}

impl TrackBuilder {
    fn into_stream(self) -> Result<Option<StreamMetadata>> {
        let Some(track_type) = self.track_type else {
            return Ok(None);
        };
        let Some(codec_id) = self.codec_id else {
            return Ok(None);
        };
        let Some(codec_name) = codec_name(&codec_id) else {
            return Ok(None);
        };

        match track_type {
            TRACK_TYPE_VIDEO => {
                let width = self.width.ok_or_else(|| {
                    RmpegError::InvalidData("Matroska video track missing PixelWidth".to_string())
                })?;
                let height = self.height.ok_or_else(|| {
                    RmpegError::InvalidData("Matroska video track missing PixelHeight".to_string())
                })?;
                Ok(Some(StreamMetadata::video(
                    0, codec_name, width, height, None, None,
                )))
            }
            TRACK_TYPE_AUDIO => {
                let sample_rate = self.sample_rate.unwrap_or(8000);
                let channels = self.channels.unwrap_or(1);
                Ok(Some(StreamMetadata {
                    index: 0,
                    codec_type: "audio".to_string(),
                    codec_name: codec_name.to_string(),
                    sample_rate: Some(sample_rate),
                    channels: Some(channels),
                    bits_per_sample: Some(self.bits_per_sample.unwrap_or(0)),
                    duration_seconds: None,
                    width: None,
                    height: None,
                    frame_rate: None,
                }))
            }
            _ => Ok(None),
        }
    }
}

fn codec_name(codec_id: &str) -> Option<&'static str> {
    match codec_id {
        "V_VP8" => Some("vp8"),
        "V_VP9" => Some("vp9"),
        "V_AV1" => Some("av1"),
        "V_MPEG4/ISO/AVC" => Some("h264"),
        "V_MPEGH/ISO/HEVC" => Some("hevc"),
        "A_OPUS" => Some("opus"),
        "A_VORBIS" => Some("vorbis"),
        "A_AAC" => Some("aac"),
        "A_FLAC" => Some("flac"),
        "A_MPEG/L3" => Some("mp3"),
        "A_PCM/INT/LIT" => Some("pcm_s16le"),
        _ => None,
    }
}

fn find_first(bytes: &[u8], start: usize, end: usize, id: u64) -> Result<Option<Element>> {
    let mut pos = start;
    while pos < end {
        let element = read_element(bytes, pos)?;
        let next = element.next_pos(end)?;
        if element.id == id {
            return Ok(Some(element));
        }
        pos = next;
    }
    Ok(None)
}

fn read_uint(bytes: &[u8], element: Element) -> Result<u64> {
    let end = element
        .end
        .ok_or_else(|| RmpegError::InvalidData("integer element has unknown size".to_string()))?;
    let len = end - element.data_start;
    if len == 0 || len > 8 {
        return Err(RmpegError::InvalidData(format!(
            "unsupported EBML integer length {len}"
        )));
    }
    let mut value = 0_u64;
    for byte in &bytes[element.data_start..end] {
        value = (value << 8) | u64::from(*byte);
    }
    Ok(value)
}

fn read_float(bytes: &[u8], element: Element) -> Result<f64> {
    let end = element
        .end
        .ok_or_else(|| RmpegError::InvalidData("float element has unknown size".to_string()))?;
    match end - element.data_start {
        4 => {
            let raw = bytes[element.data_start..end]
                .try_into()
                .expect("slice length checked");
            Ok(f32::from_be_bytes(raw) as f64)
        }
        8 => {
            let raw = bytes[element.data_start..end]
                .try_into()
                .expect("slice length checked");
            Ok(f64::from_be_bytes(raw))
        }
        len => Err(RmpegError::InvalidData(format!(
            "unsupported EBML float length {len}"
        ))),
    }
}

fn read_ascii(bytes: &[u8], element: Element) -> Result<&str> {
    let end = element
        .end
        .ok_or_else(|| RmpegError::InvalidData("string element has unknown size".to_string()))?;
    std::str::from_utf8(&bytes[element.data_start..end])
        .map_err(|_| RmpegError::InvalidData("Matroska string is not UTF-8".to_string()))
}

#[derive(Clone, Copy)]
struct Element {
    id: u64,
    data_start: usize,
    end: Option<usize>,
}

impl Element {
    fn next_pos(self, container_end: usize) -> Result<usize> {
        match self.end {
            Some(end) if end <= container_end => Ok(end),
            Some(end) => Err(RmpegError::UnexpectedEof {
                needed: end,
                remaining: container_end,
            }),
            None => Ok(container_end),
        }
    }
}

fn read_element(bytes: &[u8], pos: usize) -> Result<Element> {
    let (id, id_len) = read_element_id(bytes, pos)?;
    let size_pos = pos
        .checked_add(id_len)
        .ok_or_else(|| RmpegError::InvalidData("EBML element header overflow".to_string()))?;
    let (size, size_len) = read_element_size(bytes, size_pos)?;
    let data_start = size_pos
        .checked_add(size_len)
        .ok_or_else(|| RmpegError::InvalidData("EBML element data offset overflow".to_string()))?;
    let end = size
        .map(usize::try_from)
        .transpose()
        .map_err(|_| RmpegError::InvalidData("EBML element size is too large".to_string()))?
        .map(|size| {
            data_start
                .checked_add(size)
                .ok_or_else(|| RmpegError::InvalidData("EBML element size overflows".to_string()))
        })
        .transpose()?;
    if let Some(end) = end {
        if end > bytes.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: end,
                remaining: bytes.len(),
            });
        }
    }
    Ok(Element {
        id,
        data_start,
        end,
    })
}

fn read_element_id(bytes: &[u8], pos: usize) -> Result<(u64, usize)> {
    let len = vint_len(bytes, pos, 4)?;
    let end = pos + len;
    let mut value = 0_u64;
    for byte in &bytes[pos..end] {
        value = (value << 8) | u64::from(*byte);
    }
    Ok((value, len))
}

fn read_element_size(bytes: &[u8], pos: usize) -> Result<(Option<u64>, usize)> {
    let len = vint_len(bytes, pos, 8)?;
    let end = pos + len;
    let marker = 1_u8 << (8 - len);
    let mut value = u64::from(bytes[pos] & !marker);
    for byte in &bytes[pos + 1..end] {
        value = (value << 8) | u64::from(*byte);
    }
    let unknown = value == (1_u64 << (7 * len)) - 1;
    Ok((if unknown { None } else { Some(value) }, len))
}

fn vint_len(bytes: &[u8], pos: usize, max_len: usize) -> Result<usize> {
    let first = *bytes.get(pos).ok_or(RmpegError::UnexpectedEof {
        needed: pos + 1,
        remaining: bytes.len(),
    })?;
    if first == 0 {
        return Err(RmpegError::InvalidData(
            "invalid zero EBML VINT".to_string(),
        ));
    }
    let mut mask = 0x80_u8;
    let mut len = 1_usize;
    while first & mask == 0 {
        len += 1;
        mask >>= 1;
    }
    if len > max_len {
        return Err(RmpegError::InvalidData(format!(
            "EBML VINT length {len} exceeds {max_len}"
        )));
    }
    let end = pos
        .checked_add(len)
        .ok_or_else(|| RmpegError::InvalidData("EBML VINT offset overflow".to_string()))?;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elem(id: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&size_vint(payload.len()));
        out.extend_from_slice(payload);
        out
    }

    fn uint_elem(id: &[u8], value: u64) -> Vec<u8> {
        elem(id, &[value as u8])
    }

    fn float_elem(id: &[u8], value: f64) -> Vec<u8> {
        elem(id, &value.to_be_bytes())
    }

    fn str_elem(id: &[u8], value: &str) -> Vec<u8> {
        elem(id, value.as_bytes())
    }

    fn size_vint(size: usize) -> Vec<u8> {
        assert!(size < 0x7f);
        vec![0x80 | size as u8]
    }

    fn minimal_webm(codec: &str, width: u8, height: u8) -> Vec<u8> {
        let ebml = elem(&[0x1A, 0x45, 0xDF, 0xA3], &str_elem(&[0x42, 0x82], "webm"));
        let video = elem(
            &[0xE0],
            &[
                uint_elem(&[0xB0], u64::from(width)),
                uint_elem(&[0xBA], u64::from(height)),
            ]
            .concat(),
        );
        let track = elem(
            &[0xAE],
            &[
                uint_elem(&[0x83], TRACK_TYPE_VIDEO),
                str_elem(&[0x86], codec),
                video,
            ]
            .concat(),
        );
        let tracks = elem(&[0x16, 0x54, 0xAE, 0x6B], &track);
        let cluster = elem(&[0x1F, 0x43, 0xB6, 0x75], &[]);
        let segment = elem(&[0x18, 0x53, 0x80, 0x67], &[tracks, cluster].concat());
        [ebml, segment].concat()
    }

    #[test]
    fn parses_vp9_track_dimensions() {
        let doc = parse_matroska(&minimal_webm("V_VP9", 64, 48)).expect("valid webm");
        assert_eq!(doc.format, "matroska");
        assert_eq!(doc.streams[0].codec_type, "video");
        assert_eq!(doc.streams[0].codec_name, "vp9");
        assert_eq!(doc.streams[0].width, Some(64));
        assert_eq!(doc.streams[0].height, Some(48));
    }

    #[test]
    fn parses_opus_audio_track() {
        let ebml = elem(
            &[0x1A, 0x45, 0xDF, 0xA3],
            &str_elem(&[0x42, 0x82], "matroska"),
        );
        let audio = elem(
            &[0xE1],
            &[
                float_elem(&[0xB5], 48_000.0),
                uint_elem(&[0x9F], 2),
                uint_elem(&[0x62, 0x64], 16),
            ]
            .concat(),
        );
        let track = elem(
            &[0xAE],
            &[
                uint_elem(&[0x83], TRACK_TYPE_AUDIO),
                str_elem(&[0x86], "A_OPUS"),
                audio,
            ]
            .concat(),
        );
        let tracks = elem(&[0x16, 0x54, 0xAE, 0x6B], &track);
        let cluster = elem(&[0x1F, 0x43, 0xB6, 0x75], &[]);
        let segment = elem(&[0x18, 0x53, 0x80, 0x67], &[tracks, cluster].concat());
        let doc = parse_matroska(&[ebml, segment].concat()).expect("valid mka");
        assert_eq!(doc.streams[0].codec_type, "audio");
        assert_eq!(doc.streams[0].codec_name, "opus");
        assert_eq!(doc.streams[0].sample_rate, Some(48_000));
        assert_eq!(doc.streams[0].channels, Some(2));
    }

    #[test]
    fn rejects_non_matroska_ebml_doctype() {
        let ebml = elem(
            &[0x1A, 0x45, 0xDF, 0xA3],
            &str_elem(&[0x42, 0x82], "not-webm"),
        );
        let err = parse_matroska(&ebml).expect_err("bad doctype");
        assert!(err.to_string().contains("DocType"));
    }

    #[test]
    fn rejects_header_without_cluster() {
        let ebml = elem(&[0x1A, 0x45, 0xDF, 0xA3], &str_elem(&[0x42, 0x82], "webm"));
        let video = elem(
            &[0xE0],
            &[
                uint_elem(&[0xB0], u64::from(64_u8)),
                uint_elem(&[0xBA], u64::from(48_u8)),
            ]
            .concat(),
        );
        let track = elem(
            &[0xAE],
            &[
                uint_elem(&[0x83], TRACK_TYPE_VIDEO),
                str_elem(&[0x86], "V_VP8"),
                video,
            ]
            .concat(),
        );
        let tracks = elem(&[0x16, 0x54, 0xAE, 0x6B], &track);
        let segment = elem(&[0x18, 0x53, 0x80, 0x67], &tracks);
        let err = parse_matroska(&[ebml, segment].concat()).expect_err("header only");
        assert!(err.to_string().contains("Cluster"));
    }
}
