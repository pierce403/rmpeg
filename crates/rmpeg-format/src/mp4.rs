use crate::aac::{parse_audio_specific_config, AacAudioConfig};
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
    bits_per_sample: Option<u16>,
    width: Option<u32>,
    height: Option<u32>,
}

pub fn parse_mp4(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_mp4(bytes) {
        return Err(RmpegError::InvalidData(
            "missing MP4/MOV top-level box".to_string(),
        ));
    }

    let mut streams = Vec::new();
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
            }
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

pub fn looks_like_mp4(bytes: &[u8]) -> bool {
    bytes.len() >= 8
        && matches!(
            &bytes[4..8],
            b"ftyp" | b"moov" | b"wide" | b"mdat" | b"free" | b"skip"
        )
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
        b"Hap1" | b"Hap5" | b"HapA" | b"HapM" | b"HapY" => "hap",
        b"MAC3" => "mace3",
        b"MAC6" => "mace6",
        b"QDM2" => "qdm2",
        b"avc1" | b"avc3" => "h264",
        b"ap4h" | b"ap4x" | b"apch" | b"apcn" | b"apco" | b"apcs" => "prores",
        b"dvc " | b"dvcp" | b"dvh5" | b"dvh6" | b"dvhp" | b"dvhq" | b"dvh1" => "dvvideo",
        b"mp4a" => "aac",
        b"mp4v" => "mpeg4",
        b"hvc1" | b"hev1" => "hevc",
        b"icod" => "aic",
        b"ima4" => "adpcm_ima_qt",
        b"in24" => "pcm_s24le",
        b"msVo" => "vorbis",
        b"mp3 " => "mp3",
        b"raw " => "pcm_u8",
        b"rle " => "qtrle",
        b"samr" => "amr_nb",
        b"sawb" => "amr_wb",
        b"sowt" => "pcm_s16le",
        b"twos" => "pcm_s16be",
        b"alaw" => "pcm_alaw",
        b"ulaw" => "pcm_mulaw",
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
    }
}
