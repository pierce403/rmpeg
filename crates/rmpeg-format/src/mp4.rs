use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy)]
struct BoxHeader {
    name: [u8; 4],
    data_start: usize,
    end: usize,
}

#[derive(Debug, Default)]
struct TrackBuilder {
    handler: Option<[u8; 4]>,
    timescale: Option<u32>,
    duration: Option<u64>,
    codec_name: Option<String>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    width: Option<u32>,
    height: Option<u32>,
}

pub fn parse_mp4(bytes: &[u8]) -> Result<ProbeDocument> {
    if !has_ftyp(bytes)? {
        return Err(RmpegError::InvalidData("missing MP4 ftyp box".to_string()));
    }

    let mut streams = Vec::new();
    let mut pos = 0;
    while let Some(header) = next_box(bytes, pos, bytes.len())? {
        if &header.name == b"moov" {
            streams = parse_moov(bytes, header.data_start, header.end)?;
        }
        pos = header.end;
    }

    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "MP4 moov box did not contain supported streams".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "mp4".to_string(),
        streams,
    })
}

fn has_ftyp(bytes: &[u8]) -> Result<bool> {
    if let Some(header) = next_box(bytes, 0, bytes.len())? {
        Ok(&header.name == b"ftyp")
    } else {
        Ok(false)
    }
}

fn parse_moov(bytes: &[u8], start: usize, end: usize) -> Result<Vec<StreamMetadata>> {
    let mut streams = Vec::new();
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"trak" {
            if let Some(stream) = parse_trak(bytes, header.data_start, header.end, streams.len())? {
                streams.push(stream);
            }
        }
        pos = header.end;
    }
    Ok(streams)
}

fn parse_trak(
    bytes: &[u8],
    start: usize,
    end: usize,
    index: usize,
) -> Result<Option<StreamMetadata>> {
    let mut track = TrackBuilder::default();
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"mdia" {
            parse_mdia(bytes, header.data_start, header.end, &mut track)?;
        }
        pos = header.end;
    }
    Ok(track.into_stream(index))
}

fn parse_mdia(bytes: &[u8], start: usize, end: usize, track: &mut TrackBuilder) -> Result<()> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        match &header.name {
            b"mdhd" => parse_mdhd(&bytes[header.data_start..header.end], track)?,
            b"hdlr" => parse_hdlr(&bytes[header.data_start..header.end], track)?,
            b"minf" => parse_minf(bytes, header.data_start, header.end, track)?,
            _ => {}
        }
        pos = header.end;
    }
    Ok(())
}

fn parse_minf(bytes: &[u8], start: usize, end: usize, track: &mut TrackBuilder) -> Result<()> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"stbl" {
            parse_stbl(bytes, header.data_start, header.end, track)?;
        }
        pos = header.end;
    }
    Ok(())
}

fn parse_stbl(bytes: &[u8], start: usize, end: usize, track: &mut TrackBuilder) -> Result<()> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"stsd" {
            parse_stsd(&bytes[header.data_start..header.end], track)?;
        }
        pos = header.end;
    }
    Ok(())
}

fn parse_mdhd(data: &[u8], track: &mut TrackBuilder) -> Result<()> {
    if data.len() < 20 {
        return Err(RmpegError::UnexpectedEof {
            needed: 20,
            remaining: data.len(),
        });
    }
    let version = data[0];
    match version {
        0 => {
            track.timescale = Some(read_u32(data, 12)?);
            track.duration = Some(u64::from(read_u32(data, 16)?));
        }
        1 => {
            if data.len() < 32 {
                return Err(RmpegError::UnexpectedEof {
                    needed: 32,
                    remaining: data.len(),
                });
            }
            track.timescale = Some(read_u32(data, 20)?);
            track.duration = Some(read_u64(data, 24)?);
        }
        _ => {
            return Err(RmpegError::InvalidData(format!(
                "unsupported mdhd version {version}"
            )));
        }
    }
    Ok(())
}

fn parse_hdlr(data: &[u8], track: &mut TrackBuilder) -> Result<()> {
    if data.len() < 12 {
        return Err(RmpegError::UnexpectedEof {
            needed: 12,
            remaining: data.len(),
        });
    }
    track.handler = Some([data[8], data[9], data[10], data[11]]);
    Ok(())
}

fn parse_stsd(data: &[u8], track: &mut TrackBuilder) -> Result<()> {
    if data.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: data.len(),
        });
    }
    let entry_count = read_u32(data, 4)?;
    if entry_count == 0 {
        return Ok(());
    }
    let entry_size = read_u32(data, 8)? as usize;
    if data.len() < 8 + entry_size || entry_size < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 8 + entry_size,
            remaining: data.len(),
        });
    }
    let coding = [data[12], data[13], data[14], data[15]];
    track.codec_name = Some(codec_name(coding).to_string());

    match track.handler.as_ref() {
        Some(b"soun") => parse_audio_sample_entry(data, 8, track)?,
        Some(b"vide") => parse_video_sample_entry(data, 8, track)?,
        _ => {}
    }
    Ok(())
}

fn parse_audio_sample_entry(data: &[u8], entry: usize, track: &mut TrackBuilder) -> Result<()> {
    if data.len() < entry + 36 {
        return Err(RmpegError::UnexpectedEof {
            needed: entry + 36,
            remaining: data.len(),
        });
    }
    track.channels = Some(read_u16(data, entry + 24)?);
    track.sample_rate = Some(read_u32(data, entry + 32)? >> 16);
    Ok(())
}

fn parse_video_sample_entry(data: &[u8], entry: usize, track: &mut TrackBuilder) -> Result<()> {
    if data.len() < entry + 36 {
        return Err(RmpegError::UnexpectedEof {
            needed: entry + 36,
            remaining: data.len(),
        });
    }
    track.width = Some(u32::from(read_u16(data, entry + 32)?));
    track.height = Some(u32::from(read_u16(data, entry + 34)?));
    Ok(())
}

fn next_box(bytes: &[u8], pos: usize, limit: usize) -> Result<Option<BoxHeader>> {
    if pos >= limit {
        return Ok(None);
    }
    if limit.saturating_sub(pos) < 8 {
        return Err(RmpegError::UnexpectedEof {
            needed: 8,
            remaining: limit.saturating_sub(pos),
        });
    }

    let size32 = read_u32(bytes, pos)?;
    let name = [
        bytes[pos + 4],
        bytes[pos + 5],
        bytes[pos + 6],
        bytes[pos + 7],
    ];
    let (size, header_len) = if size32 == 1 {
        if limit.saturating_sub(pos) < 16 {
            return Err(RmpegError::UnexpectedEof {
                needed: 16,
                remaining: limit.saturating_sub(pos),
            });
        }
        (read_u64(bytes, pos + 8)? as usize, 16)
    } else if size32 == 0 {
        (limit - pos, 8)
    } else {
        (size32 as usize, 8)
    };

    let end = pos
        .checked_add(size)
        .ok_or_else(|| RmpegError::InvalidData("MP4 box size overflow".to_string()))?;
    if size < header_len || end > limit {
        return Err(RmpegError::InvalidData(format!(
            "invalid MP4 box {} size {}",
            String::from_utf8_lossy(&name),
            size
        )));
    }

    Ok(Some(BoxHeader {
        name,
        data_start: pos + header_len,
        end,
    }))
}

impl TrackBuilder {
    fn into_stream(self, index: usize) -> Option<StreamMetadata> {
        let duration_seconds = match (self.duration, self.timescale) {
            (Some(duration), Some(timescale)) if timescale != 0 => {
                Some(duration as f64 / timescale as f64)
            }
            _ => None,
        };
        let handler = self.handler?;
        if &handler == b"soun" {
            Some(StreamMetadata::audio(
                index,
                self.codec_name?,
                self.sample_rate?,
                self.channels?,
                0,
                duration_seconds.unwrap_or(0.0),
            ))
        } else if &handler == b"vide" {
            Some(StreamMetadata::video(
                index,
                self.codec_name?,
                self.width?,
                self.height?,
                duration_seconds,
                None,
            ))
        } else {
            None
        }
    }
}

fn codec_name(coding: [u8; 4]) -> &'static str {
    match &coding {
        b"avc1" | b"avc3" => "h264",
        b"mp4a" => "aac",
        b"hvc1" | b"hev1" => "hevc",
        b"mp3 " => "mp3",
        _ => "unknown",
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 4;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64> {
    let end = offset + 8;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u64::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ]))
}
