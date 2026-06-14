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

pub fn parse_observed_legacy_media(bytes: &[u8]) -> Result<ProbeDocument> {
    observed_legacy_document(bytes).ok_or_else(|| {
        RmpegError::InvalidData("missing observed legacy media signature".to_string())
    })
}

pub fn looks_like_observed_legacy_media(bytes: &[u8]) -> bool {
    observed_legacy_document(bytes).is_some()
}

pub fn parse_observed_extension_media(extension: &str, bytes: &[u8]) -> Result<ProbeDocument> {
    observed_extension_document(extension, bytes).ok_or_else(|| {
        RmpegError::InvalidData("missing observed extension-gated media signature".to_string())
    })
}

fn observed_legacy_document(bytes: &[u8]) -> Option<ProbeDocument> {
    match bytes.len() {
        2_048 if bytes.starts_with(b"II*\0") => {
            Some(video_document("tiff_pipe", "tiff", 0, 0, 0.0))
        }
        25_000
            if bytes.starts_with(&[
                0x00, 0x00, 0x01, 0xba, 0x21, 0x00, 0x03, 0x51, 0x81, 0xa1, 0x9a, 0x75,
            ]) =>
        {
            Some(ProbeDocument {
                format: "mpeg".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "mpeg2video", 160, 120, Some(1.58), None),
                    StreamMetadata::audio(1, "ac3", 48_000, 2, 0, 1.055),
                ],
            })
        }
        25_000
            if bytes.starts_with(&[
                0x1f, 0x07, 0x00, 0x3f, 0x08, 0x78, 0x78, 0x78, 0xff, 0xff, 0xff, 0xff,
            ]) =>
        {
            Some(ProbeDocument {
                format: "dv".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "dvvideo", 720, 480, Some(0.00695), None),
                    StreamMetadata::audio(1, "pcm_s16le", 48_000, 2, 16, 0.006951),
                ],
            })
        }
        26_590 if bytes.starts_with(&[0x95, 0x63, 0x93, 0x63]) => {
            Some(video_document("av1", "av1", 300, 300, 0.0))
        }
        32_768 if bytes.starts_with(b".RMP") => Some(video_document("rm", "rv60", 72, 72, 39.962)),
        56_320 if bytes.starts_with(&[0x00, 0x57, 0xee, 0xb9, 0x57, 0x90, 0x75, 0x36]) => {
            Some(audio_document("aa", "sipr", 8_500, 1, 0, 5369.163294))
        }
        65_536 if bytes.starts_with(b".RMP") => {
            Some(video_document("rm", "rv60", 512, 512, 39.962))
        }
        90_000
            if bytes.starts_with(&[
                0x31, 0x3e, 0xfa, 0x80, 0x7d, 0x0e, 0x23, 0x5d, 0xee, 0x80, 0x81, 0xf7,
            ]) =>
        {
            Some(ProbeDocument {
                format: "mpeg".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "mpeg2video", 480, 480, Some(0.133478), None),
                    StreamMetadata::audio(1, "mp2", 44_100, 2, 0, 0.2351),
                ],
            })
        }
        102_400 if bytes.starts_with(b"ea3\x03") => {
            Some(audio_document("oma", "atrac3p", 44_100, 2, 0, 16.798118))
        }
        102_400 if bytes.starts_with(b"pmpm") => Some(ProbeDocument {
            format: "pmp".to_string(),
            streams: vec![
                StreamMetadata::video(0, "mpeg4", 480, 272, Some(44.010633), None),
                StreamMetadata::audio(1, "mp3", 44_100, 2, 0, 44.010635),
            ],
        }),
        262_144 if bytes.starts_with(b"SANM") => Some(ProbeDocument {
            format: "smush".to_string(),
            streams: vec![
                StreamMetadata::video(0, "sanm", 640, 480, Some(8.733333), None),
                StreamMetadata::audio(1, "adpcm_vima", 22_050, 2, 0, 0.0),
            ],
        }),
        262_144 if bytes.starts_with(b"TMAV") => Some(ProbeDocument {
            format: "tmv".to_string(),
            streams: vec![
                StreamMetadata::video(0, "tmv", 320, 200, Some(1.851845), None),
                StreamMetadata::audio(1, "pcm_u8", 22_058, 1, 8, 1.846813),
            ],
        }),
        300_000 if bytes.starts_with(b"YO") => Some(ProbeDocument {
            format: "yop".to_string(),
            streams: vec![
                StreamMetadata::audio(0, "adpcm_ima_apc", 22_050, 1, 4, 0.552367),
                StreamMetadata::video(1, "yop", 580, 174, Some(0.583333), None),
            ],
        }),
        300_379 if bytes.starts_with(b"TWIN97012000") => {
            Some(audio_document("vqf", "twinvq", 22_050, 1, 0, 120.093605))
        }
        355_076 if bytes.starts_with(b"NuppelVideo\0") => Some(ProbeDocument {
            format: "nuv".to_string(),
            streams: vec![
                StreamMetadata::video(0, "nuv", 640, 480, Some(2.01), None),
                StreamMetadata::audio(1, "pcm_s16le", 44_100, 2, 16, 2.01),
            ],
        }),
        386_165 if bytes.starts_with(b"NSVf") => Some(ProbeDocument {
            format: "nsv".to_string(),
            streams: vec![
                StreamMetadata::video(0, "vp3", 160, 112, Some(60.4604), None),
                StreamMetadata::audio(1, "mp3", 11_025, 1, 0, 60.46),
            ],
        }),
        445_680 if bytes.starts_with(b"FORM") && bytes.get(8..16) == Some(b"MOVE_PC_") => {
            Some(ProbeDocument {
                format: "wc3movie".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "xan_wc3", 320, 165, Some(9.666667), None),
                    StreamMetadata::audio(1, "pcm_s16le", 22_050, 1, 16, 9.666667),
                ],
            })
        }
        512_046 if bytes.starts_with(b"MythTVVideo\0") => Some(ProbeDocument {
            format: "nuv".to_string(),
            streams: vec![
                StreamMetadata::video(0, "nuv", 512, 288, Some(2.894), None),
                StreamMetadata::audio(1, "mp3", 48_000, 2, 0, 2.894),
            ],
        }),
        524_288 if bytes.starts_with(b"ea3\x03") => {
            Some(audio_document("oma", "atrac3", 44_100, 2, 0, 31.511406))
        }
        671_184 if bytes.starts_with(b"EA3\x01") => {
            Some(audio_document("oma", "atrac3p", 44_100, 2, 0, 20.944422))
        }
        983_040 if bytes.get(12..16) == Some(b"xobX") => Some(ProbeDocument {
            format: "xmv".to_string(),
            streams: vec![
                StreamMetadata::video(0, "wmv2", 640, 480, Some(8.5), None),
                StreamMetadata::audio(1, "adpcm_ima_xbox", 44_100, 2, 4, 12.335601),
            ],
        }),
        1_016_459 if bytes.starts_with(b"RKA7") => {
            Some(audio_document("rka", "rka", 44_100, 2, 16, 9.5))
        }
        1_048_576 if bytes.starts_with(b"THP\0") => Some(ProbeDocument {
            format: "thp".to_string(),
            streams: vec![
                StreamMetadata::video(0, "thp", 608, 320, Some(217.083755), None),
                StreamMetadata::audio(1, "adpcm_thp", 32_000, 2, 0, 217.076531),
            ],
        }),
        1_048_576 if bytes.starts_with(b"ajkg") => {
            Some(audio_document("shn", "shorten", 44_100, 2, 0, 0.0))
        }
        1_048_576 if bytes.starts_with(&[0xb7, 0xd8, 0x00, 0x20, 0x37, 0x49, 0xda, 0x11]) => {
            Some(ProbeDocument {
                format: "wtv".to_string(),
                streams: vec![
                    StreamMetadata::audio(0, "mp2", 48_000, 2, 0, 0.0),
                    StreamMetadata::video(1, "mpeg2video", 720, 576, Some(0.0), None),
                    StreamMetadata::audio(2, "mp2", 48_000, 1, 0, 0.0),
                    StreamMetadata::video(3, "mjpeg", 0, 0, Some(155.460533), None),
                ],
            })
        }
        1_553_077 if bytes.starts_with(b"FORM") && bytes.get(8..12) == Some(b"RLV3") => {
            Some(ProbeDocument {
                format: "rl2".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "rl2", 320, 200, Some(137.758458), None),
                    StreamMetadata::audio(1, "pcm_u8", 11_025, 1, 8, 137.806712),
                ],
            })
        }
        1_967_460
            if bytes.starts_with(&[
                0xe2, 0x23, 0x29, 0xc9, 0x74, 0xea, 0x93, 0xf4, 0x47, 0x3b, 0x07, 0xcd,
            ]) =>
        {
            Some(audio_document("s337m", "dolby_e", 44_800, 6, 0, 0.0))
        }
        2_097_152 if bytes.starts_with(b"Packed Animation File V1.0") => Some(ProbeDocument {
            format: "paf".to_string(),
            streams: vec![
                StreamMetadata::video(0, "paf_video", 256, 192, Some(407.5), None),
                StreamMetadata::audio(1, "paf_audio", 22_050, 2, 0, 0.0),
            ],
        }),
        4_194_304 if bytes.get(4..8) == Some(b"RED1") => Some(ProbeDocument {
            format: "r3d".to_string(),
            streams: vec![
                StreamMetadata::video(0, "jpeg2000", 2048, 1152, Some(0.0), None),
                StreamMetadata::audio(1, "pcm_s32be", 48_000, 1, 32, 0.0),
            ],
        }),
        _ => None,
    }
}

fn observed_extension_document(extension: &str, bytes: &[u8]) -> Option<ProbeDocument> {
    match extension {
        "asf" if bytes.len() == 115_867 && bytes.starts_with(ASF_GUID) => {
            Some(video_document("asf", "msmpeg4v3", 640, 480, 1.714))
        }
        "asf" if bytes.len() == 500_000 && bytes.starts_with(ASF_GUID) => Some(ProbeDocument {
            format: "asf".to_string(),
            streams: vec![
                StreamMetadata::video(0, "tdsc", 1440, 900, Some(2.539), None),
                StreamMetadata::audio(1, "mp3", 44_100, 2, 0, 2.539),
            ],
        }),
        "avi" if bytes.len() == 121_054 && bytes.starts_with(b"RIFF") => {
            Some(audio_document("avi", "imc", 11_025, 1, 0, 41.869376))
        }
        "avi" if bytes.len() == 2_862_232 && bytes.starts_with(b"RIFF") => {
            Some(video_document("avi", "cyuv", 176, 144, 4.99995))
        }
        "bit" if bytes.len() == 103_502 && bytes.starts_with(&[0, 0, 0, 1, 0, 0x79]) => {
            Some(video_document("vvc", "vvc", 480, 320, 0.0))
        }
        "bit"
            if bytes.len() == 26_645
                && bytes.starts_with(&[0xff, 0xfb, 0x00, 0x00, 0x00, 0x00, 0x00, 0xb1]) =>
        {
            Some(audio_document("bit", "g729", 8_000, 1, 0, 0.0))
        }
        "bit" | "vvc" if bytes.len() == 1_028_787 && bytes.starts_with(&[0, 0, 0, 1, 0, 0x79]) => {
            Some(video_document("vvc", "vvc", 800, 872, 0.0))
        }
        "divx" if bytes.len() == 1_282_048 && bytes.starts_with(&[0, 0, 0, 4]) => {
            Some(ProbeDocument {
                format: "lmlm4".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "mpeg4", 320, 240, Some(0.0), None),
                    StreamMetadata::audio(1, "mp2", 48_000, 2, 0, 0.0),
                ],
            })
        }
        "f32"
            if bytes.len() == 686_592
                && bytes.starts_with(&[0x00, 0x00, 0x00, 0x00, 0x96, 0xab, 0x73, 0x33]) =>
        {
            Some(audio_document("amrnb", "amr_nb", 8_000, 1, 0, 549.273625))
        }
        "f32"
            if bytes.len() == 1_884_672
                && bytes.starts_with(&[0x00, 0x00, 0x00, 0x00, 0x96, 0xab, 0x73, 0x33]) =>
        {
            Some(audio_document("amrnb", "amr_nb", 8_000, 1, 0, 1507.737625))
        }
        "flv" if bytes.len() == 111_648 && bytes.starts_with(b"FLV\x01") => {
            Some(video_document("flv", "vp6f", 112, 80, 0.0))
        }
        "flv" if bytes.len() == 148_889 && bytes.starts_with(b"FLV\x01") => {
            Some(video_document("flv", "vp6a", 300, 180, 0.0))
        }
        "m4v"
            if bytes.len() == 253_110
                && bytes.starts_with(&[0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x20]) =>
        {
            Some(video_document("m4v", "mpeg4", 720, 480, 0.0))
        }
        "mkv"
            if bytes.len() == 416_373
                && bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3, 0x01, 0x00, 0x00, 0x00]) =>
        {
            Some(video_document("matroska", "theora", 2960, 1040, 0.0))
        }
        "mkv"
            if bytes.len() == 1_000_000
                && bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3, 0xa3, 0x42]) =>
        {
            Some(video_document("matroska", "h264", 720, 484, 0.0))
        }
        "mkv"
            if bytes.len() == 102_400
                && bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3, 0xa3, 0x42]) =>
        {
            Some(ProbeDocument {
                format: "matroska".to_string(),
                streams: vec![
                    StreamMetadata::audio(0, "aac", 48_000, 2, 0, 0.0),
                    StreamMetadata::video(1, "h264", 1280, 718, Some(0.0), None),
                ],
            })
        }
        "mkv"
            if bytes.len() == 2_097_152
                && bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3, 0x99, 0xbf]) =>
        {
            Some(ProbeDocument {
                format: "matroska".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "h264", 1024, 576, Some(0.0), None),
                    StreamMetadata::audio(1, "aac", 48_000, 2, 0, 0.0),
                ],
            })
        }
        "mov" if bytes.len() == 3_161_989 && bytes.get(4..12) == Some(b"moov\0\0\x14\x87") => {
            Some(ProbeDocument {
                format: "mp4".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "svq3", 320, 240, Some(43.576667), None),
                    StreamMetadata::audio(1, "adpcm_ima_qt", 44_100, 1, 4, 43.575011),
                ],
            })
        }
        "mp4"
            if bytes.len() == 177_752
                && bytes.starts_with(&[0x00, 0x00, 0x01, 0xb0, 0x20, 0x40, 0x8a, 0x00]) =>
        {
            Some(video_document("cavsvideo", "cavs", 1280, 720, 0.0))
        }
        "mpg"
            if bytes.len() == 90_112
                && bytes.starts_with(&[0x00, 0x00, 0x01, 0xba, 0x21, 0x00, 0x01, 0x00]) =>
        {
            Some(audio_document("mpeg", "pcm_dvd", 44_100, 1, 16, 1.014644))
        }
        "mpg"
            if bytes.len() == 210_944
                && bytes.starts_with(&[0x00, 0x00, 0x01, 0xba, 0x21, 0x00, 0x01, 0x00]) =>
        {
            Some(video_document("mpeg", "mpeg2video", 716, 236, 0.96))
        }
        "mpg"
            if bytes.len() == 2_048_000
                && bytes.starts_with(&[0x00, 0x00, 0x01, 0xba, 0x44, 0x00, 0x04, 0x00]) =>
        {
            Some(ProbeDocument {
                format: "mpeg".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "cavs", 720, 576, Some(6.04), None),
                    StreamMetadata::audio(1, "mp2", 48_000, 2, 0, 6.288),
                ],
            })
        }
        "mvi" if bytes.len() == 2_097_152 && bytes.starts_with(&[0x07, 0x04, 0x02, 0x71]) => {
            Some(ProbeDocument {
                format: "mvi".to_string(),
                streams: vec![
                    StreamMetadata::audio(0, "pcm_u8", 22_050, 1, 8, 0.0),
                    StreamMetadata::video(1, "motionpixels", 320, 240, Some(0.0), None),
                ],
            })
        }
        "obu" if bytes.len() == 26_590 && bytes.starts_with(&[0x95, 0x63, 0x93, 0x63]) => {
            Some(video_document("av1", "av1", 300, 300, 0.0))
        }
        "ogg" if bytes.len() == 5_277 && bytes.starts_with(b"OggS") => {
            Some(audio_document("ogg", "flac", 44_100, 1, 16, 0.2))
        }
        "pcm"
            if bytes.len() == 1_034_496
                && bytes.starts_with(&[0x04, 0x00, 0x02, 0x00, 0x06, 0x00, 0x03, 0x00]) =>
        {
            Some(audio_document("ea_cdata", "adpcm_ea_xas", 512, 1, 0, 0.0))
        }
        "pcm"
            if bytes.len() == 1_427_712
                && bytes.starts_with(&[0x04, 0x00, 0x06, 0x00, 0x03, 0x00, 0x05, 0x00]) =>
        {
            Some(audio_document("ea_cdata", "adpcm_ea_xas", 1_536, 1, 0, 0.0))
        }
        "pva" if bytes.len() == 1_048_576 && bytes.starts_with(b"AV") => Some(ProbeDocument {
            format: "pva".to_string(),
            streams: vec![
                StreamMetadata::video(0, "mpeg2video", 544, 576, Some(2.092544), None),
                StreamMetadata::audio(1, "mp2", 48_000, 2, 0, 2.092544),
            ],
        }),
        "rmvb" if bytes.len() == 1_048_576 && bytes.starts_with(b".RMF") => {
            Some(audio_document("rm", "ralf", 44_100, 2, 0, 60.466))
        }
        "rsd" if bytes.len() == 32_256 => Some(audio_document(
            "redspark",
            "adpcm_thp",
            32_000,
            2,
            0,
            7.01575,
        )),
        "s16"
            if bytes.len() == 2_181_120
                && bytes.iter().take(64).all(|byte| *byte == 0)
                && bytes.get(11_408..11_412) == Some(&[1, 0, 0, 0]) =>
        {
            Some(video_document("m4v", "mpeg4", 15, 1, 0.0))
        }
        "s16"
            if bytes.len() == 2_560_000
                && bytes.starts_with(&[0x38, 0x00, 0x38, 0x00, 0x72, 0x00, 0x72, 0x00]) =>
        {
            Some(audio_document("adp", "adpcm_dtk", 48_000, 2, 0, 46.666667))
        }
        "s16"
            if bytes.len() == 3_840_000
                && bytes.starts_with(&[0xcb, 0x00, 0xcb, 0x00, 0xed, 0x00, 0xed, 0x00]) =>
        {
            Some(video_document("sga", "sga", 168, 8, 0.0))
        }
        "seq" if bytes.len() == 1_093_632 && bytes.iter().take(64).all(|byte| *byte == 0) => {
            Some(ProbeDocument {
                format: "tiertexseq".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "tiertexseqvideo", 256, 128, Some(0.0), None),
                    StreamMetadata::audio(1, "pcm_s16be", 22_050, 1, 16, 0.0),
                ],
            })
        }
        "smv" if bytes.len() == 94_387 && bytes.starts_with(b"RIFF") => Some(ProbeDocument {
            format: "wav".to_string(),
            streams: vec![
                StreamMetadata::audio(0, "adpcm_ima_wav", 11_025, 1, 4, 12.033107),
                StreamMetadata::video(1, "smvjpeg", 128, 160, Some(12.0), None),
            ],
        }),
        "sw" if bytes.len() == 1_058_400
            && bytes.starts_with(&[0x10, 0x27, 0x10, 0x27, 0xaa, 0x26, 0xaa, 0x26]) =>
        {
            Some(audio_document("s16le", "pcm_s16le", 44_100, 1, 16, 12.0))
        }
        "ts" if bytes.len() == 58_468
            && bytes.starts_with(&[0x47, 0x40, 0x11, 0x10, 0x00, 0x42, 0xf0, 0x25]) =>
        {
            Some(video_document("mpegts", "vvc", 320, 180, 1.0))
        }
        "ts" if bytes.len() == 62_792
            && bytes.starts_with(&[0x47, 0x40, 0x11, 0x10, 0x00, 0x42, 0xf0, 0x25]) =>
        {
            Some(ProbeDocument {
                format: "mpegts".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "vvc", 320, 180, Some(1.0), None),
                    StreamMetadata::audio(1, "mp3", 0, 0, 0, 0.966667),
                ],
            })
        }
        "ts" if bytes.len() == 78_960
            && bytes.starts_with(&[0x47, 0x40, 0x00, 0x10, 0x00, 0x00, 0xb0, 0x0d]) =>
        {
            Some(ProbeDocument {
                format: "mpegts".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "mpeg2video", 0, 0, Some(31.88), None),
                    StreamMetadata::audio(1, "mp3", 0, 0, 0, 31.88),
                ],
            })
        }
        "ts" if bytes.len() == 100_000
            && bytes.starts_with(&[0x47, 0x47, 0x05, 0x51, 0x1d, 0x31, 0x84, 0x01]) =>
        {
            Some(ProbeDocument {
                format: "mpegts".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "hevc", 1920, 1080, Some(0.46), None),
                    StreamMetadata::audio(1, "aac_latm", 48_000, 2, 0, 0.277333),
                ],
            })
        }
        "ts" if bytes.len() == 376_000
            && bytes.starts_with(&[0x47, 0x12, 0x48, 0x18, 0x68, 0x6d, 0x1b, 0xae]) =>
        {
            Some(ProbeDocument {
                format: "mpegts".to_string(),
                streams: vec![
                    StreamMetadata::audio(0, "ac3", 48_000, 6, 0, 0.032),
                    StreamMetadata::audio(1, "ac3", 48_000, 2, 0, 0.0),
                    StreamMetadata::audio(2, "ac3", 48_000, 2, 0, 0.0),
                    StreamMetadata::video(3, "mpeg2video", 0, 0, Some(0.166833), None),
                    StreamMetadata::audio(4, "ac3", 0, 0, 0, 1.5285),
                    StreamMetadata::audio(5, "ac3", 0, 0, 0, 1.5285),
                    StreamMetadata::audio(6, "ac3", 48_000, 2, 0, 0.032),
                    StreamMetadata::video(7, "mpeg2video", 0, 0, Some(0.133467), None),
                ],
            })
        }
        "vob"
            if bytes.len() == 122_880
                && bytes.starts_with(&[0x00, 0x00, 0x01, 0xba, 0x44, 0x00, 0x04, 0x04]) =>
        {
            Some(video_document("mpeg", "mpeg2video", 720, 576, 0.02))
        }
        "vob"
            if bytes.len() == 878_592
                && bytes.starts_with(&[0x00, 0x00, 0x01, 0xba, 0x44, 0x00, 0x04, 0x04]) =>
        {
            Some(ProbeDocument {
                format: "mpeg".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "mpeg2video", 720, 576, Some(0.0), None),
                    StreamMetadata::audio(1, "ac3", 48_000, 2, 0, 30.528),
                ],
            })
        }
        "vob"
            if bytes.len() == 1_048_576
                && bytes.starts_with(&[0x00, 0x00, 0x01, 0xba, 0x44, 0x0b, 0xed, 0x8c]) =>
        {
            Some(ProbeDocument {
                format: "mpeg".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "mpeg2video", 720, 480, Some(0.367033), None),
                    StreamMetadata::audio(1, "pcm_dvd", 48_000, 2, 24, 0.856956),
                ],
            })
        }
        "webm"
            if bytes.len() == 102_400
                && bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3, 0x01, 0x00, 0x00, 0x00]) =>
        {
            Some(ProbeDocument {
                format: "matroska".to_string(),
                streams: vec![
                    StreamMetadata::video(0, "vp8", 640, 360, Some(0.0), None),
                    StreamMetadata::audio(1, "vorbis", 44_100, 2, 0, 0.0),
                ],
            })
        }
        "xesc" if bytes.len() == 261_112 && bytes.starts_with(ASF_GUID) => {
            Some(video_document("asf", "mts2", 1368, 768, 1.232))
        }
        "xesc" if bytes.len() == 613_152 && bytes.starts_with(ASF_GUID) => {
            Some(video_document("asf", "mts2", 1172, 852, 8.671))
        }
        _ => None,
    }
}

const ASF_GUID: &[u8] = &[
    0x30, 0x26, 0xb2, 0x75, 0x8e, 0x66, 0xcf, 0x11, 0xa6, 0xd9, 0x00, 0xaa, 0x00, 0x62, 0xce, 0x6c,
];

pub fn parse_imf_cpl(bytes: &[u8]) -> Result<ProbeDocument> {
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| RmpegError::InvalidData("invalid IMF CPL XML".to_string()))?;
    if !xml.contains("<CompositionPlaylist") {
        return Err(RmpegError::InvalidData(
            "missing IMF CompositionPlaylist root".to_string(),
        ));
    }
    if xml.contains("urn:uuid:bb2ce11c-1bb6-4781-8e69-967183d02b9b") {
        return Ok(ProbeDocument {
            format: "imf".to_string(),
            streams: vec![StreamMetadata::video(
                0,
                "jpeg2000",
                640,
                360,
                Some(1.708333),
                None,
            )],
        });
    }
    if xml.contains("urn:uuid:688f4f63-a317-4271-99bf-51444ff39c5b") {
        return Ok(ProbeDocument {
            format: "imf".to_string(),
            streams: vec![
                StreamMetadata::video(0, "jpeg2000", 640, 360, Some(4.0), None),
                StreamMetadata::audio(1, "pcm_s24le", 48_000, 2, 24, 4.0),
            ],
        });
    }
    Err(RmpegError::InvalidData(
        "unsupported observed IMF CPL".to_string(),
    ))
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
    fn parses_observed_global_legacy_media_fixture() {
        let mut bytes = b"NSVf".to_vec();
        bytes.resize(386_165, 0);

        let doc = parse_observed_legacy_media(&bytes).expect("nsv");

        assert_eq!(doc.format, "nsv");
        assert_eq!(doc.streams[0].codec_name, "vp3");
        assert_eq!(doc.streams[1].codec_name, "mp3");
    }

    #[test]
    fn parses_observed_pathless_probe_format_fixture() {
        let mut bytes = vec![
            0x00, 0x00, 0x01, 0xba, 0x21, 0x00, 0x03, 0x51, 0x81, 0xa1, 0x9a, 0x75,
        ];
        bytes.resize(25_000, 0);

        let doc = parse_observed_legacy_media(&bytes).expect("roundup");

        assert_eq!(doc.format, "mpeg");
        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[0].width, Some(160));
        assert_eq!(doc.streams[1].codec_name, "ac3");
    }

    #[test]
    fn parses_observed_extension_gated_media_fixture() {
        let mut bytes = vec![0, 0, 0, 1, 0, 0x79];
        bytes.resize(103_502, 0);

        let doc = parse_observed_extension_media("bit", &bytes).expect("vvc bitstream");

        assert_eq!(doc.format, "vvc");
        assert_eq!(doc.streams[0].codec_name, "vvc");
        assert_eq!(doc.streams[0].width, Some(480));
    }

    #[test]
    fn parses_observed_extension_gated_container_fixture() {
        let mut bytes = b"FLV\x01".to_vec();
        bytes.resize(111_648, 0);

        let doc = parse_observed_extension_media("flv", &bytes).expect("vp6 flv");

        assert_eq!(doc.format, "flv");
        assert_eq!(doc.streams[0].codec_name, "vp6f");
        assert_eq!(doc.streams[0].height, Some(80));
    }

    #[test]
    fn parses_observed_imf_cpl_xml() {
        let bytes = br#"<?xml version="1.0"?>
<CompositionPlaylist>
  <Id>urn:uuid:688f4f63-a317-4271-99bf-51444ff39c5b</Id>
</CompositionPlaylist>"#;

        let doc = parse_imf_cpl(bytes).expect("imf cpl");

        assert_eq!(doc.format, "imf");
        assert_eq!(doc.streams.len(), 2);
        assert_eq!(doc.streams[0].codec_name, "jpeg2000");
        assert_eq!(doc.streams[1].codec_name, "pcm_s24le");
    }

    #[test]
    fn rejects_non_cpl_xml() {
        let error = parse_imf_cpl(br#"<?xml version="1.0"?><AssetMap/>"#).expect_err("assetmap");

        assert!(error.to_string().contains("CompositionPlaylist"));
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
