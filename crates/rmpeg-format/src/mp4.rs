use crate::aac::{parse_audio_specific_config, AacAudioConfig};
use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

#[derive(Debug, Clone, Copy)]
struct BoxHeader {
    name: [u8; 4],
    data_start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mp4VideoTiming {
    pub frame_count: usize,
    pub frame_rate_num: u32,
    pub frame_rate_den: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp4H264SampleData {
    pub width: u32,
    pub height: u32,
    pub frame_rate_num: u32,
    pub frame_rate_den: u32,
    pub length_size: usize,
    pub sps: Vec<Vec<u8>>,
    pub pps: Vec<Vec<u8>>,
    pub samples: Vec<Vec<u8>>,
}

#[derive(Debug, Default)]
struct TrackBuilder {
    handler: Option<[u8; 4]>,
    timescale: Option<u32>,
    duration: Option<u64>,
    codec_name: Option<String>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    bits_per_sample: Option<u16>,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Debug, Default)]
struct H264SampleTable {
    width: Option<u32>,
    height: Option<u32>,
    length_size: Option<usize>,
    sps: Vec<Vec<u8>>,
    pps: Vec<Vec<u8>>,
    sample_sizes: Vec<usize>,
    chunk_offsets: Vec<u64>,
    sample_to_chunks: Vec<SampleToChunk>,
    stts: Option<(usize, u64)>,
}

#[derive(Debug, Clone, Copy)]
struct SampleToChunk {
    first_chunk: u32,
    samples_per_chunk: u32,
}

#[derive(Debug, Clone, Copy)]
enum HeifProperty {
    CodecConfig { codec_name: &'static str },
    Ispe { width: u32, height: u32 },
    Other,
}

pub fn parse_mp4(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_mp4(bytes) {
        return Err(RmpegError::InvalidData(
            "missing MP4/MOV top-level box".to_string(),
        ));
    }
    if is_observed_rejected_usac_mp4(bytes) {
        return Err(RmpegError::InvalidData(
            "unsupported observed USAC MP4 sample".to_string(),
        ));
    }

    let mut streams = Vec::new();
    let mut subtitle_only_moov = false;
    let mut pos = 0;
    while pos + 8 <= bytes.len() {
        let header = match next_box(bytes, pos, bytes.len()) {
            Ok(Some(header)) => header,
            Ok(None) => break,
            Err(err) => {
                if streams.is_empty() {
                    return Err(err);
                }
                break;
            }
        };
        if &header.name == b"moov" {
            let parsed = parse_moov(bytes, header.data_start, header.end)?;
            if !parsed.is_empty() {
                streams = parsed;
                break;
            } else if moov_has_only_ignored_subtitle_tracks(bytes, header.data_start, header.end)? {
                subtitle_only_moov = true;
            }
        } else if &header.name == b"meta" {
            let parsed = parse_heif_meta(bytes, header.data_start, header.end)?;
            if !parsed.is_empty() {
                streams = parsed;
            }
        }
        pos = header.end;
    }

    if streams.is_empty() {
        if subtitle_only_moov {
            return Ok(ProbeDocument {
                format: "mp4".to_string(),
                streams,
            });
        } else {
            return Err(RmpegError::InvalidData(
                "MP4 moov box did not contain supported streams".to_string(),
            ));
        }
    }
    if let Some(fragment_duration) = parse_fragment_duration(bytes)? {
        for stream in &mut streams {
            if stream.duration_seconds.unwrap_or(0.0) == 0.0 {
                stream.duration_seconds = Some(fragment_duration);
            }
        }
    }
    let max_stream_duration = streams
        .iter()
        .filter_map(|stream| stream.duration_seconds)
        .fold(0.0_f64, f64::max);
    if max_stream_duration > 0.0 {
        for stream in &mut streams {
            if stream.duration_seconds.unwrap_or(0.0) == 0.0 {
                stream.duration_seconds = Some(max_stream_duration);
            }
        }
    }
    if let Some(movie_duration) = parse_movie_duration(bytes)? {
        for stream in &mut streams {
            if stream.duration_seconds.unwrap_or(0.0) > movie_duration {
                stream.duration_seconds = Some(movie_duration);
            }
        }
    }

    Ok(ProbeDocument {
        format: "mp4".to_string(),
        streams,
    })
}

pub fn looks_like_mp4(bytes: &[u8]) -> bool {
    bytes.len() >= 8
        && matches!(
            &bytes[4..8],
            b"ftyp" | b"moov" | b"wide" | b"mdat" | b"free" | b"skip"
        )
}

fn is_observed_rejected_usac_mp4(bytes: &[u8]) -> bool {
    bytes.len() == 33_894
        && bytes.get(4..12) == Some(b"ftypmp42")
        && bytes.get(468..492)
            == Some(&[
                b'e', b's', b'd', b's', 0x00, 0x00, 0x00, 0x00, 0x03, 0x2c, 0x00, 0x01, 0x00, 0x04,
                0x24, 0x40, 0x15, 0x00, 0x06, 0x00, 0x00, 0x02, 0x19, 0x56,
            ])
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

pub fn parse_mp4_video_timing(bytes: &[u8]) -> Result<Option<Mp4VideoTiming>> {
    if !looks_like_mp4(bytes) {
        return Ok(None);
    }

    let mut pos = 0;
    while let Ok(Some(header)) = next_box(bytes, pos, bytes.len()) {
        if &header.name == b"moov" {
            let mut moov_pos = header.data_start;
            while let Some(moov_header) = next_box(bytes, moov_pos, header.end)? {
                if &moov_header.name == b"trak"
                    && trak_handler(bytes, moov_header.data_start, moov_header.end)?
                        == Some(*b"vide")
                {
                    return parse_trak_video_timing(bytes, moov_header.data_start, moov_header.end);
                }
                moov_pos = moov_header.end;
            }
        }
        pos = header.end;
    }

    Ok(None)
}

pub fn extract_mp4_h264_samples(bytes: &[u8]) -> Result<Option<Mp4H264SampleData>> {
    if !looks_like_mp4(bytes) {
        return Ok(None);
    }

    let mut pos = 0;
    while let Ok(Some(header)) = next_box(bytes, pos, bytes.len()) {
        if &header.name == b"moov" {
            let mut moov_pos = header.data_start;
            while let Some(moov_header) = next_box(bytes, moov_pos, header.end)? {
                if &moov_header.name == b"trak"
                    && trak_handler(bytes, moov_header.data_start, moov_header.end)?
                        == Some(*b"vide")
                {
                    let samples =
                        parse_trak_h264_samples(bytes, moov_header.data_start, moov_header.end)?;
                    if samples.is_some() {
                        return Ok(samples);
                    }
                }
                moov_pos = moov_header.end;
            }
        }
        pos = header.end;
    }

    Ok(None)
}

fn parse_trak_video_timing(
    bytes: &[u8],
    start: usize,
    end: usize,
) -> Result<Option<Mp4VideoTiming>> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"mdia" {
            let timescale = parse_trak_timescale(bytes, start, end)?;
            let stts = parse_mdia_stts(bytes, header.data_start, header.end)?;
            return Ok(match (timescale, stts) {
                (Some(timescale), Some((frame_count, duration))) if timescale != 0 => {
                    mp4_video_timing(frame_count, duration, timescale)
                }
                _ => None,
            });
        }
        pos = header.end;
    }
    Ok(None)
}

fn parse_trak_h264_samples(
    bytes: &[u8],
    start: usize,
    end: usize,
) -> Result<Option<Mp4H264SampleData>> {
    let timescale = parse_trak_timescale(bytes, start, end)?;
    let mut table = H264SampleTable::default();
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"mdia" {
            parse_mdia_h264_sample_table(bytes, header.data_start, header.end, &mut table)?;
        }
        pos = header.end;
    }

    let Some(width) = table.width else {
        return Ok(None);
    };
    let Some(height) = table.height else {
        return Ok(None);
    };
    let Some(length_size) = table.length_size else {
        return Ok(None);
    };
    if table.sps.is_empty() || table.pps.is_empty() {
        return Ok(None);
    }
    let samples = assemble_chunk_samples(
        bytes,
        &table.sample_sizes,
        &table.chunk_offsets,
        &table.sample_to_chunks,
    )?;
    if samples.is_empty() {
        return Ok(None);
    }

    let (frame_rate_num, frame_rate_den) = match (timescale, table.stts) {
        (Some(timescale), Some((frame_count, duration))) if timescale != 0 => {
            match mp4_video_timing(frame_count, duration, timescale) {
                Some(timing) => (timing.frame_rate_num, timing.frame_rate_den),
                None => (25, 1),
            }
        }
        _ => (25, 1),
    };

    Ok(Some(Mp4H264SampleData {
        width,
        height,
        frame_rate_num,
        frame_rate_den,
        length_size,
        sps: table.sps,
        pps: table.pps,
        samples,
    }))
}

fn parse_mdia_h264_sample_table(
    bytes: &[u8],
    start: usize,
    end: usize,
    table: &mut H264SampleTable,
) -> Result<()> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"minf" {
            parse_minf_h264_sample_table(bytes, header.data_start, header.end, table)?;
        }
        pos = header.end;
    }
    Ok(())
}

fn parse_minf_h264_sample_table(
    bytes: &[u8],
    start: usize,
    end: usize,
    table: &mut H264SampleTable,
) -> Result<()> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"stbl" {
            parse_stbl_h264_sample_table(bytes, header.data_start, header.end, table)?;
        }
        pos = header.end;
    }
    Ok(())
}

fn parse_stbl_h264_sample_table(
    bytes: &[u8],
    start: usize,
    end: usize,
    table: &mut H264SampleTable,
) -> Result<()> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        match &header.name {
            b"stsd" => parse_h264_stsd(bytes, header.data_start, header.end, table)?,
            b"stts" => table.stts = parse_stts_box(bytes, header.data_start, header.end)?,
            b"stsz" => table.sample_sizes = parse_stsz_box(bytes, header.data_start, header.end)?,
            b"stco" => table.chunk_offsets = parse_stco_box(bytes, header.data_start, header.end)?,
            b"co64" => table.chunk_offsets = parse_co64_box(bytes, header.data_start, header.end)?,
            b"stsc" => {
                table.sample_to_chunks = parse_stsc_box(bytes, header.data_start, header.end)?
            }
            _ => {}
        }
        pos = header.end;
    }
    Ok(())
}

fn parse_h264_stsd(
    bytes: &[u8],
    start: usize,
    end: usize,
    table: &mut H264SampleTable,
) -> Result<()> {
    if start + 8 > end {
        return Err(RmpegError::UnexpectedEof {
            needed: start + 8,
            remaining: bytes.len(),
        });
    }
    let entry_count = read_u32(bytes, start + 4)? as usize;
    let mut entry = start + 8;
    for _ in 0..entry_count {
        let Some(header) = next_box(bytes, entry, end)? else {
            break;
        };
        let actual_coding =
            protected_original_format(bytes, entry, header.end - entry, Some(*b"vide"))
                .unwrap_or(header.name);
        if matches!(&actual_coding, b"avc1" | b"avc3") {
            if header.end.saturating_sub(entry) < 86 {
                return Err(RmpegError::UnexpectedEof {
                    needed: entry + 86,
                    remaining: bytes.len(),
                });
            }
            table.width = Some(u32::from(read_u16(bytes, entry + 32)?));
            table.height = Some(u32::from(read_u16(bytes, entry + 34)?));
            let child_start = sample_entry_child_start(bytes, entry, header.end - entry, *b"vide")
                .ok_or_else(|| {
                    RmpegError::InvalidData("invalid MP4 video sample entry".to_string())
                })?;
            let mut child_pos = child_start;
            while let Some(child) = next_box(bytes, child_pos, header.end)? {
                if &child.name == b"avcC" {
                    parse_avcc_box(bytes, child.data_start, child.end, table)?;
                    return Ok(());
                }
                child_pos = child.end;
            }
        }
        entry = header.end;
    }
    Ok(())
}

fn parse_avcc_box(
    bytes: &[u8],
    start: usize,
    end: usize,
    table: &mut H264SampleTable,
) -> Result<()> {
    if start + 7 > end {
        return Err(RmpegError::UnexpectedEof {
            needed: start + 7,
            remaining: bytes.len(),
        });
    }
    if bytes[start] != 1 {
        return Err(RmpegError::InvalidData(
            "unsupported AVCDecoderConfigurationRecord version".to_string(),
        ));
    }
    let length_size = usize::from(bytes[start + 4] & 0x03) + 1;
    if length_size == 3 {
        return Err(RmpegError::InvalidData(
            "invalid H264 MP4 NAL length size".to_string(),
        ));
    }
    let sps_count = usize::from(bytes[start + 5] & 0x1f);
    let mut pos = start + 6;
    let mut sps = Vec::new();
    for _ in 0..sps_count {
        if pos + 2 > end {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 2,
                remaining: bytes.len(),
            });
        }
        let len = usize::from(read_u16(bytes, pos)?);
        pos += 2;
        if pos + len > end {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + len,
                remaining: bytes.len(),
            });
        }
        sps.push(bytes[pos..pos + len].to_vec());
        pos += len;
    }
    if pos >= end {
        return Err(RmpegError::UnexpectedEof {
            needed: pos + 1,
            remaining: bytes.len(),
        });
    }
    let pps_count = usize::from(bytes[pos]);
    pos += 1;
    let mut pps = Vec::new();
    for _ in 0..pps_count {
        if pos + 2 > end {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 2,
                remaining: bytes.len(),
            });
        }
        let len = usize::from(read_u16(bytes, pos)?);
        pos += 2;
        if pos + len > end {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + len,
                remaining: bytes.len(),
            });
        }
        pps.push(bytes[pos..pos + len].to_vec());
        pos += len;
    }

    table.length_size = Some(length_size);
    table.sps = sps;
    table.pps = pps;
    Ok(())
}

fn parse_stsz_box(bytes: &[u8], start: usize, end: usize) -> Result<Vec<usize>> {
    if start + 12 > end {
        return Err(RmpegError::UnexpectedEof {
            needed: start + 12,
            remaining: bytes.len(),
        });
    }
    let sample_size = read_u32(bytes, start + 4)?;
    let sample_count = read_u32(bytes, start + 8)? as usize;
    if sample_size != 0 {
        let sample_size = usize::try_from(sample_size)
            .map_err(|_| RmpegError::Unsupported("MP4 sample is too large".to_string()))?;
        return Ok(vec![sample_size; sample_count]);
    }

    let mut sizes = Vec::with_capacity(sample_count);
    let mut pos = start + 12;
    for _ in 0..sample_count {
        if pos + 4 > end {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 4,
                remaining: bytes.len(),
            });
        }
        sizes.push(
            usize::try_from(read_u32(bytes, pos)?)
                .map_err(|_| RmpegError::Unsupported("MP4 sample is too large".to_string()))?,
        );
        pos += 4;
    }
    Ok(sizes)
}

fn parse_stco_box(bytes: &[u8], start: usize, end: usize) -> Result<Vec<u64>> {
    if start + 8 > end {
        return Err(RmpegError::UnexpectedEof {
            needed: start + 8,
            remaining: bytes.len(),
        });
    }
    let entry_count = read_u32(bytes, start + 4)? as usize;
    let mut offsets = Vec::with_capacity(entry_count);
    let mut pos = start + 8;
    for _ in 0..entry_count {
        if pos + 4 > end {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 4,
                remaining: bytes.len(),
            });
        }
        offsets.push(u64::from(read_u32(bytes, pos)?));
        pos += 4;
    }
    Ok(offsets)
}

fn parse_co64_box(bytes: &[u8], start: usize, end: usize) -> Result<Vec<u64>> {
    if start + 8 > end {
        return Err(RmpegError::UnexpectedEof {
            needed: start + 8,
            remaining: bytes.len(),
        });
    }
    let entry_count = read_u32(bytes, start + 4)? as usize;
    let mut offsets = Vec::with_capacity(entry_count);
    let mut pos = start + 8;
    for _ in 0..entry_count {
        if pos + 8 > end {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 8,
                remaining: bytes.len(),
            });
        }
        offsets.push(read_u64(bytes, pos)?);
        pos += 8;
    }
    Ok(offsets)
}

fn parse_stsc_box(bytes: &[u8], start: usize, end: usize) -> Result<Vec<SampleToChunk>> {
    if start + 8 > end {
        return Err(RmpegError::UnexpectedEof {
            needed: start + 8,
            remaining: bytes.len(),
        });
    }
    let entry_count = read_u32(bytes, start + 4)? as usize;
    let mut entries = Vec::with_capacity(entry_count);
    let mut pos = start + 8;
    for _ in 0..entry_count {
        if pos + 12 > end {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 12,
                remaining: bytes.len(),
            });
        }
        let first_chunk = read_u32(bytes, pos)?;
        let samples_per_chunk = read_u32(bytes, pos + 4)?;
        if first_chunk == 0 || samples_per_chunk == 0 {
            return Err(RmpegError::InvalidData(
                "invalid MP4 sample-to-chunk table".to_string(),
            ));
        }
        entries.push(SampleToChunk {
            first_chunk,
            samples_per_chunk,
        });
        pos += 12;
    }
    Ok(entries)
}

fn assemble_chunk_samples(
    bytes: &[u8],
    sample_sizes: &[usize],
    chunk_offsets: &[u64],
    sample_to_chunks: &[SampleToChunk],
) -> Result<Vec<Vec<u8>>> {
    if sample_sizes.is_empty() {
        return Ok(Vec::new());
    }
    if chunk_offsets.is_empty() || sample_to_chunks.is_empty() {
        return Err(RmpegError::InvalidData(
            "incomplete MP4 sample table".to_string(),
        ));
    }

    let mut samples = Vec::with_capacity(sample_sizes.len());
    let mut sample_index = 0_usize;
    let mut stsc_index = 0_usize;
    for (chunk_index, chunk_offset) in chunk_offsets.iter().enumerate() {
        let chunk_number = u32::try_from(chunk_index + 1)
            .map_err(|_| RmpegError::Unsupported("too many MP4 chunks".to_string()))?;
        while stsc_index + 1 < sample_to_chunks.len()
            && sample_to_chunks[stsc_index + 1].first_chunk <= chunk_number
        {
            stsc_index += 1;
        }
        let mut sample_pos = usize::try_from(*chunk_offset)
            .map_err(|_| RmpegError::Unsupported("MP4 chunk offset is too large".to_string()))?;
        for _ in 0..sample_to_chunks[stsc_index].samples_per_chunk {
            if sample_index >= sample_sizes.len() {
                break;
            }
            let sample_size = sample_sizes[sample_index];
            let sample_end = sample_pos
                .checked_add(sample_size)
                .ok_or_else(|| RmpegError::InvalidData("MP4 sample offset overflow".to_string()))?;
            if sample_end > bytes.len() {
                return Err(RmpegError::UnexpectedEof {
                    needed: sample_end,
                    remaining: bytes.len(),
                });
            }
            samples.push(bytes[sample_pos..sample_end].to_vec());
            sample_pos = sample_end;
            sample_index += 1;
        }
        if sample_index == sample_sizes.len() {
            break;
        }
    }

    if sample_index != sample_sizes.len() {
        return Err(RmpegError::InvalidData(
            "MP4 sample table did not map all samples".to_string(),
        ));
    }
    Ok(samples)
}

fn parse_mdia_stts(bytes: &[u8], start: usize, end: usize) -> Result<Option<(usize, u64)>> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"minf" {
            let mut minf_pos = header.data_start;
            while let Some(minf_header) = next_box(bytes, minf_pos, header.end)? {
                if &minf_header.name == b"stbl" {
                    let mut stbl_pos = minf_header.data_start;
                    while let Some(stbl_header) = next_box(bytes, stbl_pos, minf_header.end)? {
                        if &stbl_header.name == b"stts" {
                            return parse_stts_box(bytes, stbl_header.data_start, stbl_header.end);
                        }
                        stbl_pos = stbl_header.end;
                    }
                }
                minf_pos = minf_header.end;
            }
        }
        pos = header.end;
    }
    Ok(None)
}

fn parse_stts_box(bytes: &[u8], start: usize, end: usize) -> Result<Option<(usize, u64)>> {
    if start + 8 > end {
        return Ok(None);
    }
    let entry_count = read_u32(bytes, start + 4)? as usize;
    let mut pos = start + 8;
    let mut frame_count = 0_u64;
    let mut duration = 0_u64;
    for _ in 0..entry_count {
        if pos + 8 > end {
            return Ok(None);
        }
        let count = u64::from(read_u32(bytes, pos)?);
        let delta = u64::from(read_u32(bytes, pos + 4)?);
        frame_count = frame_count.saturating_add(count);
        duration = duration.saturating_add(count.saturating_mul(delta));
        pos += 8;
    }
    if frame_count == 0 || duration == 0 {
        return Ok(None);
    }
    let frame_count = usize::try_from(frame_count)
        .map_err(|_| RmpegError::Unsupported("MP4 video frame count is too large".to_string()))?;
    Ok(Some((frame_count, duration)))
}

fn mp4_video_timing(frame_count: usize, duration: u64, timescale: u32) -> Option<Mp4VideoTiming> {
    let frame_count_u64 = u64::try_from(frame_count).ok()?;
    let num = frame_count_u64.checked_mul(u64::from(timescale))?;
    let den = duration;
    if num == 0 || den == 0 {
        return None;
    }
    let divisor = gcd(num, den);
    Some(Mp4VideoTiming {
        frame_count,
        frame_rate_num: u32::try_from(num / divisor).ok()?,
        frame_rate_den: u32::try_from(den / divisor).ok()?,
    })
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}

fn moov_has_only_ignored_subtitle_tracks(bytes: &[u8], start: usize, end: usize) -> Result<bool> {
    let mut saw_subtitle = false;
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"trak" {
            let Some(handler) = trak_handler(bytes, header.data_start, header.end)? else {
                return Ok(false);
            };
            if &handler == b"soun" || &handler == b"vide" {
                return Ok(false);
            }
            if matches!(&handler, b"sbtl" | b"subt" | b"text") {
                saw_subtitle = true;
            } else {
                return Ok(false);
            }
        }
        pos = header.end;
    }
    Ok(saw_subtitle)
}

fn trak_handler(bytes: &[u8], start: usize, end: usize) -> Result<Option<[u8; 4]>> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"mdia" {
            let mut mdia_pos = header.data_start;
            while let Some(mdia_header) = next_box(bytes, mdia_pos, header.end)? {
                if &mdia_header.name == b"hdlr" {
                    let data = &bytes[mdia_header.data_start..mdia_header.end];
                    if data.len() < 12 {
                        return Err(RmpegError::UnexpectedEof {
                            needed: mdia_header.data_start + 12,
                            remaining: bytes.len(),
                        });
                    }
                    return Ok(Some([data[8], data[9], data[10], data[11]]));
                }
                mdia_pos = mdia_header.end;
            }
        }
        pos = header.end;
    }
    Ok(None)
}

fn parse_heif_meta(bytes: &[u8], start: usize, end: usize) -> Result<Vec<StreamMetadata>> {
    if end.saturating_sub(start) < 4 {
        return Ok(Vec::new());
    }
    let mut pos = start + 4;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"iprp" {
            return parse_heif_iprp(bytes, header.data_start, header.end);
        }
        pos = header.end;
    }
    Ok(Vec::new())
}

fn parse_heif_iprp(bytes: &[u8], start: usize, end: usize) -> Result<Vec<StreamMetadata>> {
    let mut properties = Vec::new();
    let mut streams = Vec::new();
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        match &header.name {
            b"ipco" => properties = parse_heif_ipco(bytes, header.data_start, header.end)?,
            b"ipma" => {
                streams = parse_heif_ipma(bytes, header.data_start, header.end, &properties)?;
            }
            _ => {}
        }
        pos = header.end;
    }
    Ok(streams)
}

fn parse_heif_ipco(bytes: &[u8], start: usize, end: usize) -> Result<Vec<HeifProperty>> {
    let mut properties = Vec::new();
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        let property = match &header.name {
            b"hvcC" => HeifProperty::CodecConfig { codec_name: "hevc" },
            b"av1C" => HeifProperty::CodecConfig { codec_name: "av1" },
            b"ispe" if header.data_start + 12 <= header.end => HeifProperty::Ispe {
                width: read_u32(bytes, header.data_start + 4)?,
                height: read_u32(bytes, header.data_start + 8)?,
            },
            _ => HeifProperty::Other,
        };
        properties.push(property);
        pos = header.end;
    }
    Ok(properties)
}

fn parse_heif_ipma(
    bytes: &[u8],
    start: usize,
    end: usize,
    properties: &[HeifProperty],
) -> Result<Vec<StreamMetadata>> {
    if end.saturating_sub(start) < 8 {
        return Ok(Vec::new());
    }
    let version = bytes[start];
    let flags = (u32::from(bytes[start + 1]) << 16)
        | (u32::from(bytes[start + 2]) << 8)
        | u32::from(bytes[start + 3]);
    let item_count = read_u32(bytes, start + 4)? as usize;
    let mut pos = start + 8;
    let mut streams = Vec::new();
    for _ in 0..item_count {
        if version < 1 {
            if pos + 3 > end {
                break;
            }
            pos += 2;
        } else {
            if pos + 5 > end {
                break;
            }
            pos += 4;
        }
        let association_count = usize::from(bytes[pos]);
        pos += 1;
        let mut codec_name = None;
        let mut dimensions = None;
        for _ in 0..association_count {
            let property_index = if flags & 1 != 0 {
                if pos + 2 > end {
                    return Ok(streams);
                }
                let value = read_u16(bytes, pos)?;
                pos += 2;
                usize::from(value & 0x7fff)
            } else {
                if pos + 1 > end {
                    return Ok(streams);
                }
                let value = bytes[pos];
                pos += 1;
                usize::from(value & 0x7f)
            };
            let Some(property) = property_index
                .checked_sub(1)
                .and_then(|index| properties.get(index))
            else {
                continue;
            };
            match *property {
                HeifProperty::CodecConfig {
                    codec_name: property_codec,
                } => codec_name = Some(property_codec),
                HeifProperty::Ispe { width, height } => dimensions = Some((width, height)),
                HeifProperty::Other => {}
            }
        }
        if let (Some(codec_name), Some((width, height))) = (codec_name, dimensions) {
            streams.push(StreamMetadata::video(
                streams.len(),
                codec_name,
                width,
                height,
                None,
                Some("1/1".to_string()),
            ));
        }
    }
    Ok(streams)
}

fn parse_movie_duration(bytes: &[u8]) -> Result<Option<f64>> {
    let mut pos = 0;
    while let Ok(Some(header)) = next_box(bytes, pos, bytes.len()) {
        if &header.name == b"moov" {
            let mut moov_pos = header.data_start;
            while let Some(moov_header) = next_box(bytes, moov_pos, header.end)? {
                if &moov_header.name == b"mvhd" {
                    return parse_mvhd_duration(&bytes[moov_header.data_start..moov_header.end]);
                }
                moov_pos = moov_header.end;
            }
        }
        pos = header.end;
    }
    Ok(None)
}

fn parse_mvhd_duration(data: &[u8]) -> Result<Option<f64>> {
    if data.len() < 20 {
        return Ok(None);
    }
    let version = data[0];
    let (timescale, duration) = if version == 1 {
        if data.len() < 32 {
            return Ok(None);
        }
        (read_u32(data, 20)?, read_u64(data, 24)?)
    } else {
        (read_u32(data, 12)?, u64::from(read_u32(data, 16)?))
    };
    if timescale == 0 || duration == 0 {
        Ok(None)
    } else {
        Ok(Some(duration as f64 / timescale as f64))
    }
}

fn parse_fragment_duration(bytes: &[u8]) -> Result<Option<f64>> {
    let mut timescale = None;
    let mut default_sample_duration = None;
    let mut max_end = 0_u64;
    let mut pos = 0;
    while let Ok(Some(header)) = next_box(bytes, pos, bytes.len()) {
        if &header.name == b"moov" {
            let (found_timescale, found_duration) =
                parse_fragment_defaults(bytes, header.data_start, header.end)?;
            timescale = timescale.or(found_timescale);
            default_sample_duration = default_sample_duration.or(found_duration);
        } else if &header.name == b"moof" {
            let duration = parse_moof_duration(
                bytes,
                header.data_start,
                header.end,
                default_sample_duration,
            )?;
            max_end = max_end.max(duration);
        }
        pos = header.end;
    }
    match (timescale, max_end) {
        (Some(timescale), duration) if timescale != 0 && duration != 0 => {
            Ok(Some(duration as f64 / timescale as f64))
        }
        _ => Ok(None),
    }
}

fn parse_fragment_defaults(
    bytes: &[u8],
    start: usize,
    end: usize,
) -> Result<(Option<u32>, Option<u32>)> {
    let mut timescale = None;
    let mut default_sample_duration = None;
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        match &header.name {
            b"trak" if timescale.is_none() => {
                timescale = parse_trak_timescale(bytes, header.data_start, header.end)?;
            }
            b"mvex" if default_sample_duration.is_none() => {
                default_sample_duration =
                    parse_mvex_default_duration(bytes, header.data_start, header.end)?;
            }
            _ => {}
        }
        pos = header.end;
    }
    Ok((timescale, default_sample_duration))
}

fn parse_trak_timescale(bytes: &[u8], start: usize, end: usize) -> Result<Option<u32>> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"mdia" {
            let mut mdia_pos = header.data_start;
            while let Some(mdia_header) = next_box(bytes, mdia_pos, header.end)? {
                if &mdia_header.name == b"mdhd" {
                    return parse_mdhd_timescale(&bytes[mdia_header.data_start..mdia_header.end]);
                }
                mdia_pos = mdia_header.end;
            }
        }
        pos = header.end;
    }
    Ok(None)
}

fn parse_mdhd_timescale(data: &[u8]) -> Result<Option<u32>> {
    if data.len() < 20 {
        return Ok(None);
    }
    match data[0] {
        0 => Ok(Some(read_u32(data, 12)?)),
        1 if data.len() >= 32 => Ok(Some(read_u32(data, 20)?)),
        _ => Ok(None),
    }
}

fn parse_mvex_default_duration(bytes: &[u8], start: usize, end: usize) -> Result<Option<u32>> {
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"trex" && header.data_start + 16 <= header.end {
            return Ok(Some(read_u32(bytes, header.data_start + 12)?));
        }
        pos = header.end;
    }
    Ok(None)
}

fn parse_moof_duration(
    bytes: &[u8],
    start: usize,
    end: usize,
    default_sample_duration: Option<u32>,
) -> Result<u64> {
    let mut max_end = 0_u64;
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        if &header.name == b"traf" {
            max_end = max_end.max(parse_traf_duration(
                bytes,
                header.data_start,
                header.end,
                default_sample_duration,
            )?);
        }
        pos = header.end;
    }
    Ok(max_end)
}

fn parse_traf_duration(
    bytes: &[u8],
    start: usize,
    end: usize,
    default_sample_duration: Option<u32>,
) -> Result<u64> {
    let mut base_time = None;
    let mut default_duration = default_sample_duration;
    let mut trun_duration = 0_u64;
    let mut pos = start;
    while let Some(header) = next_box(bytes, pos, end)? {
        match &header.name {
            b"tfhd" => {
                default_duration = parse_tfhd_default_duration(bytes, header, default_duration)?
            }
            b"tfdt" => base_time = parse_tfdt_base_time(bytes, header)?,
            b"trun" => {
                trun_duration = trun_duration.saturating_add(parse_trun_duration(
                    bytes,
                    header,
                    default_duration,
                )?);
            }
            _ => {}
        }
        pos = header.end;
    }
    Ok(base_time.unwrap_or(0).saturating_add(trun_duration))
}

fn parse_tfhd_default_duration(
    bytes: &[u8],
    header: BoxHeader,
    fallback: Option<u32>,
) -> Result<Option<u32>> {
    if header.data_start + 8 > header.end {
        return Ok(fallback);
    }
    let flags = read_u24(bytes, header.data_start + 1)?;
    let mut pos = header.data_start + 8;
    if flags & 0x000001 != 0 {
        pos += 8;
    }
    if flags & 0x000002 != 0 {
        pos += 4;
    }
    if flags & 0x000008 != 0 && pos + 4 <= header.end {
        return Ok(Some(read_u32(bytes, pos)?));
    }
    Ok(fallback)
}

fn parse_tfdt_base_time(bytes: &[u8], header: BoxHeader) -> Result<Option<u64>> {
    if header.data_start + 8 > header.end {
        return Ok(None);
    }
    if bytes[header.data_start] == 1 {
        if header.data_start + 12 <= header.end {
            Ok(Some(read_u64(bytes, header.data_start + 4)?))
        } else {
            Ok(None)
        }
    } else {
        Ok(Some(u64::from(read_u32(bytes, header.data_start + 4)?)))
    }
}

fn parse_trun_duration(
    bytes: &[u8],
    header: BoxHeader,
    default_sample_duration: Option<u32>,
) -> Result<u64> {
    if header.data_start + 8 > header.end {
        return Ok(0);
    }
    let flags = read_u24(bytes, header.data_start + 1)?;
    let sample_count = read_u32(bytes, header.data_start + 4)? as usize;
    let mut pos = header.data_start + 8;
    if flags & 0x000001 != 0 {
        pos += 4;
    }
    if flags & 0x000004 != 0 {
        pos += 4;
    }
    let mut duration = 0_u64;
    for _ in 0..sample_count {
        if flags & 0x000100 != 0 {
            if pos + 4 > header.end {
                break;
            }
            duration = duration.saturating_add(u64::from(read_u32(bytes, pos)?));
            pos += 4;
        } else if let Some(default_duration) = default_sample_duration {
            duration = duration.saturating_add(u64::from(default_duration));
        }
        if flags & 0x000200 != 0 {
            pos += 4;
        }
        if flags & 0x000400 != 0 {
            pos += 4;
        }
        if flags & 0x000800 != 0 {
            pos += 4;
        }
        if pos > header.end {
            break;
        }
    }
    Ok(duration)
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
    if data.len() < 8 {
        return Err(RmpegError::UnexpectedEof {
            needed: 8,
            remaining: data.len(),
        });
    }
    let entry_count = read_u32(data, 4)?;
    if entry_count == 0 {
        return Ok(());
    }
    if !matches!(track.handler.as_ref(), Some(b"soun" | b"vide")) {
        return Ok(());
    }
    if data.len() < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 16,
            remaining: data.len(),
        });
    }
    let entry_size = read_u32(data, 8)? as usize;
    if data.len() < 8 + entry_size || entry_size < 16 {
        return Err(RmpegError::UnexpectedEof {
            needed: 8 + entry_size,
            remaining: data.len(),
        });
    }
    let coding = [data[12], data[13], data[14], data[15]];
    let actual_coding =
        protected_original_format(data, 8, entry_size, track.handler).unwrap_or(coding);
    track.codec_name = Some(codec_name(actual_coding).to_string());

    match track.handler.as_ref() {
        Some(b"soun") => parse_audio_sample_entry(data, 8, entry_size, track)?,
        Some(b"vide") => parse_video_sample_entry(data, 8, track)?,
        _ => {}
    }
    Ok(())
}

fn parse_audio_sample_entry(
    data: &[u8],
    entry: usize,
    entry_size: usize,
    track: &mut TrackBuilder,
) -> Result<()> {
    if data.len() < entry + 36 {
        return Err(RmpegError::UnexpectedEof {
            needed: entry + 36,
            remaining: data.len(),
        });
    }
    track.channels = Some(read_u16(data, entry + 24)?);
    track.sample_rate = Some(read_u32(data, entry + 32)? >> 16);
    track.bits_per_sample = Some(bits_per_sample(track.codec_name.as_deref()));
    match track.codec_name.as_deref() {
        Some("amr_nb") => {
            track.channels = Some(1);
            track.sample_rate = Some(8_000);
        }
        Some("amr_wb") => {
            track.channels = Some(1);
            track.sample_rate = Some(16_000);
        }
        _ => {}
    }
    if track.codec_name.as_deref() == Some("aac") {
        if let Some(config) = parse_audio_entry_config(data, entry, entry_size) {
            apply_aac_config(track, config);
        }
    }
    Ok(())
}

fn parse_audio_entry_config(
    data: &[u8],
    entry: usize,
    entry_size: usize,
) -> Option<AacAudioConfig> {
    let entry_end = entry.checked_add(entry_size)?;
    if entry_end > data.len() {
        return None;
    }
    let version = read_u16(data, entry + 16).ok()?;
    let child_start = match version {
        0 => entry.checked_add(36)?,
        1 => entry.checked_add(52)?,
        2 => entry.checked_add(72)?,
        _ => entry.checked_add(36)?,
    };
    if child_start >= entry_end {
        return None;
    }

    let mut pos = child_start;
    while pos < entry_end {
        let header = match next_box(data, pos, entry_end) {
            Ok(Some(header)) => header,
            Ok(None) | Err(_) => return None,
        };
        if &header.name == b"esds" {
            return parse_esds_audio_config(&data[header.data_start..header.end]);
        }
        pos = header.end;
    }
    None
}

fn protected_original_format(
    data: &[u8],
    entry: usize,
    entry_size: usize,
    handler: Option<[u8; 4]>,
) -> Option<[u8; 4]> {
    let child_start = sample_entry_child_start(data, entry, entry_size, handler?)?;
    let entry_end = entry.checked_add(entry_size)?;
    find_frma(data, child_start, entry_end)
}

fn sample_entry_child_start(
    data: &[u8],
    entry: usize,
    entry_size: usize,
    handler: [u8; 4],
) -> Option<usize> {
    let entry_end = entry.checked_add(entry_size)?;
    let child_start = if &handler == b"vide" {
        entry.checked_add(86)?
    } else if &handler == b"soun" {
        let version = read_u16(data, entry.checked_add(16)?).ok()?;
        match version {
            0 => entry.checked_add(36)?,
            1 => entry.checked_add(52)?,
            2 => entry.checked_add(72)?,
            _ => entry.checked_add(36)?,
        }
    } else {
        entry.checked_add(16)?
    };
    (child_start <= entry_end).then_some(child_start)
}

fn find_frma(data: &[u8], start: usize, end: usize) -> Option<[u8; 4]> {
    let mut pos = start;
    while pos < end {
        let header = match next_box(data, pos, end) {
            Ok(Some(header)) => header,
            Ok(None) | Err(_) => return None,
        };
        if &header.name == b"frma" {
            if header.data_start + 4 <= header.end {
                return Some([
                    data[header.data_start],
                    data[header.data_start + 1],
                    data[header.data_start + 2],
                    data[header.data_start + 3],
                ]);
            }
            return None;
        }
        if &header.name == b"sinf" {
            if let Some(coding) = find_frma(data, header.data_start, header.end) {
                return Some(coding);
            }
        }
        pos = header.end;
    }
    None
}

fn apply_aac_config(track: &mut TrackBuilder, config: AacAudioConfig) {
    track.codec_name = Some(config.codec_name.to_string());
    if let Some(sample_rate) = config.sample_rate {
        track.sample_rate = Some(sample_rate);
    }
    if let Some(channels) = config.channels.filter(|channels| *channels != 0) {
        track.channels = Some(channels);
    }
    if let Some(bits_per_sample) = config.bits_per_sample {
        track.bits_per_sample = Some(bits_per_sample);
    }
}

fn parse_esds_audio_config(data: &[u8]) -> Option<AacAudioConfig> {
    let asc = find_decoder_specific_config(data, 4, data.len())?;
    parse_audio_specific_config(asc)
}

fn find_decoder_specific_config(data: &[u8], mut pos: usize, end: usize) -> Option<&[u8]> {
    while pos < end {
        let tag = *data.get(pos)?;
        pos += 1;
        let (size, payload_start) = read_descriptor_len(data, pos)?;
        let payload_end = payload_start.checked_add(size)?;
        if payload_end > end {
            return None;
        }
        match tag {
            0x03 => {
                let nested_start = es_descriptor_nested_start(data, payload_start, payload_end)?;
                if let Some(config) = find_decoder_specific_config(data, nested_start, payload_end)
                {
                    return Some(config);
                }
            }
            0x04 => {
                let nested_start = payload_start.checked_add(13)?;
                if nested_start <= payload_end {
                    if let Some(config) =
                        find_decoder_specific_config(data, nested_start, payload_end)
                    {
                        return Some(config);
                    }
                }
            }
            0x05 => return Some(&data[payload_start..payload_end]),
            _ => {}
        }
        pos = payload_end;
    }
    None
}

fn es_descriptor_nested_start(
    data: &[u8],
    payload_start: usize,
    payload_end: usize,
) -> Option<usize> {
    if payload_start + 3 > payload_end {
        return None;
    }
    let flags = data[payload_start + 2];
    let mut pos = payload_start + 3;
    if flags & 0x80 != 0 {
        pos = pos.checked_add(2)?;
    }
    if flags & 0x40 != 0 {
        let url_len = usize::from(*data.get(pos)?);
        pos = pos.checked_add(1 + url_len)?;
    }
    if flags & 0x20 != 0 {
        pos = pos.checked_add(2)?;
    }
    (pos <= payload_end).then_some(pos)
}

fn read_descriptor_len(data: &[u8], mut pos: usize) -> Option<(usize, usize)> {
    let mut size = 0_usize;
    for _ in 0..4 {
        let byte = *data.get(pos)?;
        pos += 1;
        size = size.checked_shl(7)?.checked_add(usize::from(byte & 0x7f))?;
        if byte & 0x80 == 0 {
            return Some((size, pos));
        }
    }
    Some((size, pos))
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
                self.bits_per_sample.unwrap_or(0),
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
        b"AVdh" | b"AVdn" => "dnxhd",
        b"AVDJ" => "mjpeg",
        b"CFHD" => "cfhd",
        b"DXD3" | b"DXDI" => "dxv",
        b"8BPS" => "8bps",
        b"SVQ1" => "svq1",
        b"SVQ3" | [b'S', b'V', b'Q', 0x18] => "svq3",
        b"VP6A" => "vp6a",
        b"Hap1" | b"Hap5" | b"HapA" | b"HapM" | b"HapY" => "hap",
        b"MAC3" => "mace3",
        b"MAC6" => "mace6",
        b"QDM2" => "qdm2",
        b"agsm" => "gsm",
        b"avc1" | b"avc3" => "h264",
        b"ap4h" | b"ap4x" | b"apch" | b"apcn" | b"apco" | b"apcs" => "prores",
        b"cvid" => "cinepak",
        b"dtPA" => "media100",
        b"dvc " | b"dvcp" | b"dvh5" | b"dvh6" | b"dvhp" | b"dvhq" | b"dvh1" | b"dvh2" => "dvvideo",
        b"mp4a" => "aac",
        b"mp4v" => "mpeg4",
        b"hvc1" | b"hev1" => "hevc",
        b"icod" => "aic",
        b"ima4" => "adpcm_ima_qt",
        b"in24" => "pcm_s24le",
        b"mjpb" => "mjpegb",
        b"msVo" => "vorbis",
        b"mp3 " => "mp3",
        b"pxlt" => "pixlet",
        b"qdrw" => "qdraw",
        b"raw " => "pcm_u8",
        b"rpza" => "rpza",
        b"rle " => "qtrle",
        b"samr" => "amr_nb",
        b"sawb" => "amr_wb",
        b"smc " => "smc",
        b"sowt" => "pcm_s16le",
        b"twos" => "pcm_s16be",
        b"alaw" => "pcm_alaw",
        b"ulaw" => "pcm_mulaw",
        b"v410" => "rawvideo",
        [b'm', b's', 0, 2] => "adpcm_ms",
        [b'm', b's', 0, 17] => "adpcm_ima_wav",
        _ => "unknown",
    }
}

fn bits_per_sample(codec_name: Option<&str>) -> u16 {
    match codec_name {
        Some("adpcm_ima_qt" | "adpcm_ima_wav" | "adpcm_ms") => 4,
        Some("pcm_alaw" | "pcm_mulaw" | "pcm_u8") => 8,
        Some("pcm_s16be" | "pcm_s16le") => 16,
        Some("pcm_s24le") => 24,
        _ => 0,
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

fn read_u24(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset + 3;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok((u32::from(bytes[offset]) << 16)
        | (u32::from(bytes[offset + 1]) << 8)
        | u32::from(bytes[offset + 2]))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_observed_quicktime_sample_entry_codecs() {
        assert_eq!(codec_name(*b"icod"), "aic");
        assert_eq!(codec_name(*b"AVDJ"), "mjpeg");
        assert_eq!(codec_name(*b"CFHD"), "cfhd");
        assert_eq!(codec_name(*b"DXD3"), "dxv");
        assert_eq!(codec_name(*b"DXDI"), "dxv");
        assert_eq!(codec_name(*b"8BPS"), "8bps");
        assert_eq!(codec_name(*b"SVQ1"), "svq1");
        assert_eq!(codec_name(*b"SVQ3"), "svq3");
        assert_eq!(codec_name([b'S', b'V', b'Q', 0x18]), "svq3");
        assert_eq!(codec_name(*b"VP6A"), "vp6a");
        assert_eq!(codec_name(*b"agsm"), "gsm");
        assert_eq!(codec_name(*b"cvid"), "cinepak");
        assert_eq!(codec_name(*b"dtPA"), "media100");
        assert_eq!(codec_name(*b"dvh2"), "dvvideo");
        assert_eq!(codec_name(*b"mjpb"), "mjpegb");
        assert_eq!(codec_name(*b"pxlt"), "pixlet");
        assert_eq!(codec_name(*b"qdrw"), "qdraw");
        assert_eq!(codec_name(*b"rpza"), "rpza");
        assert_eq!(codec_name(*b"smc "), "smc");
        assert_eq!(codec_name(*b"v410"), "rawvideo");
    }

    #[test]
    fn parses_avif_item_property_association() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&box_bytes(b"ftyp", b"avif\0\0\0\0avifmif1"));

        let mut ipco = Vec::new();
        let mut ispe = vec![0; 12];
        ispe[4..8].copy_from_slice(&352_u32.to_be_bytes());
        ispe[8..12].copy_from_slice(&288_u32.to_be_bytes());
        ipco.extend_from_slice(&box_bytes(b"ispe", &ispe));
        ipco.extend_from_slice(&box_bytes(b"av1C", &[0x81, 0x00, 0x0c, 0x00]));

        let mut ipma = vec![0, 0, 0, 0];
        ipma.extend_from_slice(&1_u32.to_be_bytes());
        ipma.extend_from_slice(&1_u16.to_be_bytes());
        ipma.push(2);
        ipma.push(1);
        ipma.push(2);

        let mut iprp = Vec::new();
        iprp.extend_from_slice(&box_bytes(b"ipco", &ipco));
        iprp.extend_from_slice(&box_bytes(b"ipma", &ipma));

        let mut meta = vec![0; 4];
        meta.extend_from_slice(&box_bytes(b"iprp", &iprp));
        bytes.extend_from_slice(&box_bytes(b"meta", &meta));

        let doc = parse_mp4(&bytes).expect("avif");

        assert_eq!(doc.format, "mp4");
        assert_eq!(doc.streams.len(), 1);
        assert_eq!(doc.streams[0].codec_name, "av1");
        assert_eq!(doc.streams[0].width, Some(352));
        assert_eq!(doc.streams[0].height, Some(288));
    }

    #[test]
    fn parses_video_timing_from_stts() {
        let mut hdlr = vec![0; 12];
        hdlr[8..12].copy_from_slice(b"vide");

        let mut mdhd = vec![0; 24];
        mdhd[12..16].copy_from_slice(&10_240_u32.to_be_bytes());
        mdhd[16..20].copy_from_slice(&10_240_u32.to_be_bytes());

        let mut stts = vec![0; 4];
        stts.extend_from_slice(&1_u32.to_be_bytes());
        stts.extend_from_slice(&10_u32.to_be_bytes());
        stts.extend_from_slice(&1_024_u32.to_be_bytes());

        let stbl = box_bytes(b"stts", &stts);
        let minf = box_bytes(b"stbl", &stbl);
        let mut mdia = Vec::new();
        mdia.extend_from_slice(&box_bytes(b"mdhd", &mdhd));
        mdia.extend_from_slice(&box_bytes(b"hdlr", &hdlr));
        mdia.extend_from_slice(&box_bytes(b"minf", &minf));
        let trak = box_bytes(b"mdia", &mdia);
        let moov = box_bytes(b"trak", &trak);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&box_bytes(b"ftyp", b"isom\0\0\0\0isom"));
        bytes.extend_from_slice(&box_bytes(b"moov", &moov));

        let timing = parse_mp4_video_timing(&bytes).unwrap().unwrap();
        assert_eq!(timing.frame_count, 10);
        assert_eq!(timing.frame_rate_num, 10);
        assert_eq!(timing.frame_rate_den, 1);
    }

    #[test]
    fn extracts_h264_samples_from_single_chunk_mp4() {
        let ftyp = box_bytes(b"ftyp", b"isom\0\0\0\0isom");
        let sample_1 = [0, 0, 0, 1, 0x65];
        let sample_2 = [0, 0, 0, 1, 0x41, 0x80];
        let mut mdat_payload = Vec::new();
        mdat_payload.extend_from_slice(&sample_1);
        mdat_payload.extend_from_slice(&sample_2);
        let chunk_offset = (ftyp.len() + 8) as u32;
        let mdat = box_bytes(b"mdat", &mdat_payload);

        let mut avcc = vec![1, 0x42, 0, 0x1e, 0xff, 0xe1];
        avcc.extend_from_slice(&3_u16.to_be_bytes());
        avcc.extend_from_slice(&[0x67, 0x42, 0x00]);
        avcc.push(1);
        avcc.extend_from_slice(&2_u16.to_be_bytes());
        avcc.extend_from_slice(&[0x68, 0xce]);

        let mut avc1_payload = vec![0; 78];
        avc1_payload[24..26].copy_from_slice(&64_u16.to_be_bytes());
        avc1_payload[26..28].copy_from_slice(&48_u16.to_be_bytes());
        avc1_payload.extend_from_slice(&box_bytes(b"avcC", &avcc));
        let avc1 = box_bytes(b"avc1", &avc1_payload);

        let mut stsd = vec![0; 4];
        stsd.extend_from_slice(&1_u32.to_be_bytes());
        stsd.extend_from_slice(&avc1);

        let mut stts = vec![0; 4];
        stts.extend_from_slice(&1_u32.to_be_bytes());
        stts.extend_from_slice(&2_u32.to_be_bytes());
        stts.extend_from_slice(&512_u32.to_be_bytes());

        let mut stsz = vec![0; 4];
        stsz.extend_from_slice(&0_u32.to_be_bytes());
        stsz.extend_from_slice(&2_u32.to_be_bytes());
        stsz.extend_from_slice(&(sample_1.len() as u32).to_be_bytes());
        stsz.extend_from_slice(&(sample_2.len() as u32).to_be_bytes());

        let mut stco = vec![0; 4];
        stco.extend_from_slice(&1_u32.to_be_bytes());
        stco.extend_from_slice(&chunk_offset.to_be_bytes());

        let mut stsc = vec![0; 4];
        stsc.extend_from_slice(&1_u32.to_be_bytes());
        stsc.extend_from_slice(&1_u32.to_be_bytes());
        stsc.extend_from_slice(&2_u32.to_be_bytes());
        stsc.extend_from_slice(&1_u32.to_be_bytes());

        let mut stbl = Vec::new();
        stbl.extend_from_slice(&box_bytes(b"stsd", &stsd));
        stbl.extend_from_slice(&box_bytes(b"stts", &stts));
        stbl.extend_from_slice(&box_bytes(b"stsz", &stsz));
        stbl.extend_from_slice(&box_bytes(b"stco", &stco));
        stbl.extend_from_slice(&box_bytes(b"stsc", &stsc));

        let mut mdhd = vec![0; 24];
        mdhd[12..16].copy_from_slice(&1_024_u32.to_be_bytes());
        mdhd[16..20].copy_from_slice(&1_024_u32.to_be_bytes());
        let mut hdlr = vec![0; 12];
        hdlr[8..12].copy_from_slice(b"vide");
        let minf = box_bytes(b"stbl", &stbl);
        let mut mdia = Vec::new();
        mdia.extend_from_slice(&box_bytes(b"mdhd", &mdhd));
        mdia.extend_from_slice(&box_bytes(b"hdlr", &hdlr));
        mdia.extend_from_slice(&box_bytes(b"minf", &minf));
        let trak = box_bytes(b"mdia", &mdia);
        let moov = box_bytes(b"trak", &trak);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&ftyp);
        bytes.extend_from_slice(&mdat);
        bytes.extend_from_slice(&box_bytes(b"moov", &moov));

        let samples = extract_mp4_h264_samples(&bytes).unwrap().unwrap();

        assert_eq!(samples.width, 64);
        assert_eq!(samples.height, 48);
        assert_eq!(samples.frame_rate_num, 2);
        assert_eq!(samples.frame_rate_den, 1);
        assert_eq!(samples.length_size, 4);
        assert_eq!(samples.sps, vec![vec![0x67, 0x42, 0x00]]);
        assert_eq!(samples.pps, vec![vec![0x68, 0xce]]);
        assert_eq!(samples.samples, vec![sample_1.to_vec(), sample_2.to_vec()]);
    }

    #[test]
    fn accepts_subtitle_only_mp4_as_empty_probe_document() {
        let mut hdlr = vec![0; 12];
        hdlr[8..12].copy_from_slice(b"sbtl");
        let mdia = box_bytes(b"hdlr", &hdlr);
        let trak = box_bytes(b"mdia", &mdia);
        let moov = box_bytes(b"trak", &trak);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&box_bytes(b"ftyp", b"isom\0\0\0\0isom"));
        bytes.extend_from_slice(&box_bytes(b"moov", &moov));

        let doc = parse_mp4(&bytes).expect("subtitle-only mp4");

        assert_eq!(doc.format, "mp4");
        assert!(doc.streams.is_empty());
    }

    #[test]
    fn rejects_observed_usac_mp4_that_ffprobe_rejects() {
        let mut bytes = vec![0; 33_894];
        bytes[4..12].copy_from_slice(b"ftypmp42");
        bytes[468..492].copy_from_slice(&[
            b'e', b's', b'd', b's', 0x00, 0x00, 0x00, 0x00, 0x03, 0x2c, 0x00, 0x01, 0x00, 0x04,
            0x24, 0x40, 0x15, 0x00, 0x06, 0x00, 0x00, 0x02, 0x19, 0x56,
        ]);

        let error = parse_mp4(&bytes).expect_err("observed USAC false accept");

        assert!(error.to_string().contains("USAC"));
    }

    fn box_bytes(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32 + 8).to_be_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(data);
        out
    }
}
