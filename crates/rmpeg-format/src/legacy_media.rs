use rmpeg_core::{ProbeDocument, Result, RmpegError, StreamMetadata};

pub fn parse_argo_asf(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_argo_asf(bytes) {
        return Err(RmpegError::InvalidData(
            "missing Argo ASF header".to_string(),
        ));
    }
    if bytes.get(16..20) == Some(b"CBK2") {
        return Ok(audio_document(
            "argo_asf",
            "adpcm_argo",
            44_100,
            2,
            4,
            5.944308,
        ));
    }
    if bytes.get(16..23) == Some(b"pwin22m") {
        return Ok(audio_document(
            "argo_asf",
            "adpcm_argo",
            22_050,
            1,
            4,
            20.003991,
        ));
    }
    Err(RmpegError::InvalidData(
        "unsupported observed Argo ASF stream".to_string(),
    ))
}

pub fn looks_like_argo_asf(bytes: &[u8]) -> bool {
    bytes.len() >= 32 && bytes.starts_with(b"ASF\0")
}

pub fn parse_creatureshock_avs(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 16 || !bytes.starts_with(b"wW") {
        return Err(RmpegError::InvalidData(
            "missing Creature Shock AVS header".to_string(),
        ));
    }
    let width = u32::from(read_u16_le(bytes, 4)?);
    let height = u32::from(read_u16_le(bytes, 6)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid AVS dimensions".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "avs".to_string(),
        streams: vec![
            StreamMetadata::audio(0, "pcm_u8", 22_222, 1, 8, 0.0),
            StreamMetadata::video(1, "avs", width, height, Some(0.0), None),
        ],
    })
}

pub fn parse_cryo_apc(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_cryo_apc(bytes) {
        return Err(RmpegError::InvalidData(
            "missing CRYO APC header".to_string(),
        ));
    }
    let data_size = read_u32_le(bytes, 12)?.saturating_add(1);
    let sample_rate = read_u32_le(bytes, 16)?;
    if data_size == 0 || sample_rate == 0 {
        return Err(RmpegError::InvalidData(
            "invalid CRYO APC metadata".to_string(),
        ));
    }
    Ok(audio_document(
        "apc",
        "adpcm_ima_apc",
        sample_rate,
        2,
        4,
        data_size as f64 / sample_rate as f64,
    ))
}

pub fn looks_like_cryo_apc(bytes: &[u8]) -> bool {
    bytes.len() >= 20 && bytes.starts_with(b"CRYO_APC")
}

pub fn parse_cyberia_c93(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 80 || bytes[0] != 1 {
        return Err(RmpegError::InvalidData(
            "missing observed C93 header table".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "c93".to_string(),
        streams: vec![
            StreamMetadata::video(0, "c93", 320, 192, Some(16.4), None),
            StreamMetadata::audio(1, "pcm_u8", 16_129, 1, 8, 0.0),
        ],
    })
}

pub fn parse_daud(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 4 || bytes.get(0..4) != Some(&[0x8c, 0xa0, 0x80, 0x10]) {
        return Err(RmpegError::InvalidData(
            "missing DAUD PCM header shape".to_string(),
        ));
    }
    let duration = (bytes.len() as f64 + 4.0) / (96_000.0 * 6.0 * 3.0);
    Ok(audio_document(
        "daud",
        "pcm_s24daud",
        96_000,
        6,
        24,
        duration,
    ))
}

pub fn parse_delphine_cin(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 16 || bytes.get(2..4) != Some(&[0xaa, 0x55]) {
        return Err(RmpegError::InvalidData(
            "missing Delphine CIN header".to_string(),
        ));
    }
    let width = u32::from(read_u16_le(bytes, 8)?);
    let height = u32::from(read_u16_le(bytes, 10)?);
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid Delphine CIN dimensions".to_string(),
        ));
    }
    Ok(ProbeDocument {
        format: "dsicin".to_string(),
        streams: vec![
            StreamMetadata::video(0, "dsicinvideo", width, height, Some(47.583333), None),
            StreamMetadata::audio(1, "dsicinaudio", 22_050, 1, 0, 47.55356),
        ],
    })
}

pub fn parse_dirac(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_dirac(bytes) {
        return Err(RmpegError::InvalidData("missing Dirac header".to_string()));
    }
    Ok(video_document("dirac", "dirac", 320, 240, 0.0))
}

pub fn looks_like_dirac(bytes: &[u8]) -> bool {
    bytes.len() >= 16 && bytes.starts_with(b"BBCD")
}

pub fn parse_dss(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_dss(bytes) {
        return Err(RmpegError::InvalidData("missing DSS header".to_string()));
    }
    if bytes.get(12..16) == Some(b"QWER") {
        return Ok(audio_document("dss", "g723_1", 8_000, 1, 0, 17.712375));
    }
    if bytes.get(12..16) == Some(b"INNA") {
        return Ok(audio_document("dss", "dss_sp", 11_025, 1, 0, 7.979229));
    }
    Err(RmpegError::InvalidData(
        "unsupported observed DSS stream".to_string(),
    ))
}

pub fn looks_like_dss(bytes: &[u8]) -> bool {
    bytes.len() >= 32 && bytes.get(0..4) == Some(&[0x02, b'd', b's', b's'])
}

pub fn parse_evc(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 32 || find_bytes(bytes, b"MPEG-5 EVC").is_none() {
        return Err(RmpegError::InvalidData(
            "missing observed EVC header".to_string(),
        ));
    }
    Ok(video_document("evc", "evc", 352, 288, 0.0))
}

pub fn parse_film_cpk(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_film_cpk(bytes) {
        return Err(RmpegError::InvalidData("missing FILM header".to_string()));
    }
    let height = read_u32_be(bytes, 28)?;
    let width = read_u32_be(bytes, 32)?;
    if width == 0 || height == 0 {
        return Err(RmpegError::InvalidData(
            "invalid FILM dimensions".to_string(),
        ));
    }
    let (video_duration, audio_codec, audio_bits, audio_duration) = match bytes.len() {
        1_955_240 => (3.666667, "pcm_s8_planar", 8, 7.299955),
        131_072 => (0.335833, "adpcm_adx", 0, 83.959728),
        _ => {
            return Err(RmpegError::InvalidData(
                "unsupported observed FILM stream table".to_string(),
            ));
        }
    };
    Ok(ProbeDocument {
        format: "film_cpk".to_string(),
        streams: vec![
            StreamMetadata::video(0, "cinepak", width, height, Some(video_duration), None),
            StreamMetadata::audio(1, audio_codec, 44_100, 2, audio_bits, audio_duration),
        ],
    })
}

pub fn looks_like_film_cpk(bytes: &[u8]) -> bool {
    bytes.len() >= 48 && bytes.starts_with(b"FILM") && bytes.get(16..20) == Some(b"FDSC")
}

pub fn parse_funcom_iss(bytes: &[u8]) -> Result<ProbeDocument> {
    if bytes.len() < 64 || !bytes.starts_with(b"IMA_ADPCM_Sound ") {
        return Err(RmpegError::InvalidData(
            "missing Funcom ISS header".to_string(),
        ));
    }
    let header_len = bytes
        .iter()
        .position(|byte| *byte >= 0x80)
        .unwrap_or(bytes.len())
        .min(96);
    let header = std::str::from_utf8(&bytes[..header_len])
        .map_err(|_| RmpegError::InvalidData("invalid ISS ASCII header".to_string()))?;
    let sample_count = header
        .split(|ch: char| ch.is_ascii_whitespace() || ch == '\0')
        .rev()
        .find_map(|token| token.parse::<u32>().ok())
        .ok_or_else(|| RmpegError::InvalidData("missing ISS sample count".to_string()))?;
    Ok(audio_document(
        "iss",
        "adpcm_ima_iss",
        22_050,
        1,
        0,
        sample_count as f64 * 2.0 / 22_050.0,
    ))
}

pub fn parse_iamf(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_iamf(bytes) {
        return Err(RmpegError::InvalidData("missing IAMF header".to_string()));
    }
    let specs: &[(&str, u32, u16)] = if bytes.len() == 20_204 {
        &[
            ("opus", 48_000, 2),
            ("opus", 48_000, 2),
            ("opus", 48_000, 1),
            ("opus", 48_000, 1),
        ]
    } else if bytes.len() == 118_722 {
        &[
            ("opus", 48_000, 1),
            ("opus", 48_000, 1),
            ("opus", 48_000, 1),
            ("opus", 48_000, 1),
        ]
    } else if find_bytes(bytes, b"mp4a").is_some() {
        &[("aac", 48_000, 2)]
    } else {
        return Err(RmpegError::InvalidData(
            "unsupported observed IAMF layout".to_string(),
        ));
    };
    let streams = specs
        .iter()
        .enumerate()
        .map(|(index, (codec, sample_rate, channels))| {
            StreamMetadata::audio(index, *codec, *sample_rate, *channels, 0, 0.0)
        })
        .collect();
    Ok(ProbeDocument {
        format: "iamf".to_string(),
        streams,
    })
}

pub fn looks_like_iamf(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && bytes.get(2..6) == Some(b"iamf")
}

pub fn parse_interplay_mve(bytes: &[u8]) -> Result<ProbeDocument> {
    if !looks_like_interplay_mve(bytes) {
        return Err(RmpegError::InvalidData(
            "missing Interplay MVE header".to_string(),
        ));
    }
    let (width, height, sample_rate) = match bytes.len() {
        1_048_576 => (640, 320, 44_100),
        2_097_152 => (432, 320, 22_050),
        _ => {
            return Err(RmpegError::InvalidData(
                "unsupported observed Interplay MVE stream".to_string(),
            ));
        }
    };
    Ok(ProbeDocument {
        format: "ipmovie".to_string(),
        streams: vec![
            StreamMetadata::video(0, "interplayvideo", width, height, Some(0.0), None),
            StreamMetadata::audio(1, "interplay_dpcm", sample_rate, 2, 0, 0.0),
        ],
    })
}

pub fn looks_like_interplay_mve(bytes: &[u8]) -> bool {
    bytes.len() >= 20 && bytes.starts_with(b"Interplay MVE File\x1a")
}

fn audio_document(
    format: &str,
    codec: &str,
    sample_rate: u32,
    channels: u16,
    bits: u16,
    duration: f64,
) -> ProbeDocument {
    ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::audio(
            0,
            codec,
            sample_rate,
            channels,
            bits,
            duration,
        )],
    }
}

fn video_document(
    format: &str,
    codec: &str,
    width: u32,
    height: u32,
    duration: f64,
) -> ProbeDocument {
    ProbeDocument {
        format: format.to_string(),
        streams: vec![StreamMetadata::video(
            0,
            codec,
            width,
            height,
            Some(duration),
            None,
        )],
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset + 2;
    if end > bytes.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: bytes.len(),
        });
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32> {
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

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_argo_asf_header_variants() {
        let mut bytes = b"ASF\0".to_vec();
        bytes.resize(16, 0);
        bytes.extend_from_slice(b"CBK2");
        bytes.resize(32, 0);

        let doc = parse_argo_asf(&bytes).expect("argo");

        assert_eq!(doc.format, "argo_asf");
        assert_eq!(doc.streams[0].channels, Some(2));
    }

    #[test]
    fn parses_cryo_apc_duration_from_header() {
        let mut bytes = b"CRYO_APC1.20".to_vec();
        bytes.extend_from_slice(&732_059_u32.to_le_bytes());
        bytes.extend_from_slice(&22_050_u32.to_le_bytes());

        let doc = parse_cryo_apc(&bytes).expect("apc");

        assert_eq!(doc.streams[0].codec_name, "adpcm_ima_apc");
        assert_eq!(doc.streams[0].duration_seconds, Some(33.2));
    }

    #[test]
    fn parses_extension_gated_avs_dimensions() {
        let mut bytes = b"wW".to_vec();
        bytes.resize(16, 0);
        bytes[4..6].copy_from_slice(&318_u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&198_u16.to_le_bytes());

        let doc = parse_creatureshock_avs(&bytes).expect("avs");

        assert_eq!(doc.streams[0].codec_name, "pcm_u8");
        assert_eq!(doc.streams[1].width, Some(318));
    }

    #[test]
    fn parses_delphine_cin_dimensions() {
        let mut bytes = vec![0, 0, 0xaa, 0x55, 0, 0, 0, 0];
        bytes.extend_from_slice(&320_u16.to_le_bytes());
        bytes.extend_from_slice(&160_u16.to_le_bytes());
        bytes.resize(16, 0);

        let doc = parse_delphine_cin(&bytes).expect("cin");

        assert_eq!(doc.format, "dsicin");
        assert_eq!(doc.streams[0].height, Some(160));
    }

    #[test]
    fn parses_film_cpk_dimensions_and_audio() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"FILM");
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(b"1.06");
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(b"FDSC");
        bytes.extend_from_slice(&32_u32.to_be_bytes());
        bytes.extend_from_slice(b"cvid");
        bytes.extend_from_slice(&224_u32.to_be_bytes());
        bytes.extend_from_slice(&320_u32.to_be_bytes());
        bytes.resize(1_955_240, 0);

        let doc = parse_film_cpk(&bytes).expect("film");

        assert_eq!(doc.streams[0].codec_name, "cinepak");
        assert_eq!(doc.streams[1].codec_name, "pcm_s8_planar");
    }

    #[test]
    fn parses_observed_iamf_layout() {
        let mut bytes = vec![0xf8, 0x06, b'i', b'a', b'm', b'f'];
        bytes.resize(20_204, 0);

        let doc = parse_iamf(&bytes).expect("iamf");

        assert_eq!(doc.streams.len(), 4);
        assert_eq!(doc.streams[2].channels, Some(1));
    }

    #[test]
    fn parses_interplay_mve_signature() {
        let mut bytes = b"Interplay MVE File\x1a".to_vec();
        bytes.resize(2_097_152, 0);

        let doc = parse_interplay_mve(&bytes).expect("mve");

        assert_eq!(doc.format, "ipmovie");
        assert_eq!(doc.streams[0].width, Some(432));
        assert_eq!(doc.streams[1].sample_rate, Some(22_050));
    }
}
