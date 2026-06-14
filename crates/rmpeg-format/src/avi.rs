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
}

pub fn parse_avi(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_avi(bytes) {
        return Err(RmpegError::InvalidData(
            "missing AVI RIFF header".to_string(),
        ));
    }

    let mut streams = Vec::new();
    parse_chunks(bytes, 12, bytes.len(), &mut streams)?;
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

pub fn looks_like_avi(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"AVI "
}

fn parse_chunks(
    bytes: &[u8],
    mut pos: usize,
    end: usize,
    streams: &mut Vec<StreamMetadata>,
) -> Result<()> {
    while pos + 8 <= end {
        let chunk = ChunkHeader::read(bytes, pos, end)?;
        if &chunk.id == b"LIST" && chunk.data_start + 4 <= chunk.end {
            let list_type = &bytes[chunk.data_start..chunk.data_start + 4];
            if list_type == b"strl" {
                if let Some(stream) =
                    parse_stream_list(bytes, chunk.data_start + 4, chunk.end, streams.len())?
                {
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

fn parse_stream_list(
    bytes: &[u8],
    mut pos: usize,
    end: usize,
    index: usize,
) -> Result<Option<StreamMetadata>> {
    let mut stream = AviStreamBuilder::default();
    while pos + 8 <= end {
        let chunk = ChunkHeader::read(bytes, pos, end)?;
        match &chunk.id {
            b"strh" => parse_stream_header(&bytes[chunk.data_start..chunk.end], &mut stream)?,
            b"strf" => parse_stream_format(&bytes[chunk.data_start..chunk.end], &mut stream)?,
            b"LIST" if chunk.data_start + 4 <= chunk.end => {
                if let Some(nested) =
                    parse_stream_list(bytes, chunk.data_start + 4, chunk.end, index)?
                {
                    return Ok(Some(nested));
                }
            }
            _ => {}
        }
        pos = chunk.padded_end();
    }
    Ok(stream.into_stream(index))
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

impl AviStreamBuilder {
    fn into_stream(self, index: usize) -> Option<StreamMetadata> {
        let stream_type = self.stream_type?;
        if &stream_type != b"vids" {
            return None;
        }
        let codec = video_codec_name(self.bitmap_codec.or(self.handler)?)?;
        let duration_seconds = match (self.length, self.scale, self.rate) {
            (Some(length), Some(scale), Some(rate)) if rate != 0 => {
                Some(length as f64 * scale as f64 / rate as f64)
            }
            _ => None,
        };
        Some(StreamMetadata::video(
            index,
            codec,
            self.width?,
            self.height?,
            duration_seconds,
            None,
        ))
    }
}

fn video_codec_name(fourcc: [u8; 4]) -> Option<&'static str> {
    match &fourcc {
        b"ULRG" | b"ULRA" | b"ULH0" | b"ULH2" | b"ULH4" | b"ULY0" | b"ULY2" => Some("utvideo"),
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
