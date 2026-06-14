use crate::{
    ac3::find_ac3_info, h264::parse_h264_annex_b, hevc::parse_hevc_annex_b,
    mp3::mpeg_audio_frame_stats, mpegvideo::parse_mpeg_video_payload, vvc::parse_vvc_annex_b,
};
use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

const TS_PACKET_SIZE: usize = 188;

pub fn looks_like_mpegts(bytes: &[u8]) -> bool {
    bytes.len() >= TS_PACKET_SIZE * 3
        && (0..3).all(|packet| bytes.get(packet * TS_PACKET_SIZE) == Some(&0x47))
}

pub fn parse_mpegts(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_mpegts(bytes) {
        return Err(RmpegError::InvalidData(
            "missing MPEG-TS packet sync".to_string(),
        ));
    }

    let streams = parse_program_streams(bytes)?;
    let mut payloads: Vec<TsPayload> = streams.iter().map(TsPayload::new).collect();
    collect_pes_payloads(bytes, &mut payloads)?;

    let mut output = Vec::new();
    for payload in &payloads {
        if let Some(stream) = payload.to_stream(output.len())? {
            output.push(stream);
        }
    }

    if output.is_empty()
        && !streams
            .iter()
            .all(|stream| matches!(stream.codec, Some(TsCodec::IgnoredData)))
    {
        return Err(RmpegError::InvalidData(
            "MPEG-TS file has no parseable supported streams".to_string(),
        ));
    }

    Ok(ProbeDocument {
        format: "mpegts".to_string(),
        streams: output,
    })
}

fn parse_program_streams(bytes: &[u8]) -> Result<Vec<TsStream>> {
    let mut pmt_pids = Vec::new();
    let mut streams: Vec<TsStream> = Vec::new();
    let mut pos = 0;
    while pos + TS_PACKET_SIZE <= bytes.len() {
        let packet = &bytes[pos..pos + TS_PACKET_SIZE];
        if packet[0] != 0x47 {
            return Err(RmpegError::InvalidData(
                "invalid MPEG-TS packet sync".to_string(),
            ));
        }
        let pid = packet_pid(packet);
        let payload_unit_start = packet[1] & 0x40 != 0;
        if payload_unit_start && pid == 0 {
            if let Some(section) = section_payload(packet)? {
                pmt_pids.extend(parse_pat(section));
            }
        } else if payload_unit_start && pmt_pids.contains(&pid) {
            if let Some(section) = section_payload(packet)? {
                streams = parse_pmt(section);
                if !streams.is_empty() {
                    break;
                }
            }
        }
        pos += TS_PACKET_SIZE;
    }
    if streams.is_empty() {
        return Err(RmpegError::InvalidData(
            "MPEG-TS file has no supported PMT".to_string(),
        ));
    }
    Ok(streams)
}

fn collect_pes_payloads(bytes: &[u8], payloads: &mut [TsPayload]) -> Result<()> {
    let mut pos = 0;
    while pos + TS_PACKET_SIZE <= bytes.len() {
        let packet = &bytes[pos..pos + TS_PACKET_SIZE];
        if packet[0] != 0x47 {
            return Err(RmpegError::InvalidData(
                "invalid MPEG-TS packet sync".to_string(),
            ));
        }
        let pid = packet_pid(packet);
        if let Some(payload) = payloads.iter_mut().find(|payload| payload.pid == pid) {
            if let Some(data) = packet_payload(packet)? {
                append_payload(payload, packet[1] & 0x40 != 0, data);
            }
        }
        pos += TS_PACKET_SIZE;
    }
    Ok(())
}

fn append_payload(payload: &mut TsPayload, payload_unit_start: bool, data: &[u8]) {
    if payload_unit_start && data.len() >= 9 && data.starts_with(&[0x00, 0x00, 0x01]) {
        if let Some(pts) = parse_pes_pts(data) {
            payload.pts.push(pts);
        }
        let payload_start = 9 + usize::from(data[8]);
        if payload_start <= data.len() {
            payload.data.extend_from_slice(&data[payload_start..]);
        }
    } else {
        payload.data.extend_from_slice(data);
    }
}

fn parse_pat(section: &[u8]) -> Vec<u16> {
    if section.len() < 12 || section[0] != 0 {
        return Vec::new();
    }
    let section_len = ((usize::from(section[1] & 0x0f)) << 8) | usize::from(section[2]);
    let end = (3 + section_len).saturating_sub(4).min(section.len());
    let mut out = Vec::new();
    let mut pos = 8;
    while pos + 4 <= end {
        let program = u16::from_be_bytes([section[pos], section[pos + 1]]);
        let pid = (u16::from(section[pos + 2] & 0x1f) << 8) | u16::from(section[pos + 3]);
        if program != 0 && !out.contains(&pid) {
            out.push(pid);
        }
        pos += 4;
    }
    out
}

fn parse_pmt(section: &[u8]) -> Vec<TsStream> {
    if section.len() < 16 || section[0] != 2 {
        return Vec::new();
    }
    let section_len = ((usize::from(section[1] & 0x0f)) << 8) | usize::from(section[2]);
    let end = (3 + section_len).saturating_sub(4).min(section.len());
    let program_info_len = ((usize::from(section[10] & 0x0f)) << 8) | usize::from(section[11]);
    let mut pos = 12 + program_info_len;
    let mut streams: Vec<TsStream> = Vec::new();
    while pos + 5 <= end {
        let stream_type = section[pos];
        let pid = (u16::from(section[pos + 1] & 0x1f) << 8) | u16::from(section[pos + 2]);
        let descriptor_len =
            ((usize::from(section[pos + 3] & 0x0f)) << 8) | usize::from(section[pos + 4]);
        let descriptor_end = (pos + 5 + descriptor_len).min(end);
        let descriptors = &section[pos + 5..descriptor_end];
        let codec = ts_codec(stream_type, descriptors);
        if let Some(existing) = streams.iter_mut().find(|stream| stream.pid == pid) {
            existing.codec = merge_codec(existing.codec, codec);
        } else {
            streams.push(TsStream { pid, codec });
        }
        pos = descriptor_end;
    }
    streams
}

fn ts_codec(stream_type: u8, descriptors: &[u8]) -> Option<TsCodec> {
    let registration = registration_descriptor(descriptors);
    match (stream_type, registration.as_deref()) {
        (0x02, _) => Some(TsCodec::Mpeg2Video),
        (0x03, _) => Some(TsCodec::MpegAudio("mp2")),
        (0x04, _) => Some(TsCodec::MpegAudio("mp3")),
        (0x06, _) if has_descriptor(descriptors, 0x6a) => Some(TsCodec::Ac3),
        (0x1b, _) | (_, Some("H264")) => Some(TsCodec::H264),
        (0x24, _) | (_, Some("HEVC")) | (_, Some("HDMV")) | (_, Some("DOVI")) => {
            Some(TsCodec::Hevc)
        }
        (0x33, _) | (_, Some("VVC-")) => Some(TsCodec::Vvc),
        (0x11, _) => Some(TsCodec::AacLatm),
        (0x06, Some("Opus")) => Some(TsCodec::Opus(opus_channels(descriptors).unwrap_or(2))),
        (0x15, _) => Some(TsCodec::IgnoredData),
        _ => None,
    }
}

fn merge_codec(existing: Option<TsCodec>, new: Option<TsCodec>) -> Option<TsCodec> {
    match (existing, new) {
        (_, Some(TsCodec::Ac3)) => Some(TsCodec::Ac3),
        (Some(codec), None) => Some(codec),
        (None, Some(codec)) => Some(codec),
        (Some(codec), Some(_)) => Some(codec),
        (None, None) => None,
    }
}

fn registration_descriptor(descriptors: &[u8]) -> Option<String> {
    let mut pos = 0;
    while pos + 2 <= descriptors.len() {
        let tag = descriptors[pos];
        let len = usize::from(descriptors[pos + 1]);
        let data_start = pos + 2;
        let data_end = data_start + len;
        if data_end > descriptors.len() {
            break;
        }
        if tag == 0x05 && len >= 4 {
            return Some(
                String::from_utf8_lossy(&descriptors[data_start..data_start + 4]).to_string(),
            );
        }
        pos = data_end;
    }
    None
}

fn opus_channels(descriptors: &[u8]) -> Option<u16> {
    let mut pos = 0;
    while pos + 2 <= descriptors.len() {
        let tag = descriptors[pos];
        let len = usize::from(descriptors[pos + 1]);
        let data_start = pos + 2;
        let data_end = data_start + len;
        if data_end > descriptors.len() {
            break;
        }
        if tag == 0x7f && len >= 2 && descriptors[data_start] == 0x80 {
            return Some(u16::from(descriptors[data_start + 1]));
        }
        pos = data_end;
    }
    None
}

fn has_descriptor(descriptors: &[u8], wanted: u8) -> bool {
    let mut pos = 0;
    while pos + 2 <= descriptors.len() {
        let tag = descriptors[pos];
        let len = usize::from(descriptors[pos + 1]);
        let data_end = pos + 2 + len;
        if data_end > descriptors.len() {
            break;
        }
        if tag == wanted {
            return true;
        }
        pos = data_end;
    }
    false
}

fn section_payload(packet: &[u8]) -> Result<Option<&[u8]>> {
    let Some(payload) = packet_payload(packet)? else {
        return Ok(None);
    };
    if payload.is_empty() {
        return Ok(None);
    }
    let pointer = usize::from(payload[0]);
    let start = 1 + pointer;
    if start >= payload.len() {
        return Ok(None);
    }
    Ok(Some(&payload[start..]))
}

fn packet_payload(packet: &[u8]) -> Result<Option<&[u8]>> {
    let adaptation_control = (packet[3] >> 4) & 0x03;
    let mut offset = 4;
    if matches!(adaptation_control, 2 | 3) {
        let length = usize::from(packet[offset]);
        offset = offset.checked_add(1 + length).ok_or_else(|| {
            RmpegError::InvalidData("MPEG-TS adaptation field overflow".to_string())
        })?;
    }
    if matches!(adaptation_control, 1 | 3) && offset < TS_PACKET_SIZE {
        Ok(Some(&packet[offset..TS_PACKET_SIZE]))
    } else {
        Ok(None)
    }
}

fn packet_pid(packet: &[u8]) -> u16 {
    (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2])
}

fn parse_pes_pts(data: &[u8]) -> Option<u64> {
    if data.len() < 14 || data[7] & 0x80 == 0 {
        return None;
    }
    Some(
        (u64::from((data[9] >> 1) & 0x07) << 30)
            | (u64::from(data[10]) << 22)
            | (u64::from((data[11] >> 1) & 0x7f) << 15)
            | (u64::from(data[12]) << 7)
            | u64::from((data[13] >> 1) & 0x7f),
    )
}

fn pts_duration_seconds(pts: &[u64]) -> f64 {
    if pts.len() < 2 {
        return 0.0;
    }
    let mut values = pts.to_vec();
    values.sort_unstable();
    values.dedup();
    if values.len() < 2 {
        return 0.0;
    }
    let first = values[0];
    let last = *values.last().expect("len checked");
    let frame_delta = values
        .windows(2)
        .filter_map(|pair| pair[1].checked_sub(pair[0]))
        .filter(|delta| *delta > 0)
        .min()
        .unwrap_or(0);
    last.saturating_sub(first).saturating_add(frame_delta) as f64 / 90_000.0
}

fn pts_span_seconds(pts: &[u64]) -> f64 {
    if pts.len() < 2 {
        return 0.0;
    }
    let mut values = pts.to_vec();
    values.sort_unstable();
    values.dedup();
    match (values.first(), values.last()) {
        (Some(first), Some(last)) if last >= first => (last - first) as f64 / 90_000.0,
        _ => 0.0,
    }
}

#[derive(Clone, Copy)]
struct TsStream {
    pid: u16,
    codec: Option<TsCodec>,
}

#[derive(Clone, Copy)]
enum TsCodec {
    H264,
    Hevc,
    Vvc,
    Mpeg2Video,
    Ac3,
    MpegAudio(&'static str),
    AacLatm,
    Opus(u16),
    IgnoredData,
}

struct TsPayload {
    pid: u16,
    codec: Option<TsCodec>,
    data: Vec<u8>,
    pts: Vec<u64>,
}

impl TsPayload {
    fn new(stream: &TsStream) -> Self {
        Self {
            pid: stream.pid,
            codec: stream.codec,
            data: Vec::new(),
            pts: Vec::new(),
        }
    }

    fn to_stream(&self, index: usize) -> Result<Option<StreamMetadata>> {
        let duration = pts_duration_seconds(&self.pts);
        match self.codec {
            Some(TsCodec::H264) => {
                let mut doc = parse_h264_annex_b(&self.data)?;
                let stream = doc.streams.remove(0);
                Ok(Some(StreamMetadata::video(
                    index,
                    "h264",
                    stream.width.unwrap_or(0),
                    stream.height.unwrap_or(0),
                    Some(duration),
                    None,
                )))
            }
            Some(TsCodec::Hevc) => {
                let mut doc = parse_hevc_annex_b(&self.data)?;
                let stream = doc.streams.remove(0);
                Ok(Some(StreamMetadata::video(
                    index,
                    "hevc",
                    stream.width.unwrap_or(0),
                    stream.height.unwrap_or(0),
                    Some(duration),
                    None,
                )))
            }
            Some(TsCodec::Vvc) => {
                let mut doc = parse_vvc_annex_b(&self.data)?;
                let stream = doc.streams.remove(0);
                Ok(Some(StreamMetadata::video(
                    index,
                    "vvc",
                    stream.width.unwrap_or(0),
                    stream.height.unwrap_or(0),
                    Some(duration),
                    None,
                )))
            }
            Some(TsCodec::Mpeg2Video) => {
                let mut doc = parse_mpeg_video_payload(
                    &self.data,
                    "mpegts",
                    Some(pts_span_seconds(&self.pts)),
                )?;
                let mut stream = doc.streams.remove(0);
                stream.index = index;
                Ok(Some(stream))
            }
            Some(TsCodec::MpegAudio(codec_hint)) => {
                let Some((frame, frames)) = mpeg_audio_frame_stats(&self.data) else {
                    let duration = pts_duration_seconds(&self.pts);
                    return Ok(Some(StreamMetadata::audio(
                        index, codec_hint, 0, 0, 0, duration,
                    )));
                };
                let frame_duration = frame.samples_per_frame as f64 / frame.sample_rate as f64;
                let pts_span = pts_span_seconds(&self.pts);
                let duration = if pts_span > 0.0 {
                    pts_span + frame_duration
                } else {
                    frames as f64 * frame_duration
                };
                Ok(Some(StreamMetadata::audio(
                    index,
                    frame.codec_name,
                    frame.sample_rate,
                    frame.channels,
                    0,
                    duration,
                )))
            }
            Some(TsCodec::Ac3) => {
                let duration = pts_duration_seconds(&self.pts);
                let Some(info) = find_ac3_info(&self.data) else {
                    return Ok(Some(StreamMetadata::audio(index, "ac3", 0, 0, 0, duration)));
                };
                let duration = pts_span_seconds(&self.pts) + 1536.0 / info.sample_rate as f64;
                Ok(Some(StreamMetadata::audio(
                    index,
                    info.codec_name(),
                    info.sample_rate,
                    info.channels,
                    0,
                    duration,
                )))
            }
            Some(TsCodec::AacLatm) => Ok(Some(StreamMetadata::audio(
                index,
                "aac_latm",
                48_000,
                2,
                0,
                pts_span_seconds(&self.pts) + 1024.0 / 48_000.0,
            ))),
            Some(TsCodec::Opus(channels)) => Ok(Some(StreamMetadata::audio(
                index, "opus", 48_000, channels, 0, duration,
            ))),
            Some(TsCodec::IgnoredData) => Ok(None),
            None => Ok(None),
        }
    }
}
