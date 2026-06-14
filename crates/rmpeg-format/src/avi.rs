use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Default)]
struct AviStreamBuilder {
    stream_type: Option<[u8; 4]>,
    handler: Option<[u8; 4]>,
    scale: Option<u32>,
    rate: Option<u32>,
    length: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    bitmap_codec: Option<[u8; 4]>,
    audio_format: Option<u16>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    bits_per_sample: Option<u16>,
    observed_chunks: Option<u32>,
}

pub fn parse_avi(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_avi(bytes) {
        return Err(RmpegError::InvalidData(
            "missing AVI RIFF header".to_string(),
        ));
    }

    let mut builders = Vec::new();
    parse_chunks(bytes, 12, bytes.len(), &mut builders)?;
    let observed_chunks = count_movi_chunks(bytes);
    let mut streams = Vec::new();
    for (index, mut builder) in builders.into_iter().enumerate() {
        builder.observed_chunks = observed_chunks
            .get(index)
            .copied()
            .filter(|count| *count > 0);
        if let Some(stream) = builder.into_stream(streams.len()) {
            streams.push(stream);
        }
    }
    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "AVI file has no supported streams".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "avi".to_string(),
        streams,
    })
}

fn count_movi_chunks(bytes: &[u8]) -> Vec<u32> {
    let mut counts = Vec::new();
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let Some(chunk) = ChunkHeader::read_tolerant(bytes, pos, bytes.len()) else {
            break;
        };
        if &chunk.id == b"LIST" && chunk.data_start + 4 <= chunk.end {
            let list_type = &bytes[chunk.data_start..chunk.data_start + 4];
            if list_type == b"movi" {
                count_stream_chunks(bytes, chunk.data_start + 4, chunk.end, &mut counts);
                break;
            }
        }
        pos = chunk.padded_end();
    }
    counts
}

fn count_stream_chunks(bytes: &[u8], mut pos: usize, end: usize, counts: &mut Vec<u32>) {
    while pos + 8 <= end {
        let Some(chunk) = ChunkHeader::read_tolerant(bytes, pos, end) else {
            break;
        };
        if &chunk.id == b"LIST" && chunk.data_start + 4 <= chunk.end {
            count_stream_chunks(bytes, chunk.data_start + 4, chunk.end, counts);
        } else if let Some(stream_index) = avi_chunk_stream_index(&chunk.id) {
            if counts.len() <= stream_index {
                counts.resize(stream_index + 1, 0);
            }
            counts[stream_index] = counts[stream_index].saturating_add(1);
        }
        pos = chunk.padded_end();
    }
}

fn avi_chunk_stream_index(id: &[u8; 4]) -> Option<usize> {
    if !id[0].is_ascii_digit() || !id[1].is_ascii_digit() {
        return None;
    }
    if !matches!(&id[2..4], b"db" | b"dc" | b"wb") {
        return None;
    }
    Some(usize::from(id[0] - b'0') * 10 + usize::from(id[1] - b'0'))
}

pub fn looks_like_avi(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"AVI "
}

fn parse_chunks(
    bytes: &[u8],
    mut pos: usize,
    end: usize,
    streams: &mut Vec<AviStreamBuilder>,
) -> Result<()> {
    while pos + 8 <= end {
        let chunk = match ChunkHeader::read(bytes, pos, end) {
            Ok(chunk) => chunk,
            Err(error) => {
                if streams.is_empty() {
                    return Err(error);
                }
                break;
            }
        };
        if &chunk.id == b"LIST" && chunk.data_start + 4 <= chunk.end {
            let list_type = &bytes[chunk.data_start..chunk.data_start + 4];
            if list_type == b"strl" {
                if let Some(stream) = parse_stream_list(bytes, chunk.data_start + 4, chunk.end)? {
                    streams.push(stream);
                }
            } else {
                parse_chunks(bytes, chunk.data_start + 4, chunk.end, streams)?;
            }
        }
        pos = chunk.padded_end();
    }
    Ok(())
}

fn parse_stream_list(bytes: &[u8], mut pos: usize, end: usize) -> Result<Option<AviStreamBuilder>> {
    let mut stream = AviStreamBuilder::default();
    while pos + 8 <= end {
        let chunk = ChunkHeader::read(bytes, pos, end)?;
        match &chunk.id {
            b"strh" => parse_stream_header(&bytes[chunk.data_start..chunk.end], &mut stream)?,
            b"strf" => parse_stream_format(&bytes[chunk.data_start..chunk.end], &mut stream)?,
            b"LIST" if chunk.data_start + 4 <= chunk.end => {
                if let Some(nested) = parse_stream_list(bytes, chunk.data_start + 4, chunk.end)? {
                    return Ok(Some(nested));
                }
            }
            _ => {}
        }
        pos = chunk.padded_end();
    }
    Ok(Some(stream))
}

fn parse_stream_header(data: &[u8], stream: &mut AviStreamBuilder) -> Result<()> {
    if data.len() < 56 {
        return Err(RmpegError::UnexpectedEof {
            needed: 56,
            remaining: data.len(),
        });
    }
    stream.stream_type = Some([data[0], data[1], data[2], data[3]]);
    stream.handler = Some(normalize_fourcc([data[4], data[5], data[6], data[7]]));
    stream.scale = Some(read_u32(data, 20)?);
    stream.rate = Some(read_u32(data, 24)?);
    stream.length = Some(read_u32(data, 32)?);
    Ok(())
}

fn parse_stream_format(data: &[u8], stream: &mut AviStreamBuilder) -> Result<()> {
    match stream.stream_type.as_ref() {
        Some(b"vids") => parse_video_stream_format(data, stream),
        Some(b"auds") => parse_audio_stream_format(data, stream),
        _ => Ok(()),
    }
}

fn parse_video_stream_format(data: &[u8], stream: &mut AviStreamBuilder) -> Result<()> {
    if data.len() < 20 {
        return Err(RmpegError::UnexpectedEof {
            needed: 20,
            remaining: data.len(),
        });
    }
    let header_size = read_u32(data, 0)?;
    if header_size < 16 {
        return Err(RmpegError::InvalidData(
            "AVI bitmap header is too small".to_string(),
        ));
    }
    let width = read_i32(data, 4)?;
    let height = read_i32(data, 8)?;
    stream.width = Some(width.unsigned_abs());
    stream.height = Some(height.unsigned_abs());
    stream.bitmap_codec = Some(normalize_fourcc([data[16], data[17], data[18], data[19]]));
    Ok(())
}

fn parse_audio_stream_format(data: &[u8], stream: &mut AviStreamBuilder) -> Result<()> {
    if data.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: data.len(),
        });
    }
    stream.audio_format = Some(read_u16(data, 0)?);
    stream.channels = Some(read_u16(data, 2)?);
    stream.sample_rate = Some(read_u32(data, 4)?);
    stream.bits_per_sample = Some(read_u16(data, 14)?);
    Ok(())
}

impl AviStreamBuilder {
    fn into_stream(self, index: usize) -> Option<StreamMetadata> {
        let stream_type = self.stream_type?;
        if &stream_type == b"vids" {
            let codec = video_codec_name(self.bitmap_codec.or(self.handler)?)?;
            let length = match (self.length, self.observed_chunks) {
                (Some(declared), Some(observed)) => Some(declared.min(observed)),
                (declared, _) => declared,
            };
            return Some(StreamMetadata::video(
                index,
                codec,
                self.width?,
                self.height?,
                duration_seconds(length, self.scale, self.rate),
                None,
            ));
        }
        if &stream_type == b"auds" {
            let format = self.audio_format?;
            let codec = audio_codec_name(format, self.bits_per_sample)?;
            let duration = if index == 0 {
                audio_duration_seconds(format, self.length, self.scale, self.rate).unwrap_or(0.0)
            } else {
                0.0
            };
            return Some(StreamMetadata::audio(
                index,
                codec,
                self.sample_rate?,
                self.channels?,
                audio_bits_per_sample(format, self.bits_per_sample),
                duration,
            ));
        }
        None
    }
}

fn duration_seconds(length: Option<u32>, scale: Option<u32>, rate: Option<u32>) -> Option<f64> {
    match (length, scale, rate) {
        (Some(length), Some(scale), Some(rate)) if rate != 0 => {
            Some(length as f64 * scale as f64 / rate as f64)
        }
        _ => None,
    }
}

fn audio_duration_seconds(
    format: u16,
    length: Option<u32>,
    scale: Option<u32>,
    rate: Option<u32>,
) -> Option<f64> {
    let length = match (format, length) {
        (0x0061 | 0x0062, Some(length)) => Some(length.saturating_add(8)),
        (_, length) => length,
    };
    duration_seconds(length, scale, rate)
}

fn audio_bits_per_sample(format: u16, bits_per_sample: Option<u16>) -> u16 {
    if format == 0x0001 {
        bits_per_sample.unwrap_or(0)
    } else {
        0
    }
}

fn video_codec_name(fourcc: [u8; 4]) -> Option<&'static str> {
    match &fourcc {
        [1, 0, 0, 0] => Some("msrle"),
        b"012V" => Some("012v"),
        b"AASC" => Some("aasc"),
        b"AURA" => Some("aura"),
        b"AUR2" => Some("aura2"),
        b"AVRN" => Some("avrn"),
        b"CFHD" => Some("cfhd"),
        b"CHQX" => Some("hqx"),
        b"CLJR" => Some("cljr"),
        b"CLLC" => Some("cllc"),
        b"CRAM" | b"MSVC" => Some("msvideo1"),
        b"CSCD" => Some("cscd"),
        b"CUVC" => Some("hq_hqa"),
        b"CVID" => Some("cinepak"),
        b"DUCK" => Some("truemotion1"),
        b"FFVH" => Some("ffvhuff"),
        b"FICV" => Some("fic"),
        b"FMVC" => Some("fmvc"),
        b"FPS1" => Some("fraps"),
        b"FRWU" => Some("frwu"),
        b"ISCC" => Some("rscc"),
        b"IV32" => Some("indeo3"),
        b"IV41" => Some("indeo4"),
        b"IV50" => Some("indeo5"),
        b"KGV1" => Some("kgv1"),
        b"KMVC" => Some("kmvc"),
        b"LAGS" => Some("lagarith"),
        b"LEAD" => Some("lead"),
        b"LOCO" => Some("loco"),
        b"LSCR" => Some("lscr"),
        b"MAGY" => Some("magicyuv"),
        b"MJPG" => Some("mjpeg"),
        b"MPG4" => Some("msmpeg4v1"),
        b"MSZH" => Some("mszh"),
        b"PNG1" => Some("png"),
        b"Q1.0" => Some("qpeg"),
        b"R210" => Some("r210"),
        b"RSCC" => Some("rscc"),
        b"RT21" => Some("indeo2"),
        b"SMV2" => Some("h264"),
        b"SP54" => Some("sp5x"),
        b"SPV1" => Some("screenpresso"),
        b"TM20" => Some("truemotion2"),
        b"TR20" => Some("truemotion2rt"),
        b"TSCC" => Some("tscc"),
        b"TSC2" => Some("tscc2"),
        b"ULTI" => Some("ulti"),
        b"ULRG" | b"ULRA" | b"ULH0" | b"ULH2" | b"ULH4" | b"ULY0" | b"ULY2" => Some("utvideo"),
        b"V210" => Some("v210"),
        b"VBLE" => Some("vble"),
        b"VCR1" => Some("vcr1"),
        b"VCR2" => Some("mpeg2video"),
        b"VIXL" => Some("vixl"),
        b"VMNC" => Some("vmnc"),
        b"VP31" => Some("vp3"),
        b"VP40" => Some("vp4"),
        b"VP50" => Some("vp5"),
        b"VP60" => Some("vp6"),
        b"VP70" => Some("vp7"),
        b"VQC1" => Some("vqc"),
        b"WNV1" => Some("wnv1"),
        b"XVID" | b"DIVX" | b"MP4V" => Some("mpeg4"),
        b"XXAN" => Some("xan_wc4"),
        b"XTOR" => Some("dxtory"),
        b"ZEC0" | b"ZECO" => Some("zerocodec"),
        b"ZLIB" => Some("zlib"),
        b"ZMBV" => Some("zmbv"),
        _ => None,
    }
}

fn audio_codec_name(format: u16, bits_per_sample: Option<u16>) -> Option<&'static str> {
    match (format, bits_per_sample) {
        (0x0001, Some(8)) => Some("pcm_u8"),
        (0x0001, Some(16)) => Some("pcm_s16le"),
        (0x0055, _) => Some("mp3"),
        (0x0061, _) => Some("adpcm_ima_dk4"),
        (0x0062, _) => Some("adpcm_ima_dk3"),
        _ => None,
    }
}

fn normalize_fourcc(mut fourcc: [u8; 4]) -> [u8; 4] {
    fourcc.make_ascii_uppercase();
    fourcc
}

#[derive(Debug, Clone, Copy)]
struct ChunkHeader {
    id: [u8; 4],
    data_start: usize,
    end: usize,
}

impl ChunkHeader {
    fn read(bytes: &[u8], pos: usize, limit: usize) -> Result<Self> {
        if limit.saturating_sub(pos) < 8 {
            return Err(RmpegError::UnexpectedEof {
                needed: 8,
                remaining: limit.saturating_sub(pos),
            });
        }
        let size = read_u32(bytes, pos + 4)? as usize;
        let data_start = pos + 8;
        let end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("AVI chunk size overflow".to_string()))?;
        if end > limit {
            return Err(RmpegError::InvalidData(format!(
                "invalid AVI chunk {} size {}",
                String::from_utf8_lossy(&bytes[pos..pos + 4]),
                size
            )));
        }
        Ok(Self {
            id: [bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]],
            data_start,
            end,
        })
    }

    fn padded_end(self) -> usize {
        self.end + ((self.end - self.data_start) & 1)
    }

    fn read_tolerant(bytes: &[u8], pos: usize, limit: usize) -> Option<Self> {
        if limit.saturating_sub(pos) < 8 {
            return None;
        }
        let size = read_u32(bytes, pos + 4).ok()? as usize;
        let data_start = pos.checked_add(8)?;
        let declared_end = data_start.checked_add(size)?;
        Some(Self {
            id: [bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]],
            data_start,
            end: declared_end.min(limit),
        })
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(i32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_utvideo_avi_metadata() {
        let mut bytes = Vec::new();
        let mut strh_payload = Vec::new();
        strh_payload.extend_from_slice(b"vids");
        strh_payload.extend_from_slice(b"ULRG");
        strh_payload.extend_from_slice(&[0; 12]);
        strh_payload.extend_from_slice(&1001_u32.to_le_bytes());
        strh_payload.extend_from_slice(&30000_u32.to_le_bytes());
        strh_payload.extend_from_slice(&[0; 4]);
        strh_payload.extend_from_slice(&4_u32.to_le_bytes());
        strh_payload.extend_from_slice(&[0; 20]);
        let strh = avi_chunk(b"strh", &strh_payload);

        let mut strf_payload = Vec::new();
        strf_payload.extend_from_slice(&56_u32.to_le_bytes());
        strf_payload.extend_from_slice(&640_i32.to_le_bytes());
        strf_payload.extend_from_slice(&480_i32.to_le_bytes());
        strf_payload.extend_from_slice(&[1, 0, 24, 0]);
        strf_payload.extend_from_slice(b"ULRG");
        strf_payload.extend_from_slice(&[0; 36]);
        let strf = avi_chunk(b"strf", &strf_payload);

        let strl = avi_list(b"strl", [strh, strf].concat());
        let hdrl = avi_list(b"hdrl", strl);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(4 + hdrl.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"AVI ");
        bytes.extend_from_slice(&hdrl);

        let doc = parse_avi(&bytes).expect("avi");
        assert_eq!(doc.format, "avi");
        assert_eq!(doc.streams.len(), 1);
        let stream = &doc.streams[0];
        assert_eq!(stream.codec_name, "utvideo");
        assert_eq!(stream.width, Some(640));
        assert_eq!(stream.height, Some(480));
        assert!((stream.duration_seconds.unwrap() - 0.13346666666666668).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_duck_adpcm_audio_only_metadata() {
        let mut strh_payload = vec![0; 56];
        strh_payload[0..4].copy_from_slice(b"auds");
        strh_payload[4..8].copy_from_slice(&[1, 0, 0, 0]);
        strh_payload[20..24].copy_from_slice(&2048_u32.to_le_bytes());
        strh_payload[24..28].copy_from_slice(&44251_u32.to_le_bytes());
        strh_payload[32..36].copy_from_slice(&649_u32.to_le_bytes());
        let strh = avi_chunk(b"strh", &strh_payload);

        let mut strf_payload = Vec::new();
        strf_payload.extend_from_slice(&0x0061_u16.to_le_bytes());
        strf_payload.extend_from_slice(&2_u16.to_le_bytes());
        strf_payload.extend_from_slice(&44100_u32.to_le_bytes());
        strf_payload.extend_from_slice(&44251_u32.to_le_bytes());
        strf_payload.extend_from_slice(&2048_u16.to_le_bytes());
        strf_payload.extend_from_slice(&16_u16.to_le_bytes());
        strf_payload.extend_from_slice(&2_u16.to_le_bytes());
        strf_payload.extend_from_slice(&0x07f9_u16.to_le_bytes());
        let strf = avi_chunk(b"strf", &strf_payload);

        let strl = avi_list(b"strl", [strh, strf].concat());
        let hdrl = avi_list(b"hdrl", strl);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(4 + hdrl.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"AVI ");
        bytes.extend_from_slice(&hdrl);

        let doc = parse_avi(&bytes).expect("avi");
        assert_eq!(doc.streams.len(), 1);
        let stream = &doc.streams[0];
        assert_eq!(stream.codec_name, "adpcm_ima_dk4");
        assert_eq!(stream.sample_rate, Some(44100));
        assert_eq!(stream.channels, Some(2));
        assert_eq!(stream.bits_per_sample, Some(0));
        let expected = f64::from(649 + 8) * 2048.0 / 44251.0;
        assert!((stream.duration_seconds.unwrap() - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn maps_observed_avi_game_codec_tags() {
        assert_eq!(video_codec_name(*b"FPS1"), Some("fraps"));
        assert_eq!(video_codec_name(*b"LAGS"), Some("lagarith"));
        assert_eq!(video_codec_name(*b"CSCD"), Some("cscd"));
        assert_eq!(video_codec_name(*b"VP60"), Some("vp6"));
        assert_eq!(video_codec_name([1, 0, 0, 0]), Some("msrle"));
        assert_eq!(audio_codec_name(0x0001, Some(16)), Some("pcm_s16le"));
    }

    fn avi_list(kind: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"LIST");
        out.extend_from_slice(&(4 + payload.len() as u32).to_le_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(&payload);
        out
    }

    fn avi_chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
        if payload.len() % 2 != 0 {
            out.push(0);
        }
        out
    }
}
