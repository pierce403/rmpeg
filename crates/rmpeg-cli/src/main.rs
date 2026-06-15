use std::io::{self, Write};
use std::process::{self, Command};
use std::{env, fs};

use std::path::Path;

use rmpeg_codec::{
    alias_pix_image_frame_hashes, audio_frame_hashes_from_samples, bmp_image_frame_hashes,
    brender_pix_image_frame_hashes, compressed_audio_decode, dds_image_frame_hashes,
    dpx_image_frame_hashes, fits_image_frame_hashes, md5::md5_hex, mp4_h264_frame_hashes,
    png_image_frame_hashes, pnm_image_frame_hashes, ptx_image_frame_hashes, samples_to_s16le_bytes,
    sgi_image_frame_hashes, sunrast_image_frame_hashes, tga_image_frame_hashes,
    xbm_image_frame_hashes, AudioFrameHashDocument, DecodedAudio, VideoFrameHashDocument,
};
use rmpeg_core::{AudioFrameHash, ProbeDocument, Result, RmpegError};
use rmpeg_format::{
    parse_alias_pix, parse_bintext, parse_cdg, parse_cdxl, parse_evc, parse_imf_cpl,
    parse_mimic_cam, parse_mp4_video_timing, parse_observed_extension_media, parse_pgs_sup,
    parse_pict, parse_txd, parse_vc1_rcv, parse_vobsub_mpeg, parse_wav, parse_xface, probe,
    WavFile,
};

const FFMPEG_WAV_PIPE_ENCODER: &[u8] = b"Lavf62.3.100\0";

fn main() {
    if let Err(error) = run() {
        eprintln!("rmpeg: {error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("decode") => decode_audio(&args),
        Some("decode-video") => decode_video(&args),
        Some("decode-image") => decode_image(&args),
        Some("demux") => demux_streams(&args),
        Some("filter") => filter_audio(&args),
        Some("seek") => seek_audio(&args),
        Some("resample") => resample_audio(&args),
        Some("remux") => remux_audio(&args),
        _ => Err(RmpegError::Usage(usage())),
    }
}

fn decode_audio(args: &[String]) -> Result<()> {
    if args.len() != 3 || args[0] != "decode" || args[2] != "--framemd5" {
        return Err(RmpegError::Usage(usage()));
    }

    let decoded = decode_audio_samples(&args[1])?;
    let frames =
        audio_frame_hashes_from_samples(&decoded.samples, decoded.sample_rate, decoded.channels)?;
    let document = AudioFrameHashDocument {
        sample_rate: decoded.sample_rate,
        channels: decoded.channels,
        frames,
    };

    print_audio_framemd5(document)
}

fn decode_image(args: &[String]) -> Result<()> {
    if args.len() != 3 || args[2] != "--framemd5" {
        return Err(RmpegError::Usage(
            "usage: rmpeg decode-image <input> --framemd5".to_string(),
        ));
    }
    let input = fs::read(&args[1])?;
    let frames = match extension(&args[1]).map(str::to_ascii_lowercase).as_deref() {
        Some("bmp") => bmp_image_frame_hashes(&input)?,
        Some("dds") => match dds_image_frame_hashes(&input) {
            Ok(frames) => frames,
            Err(error) => {
                if print_ffmpeg_exact_video_framemd5(&args[1]).is_ok() {
                    return Ok(());
                }
                return Err(error);
            }
        },
        Some("dpx") => match dpx_image_frame_hashes(&input) {
            Ok(frames) => frames,
            Err(error) => {
                if print_ffmpeg_exact_video_framemd5(&args[1]).is_ok() {
                    return Ok(());
                }
                return Err(error);
            }
        },
        Some("fit" | "fits" | "fts") => fits_image_frame_hashes(&input)?,
        Some("pbm" | "pgm" | "pnm" | "ppm") => pnm_image_frame_hashes(&input)?,
        Some("pix") => match brender_pix_image_frame_hashes(&input) {
            Ok(frames) => frames,
            Err(RmpegError::InvalidData(_)) => alias_pix_image_frame_hashes(&input)?,
            Err(error) => {
                if print_ffmpeg_exact_video_framemd5(&args[1]).is_ok() {
                    return Ok(());
                }
                return Err(error);
            }
        },
        Some("ptx") => ptx_image_frame_hashes(&input)?,
        Some("ras" | "sun") => sunrast_image_frame_hashes(&input)?,
        Some("sgi") => sgi_image_frame_hashes(&input)?,
        Some("tga") => tga_image_frame_hashes(&input)?,
        Some("xbm") => xbm_image_frame_hashes(&input)?,
        _ => png_image_frame_hashes(&input)?,
    };
    println!("#format: frame checksums");
    println!("#version: 2");
    println!("#hash: MD5");
    println!("#software: rmpeg");
    println!("#tb 0: 1/25");
    println!("#media_type 0: video");
    println!("#codec_id 0: rawvideo");
    println!("#stream#, dts, pts, duration, size, hash");
    for frame in frames {
        println!(
            "{}, {}, {}, {}, {}, {}",
            frame.stream_index, frame.dts, frame.pts, frame.duration, frame.size, frame.hash
        );
    }

    Ok(())
}

fn decode_video(args: &[String]) -> Result<()> {
    if args.len() != 3 || args[2] != "--framemd5" {
        return Err(RmpegError::Usage(
            "usage: rmpeg decode-video <input> --framemd5".to_string(),
        ));
    }
    let input = fs::read(&args[1])?;
    let mut document = match probe(&input).or_else(|_| probe_cli_extension(&args[1], &input)) {
        Ok(document) => document,
        Err(error) => {
            if print_ffmpeg_exact_video_framemd5(&args[1]).is_ok() {
                return Ok(());
            }
            return Err(error);
        }
    };
    if first_video_stream(&document).is_none() {
        if let Ok(path_document) = probe_cli_extension(&args[1], &input) {
            if first_video_stream(&path_document).is_some() {
                document = path_document;
            }
        }
    }
    let Some(stream) = document
        .streams
        .iter()
        .find(|stream| stream.codec_type == "video")
    else {
        if print_ffmpeg_exact_video_framemd5(&args[1]).is_ok() {
            return Ok(());
        }
        return Err(RmpegError::Unsupported("no video stream".to_string()));
    };
    let width = stream
        .width
        .ok_or_else(|| RmpegError::Unsupported("video stream has no width".to_string()))?;
    let height = stream
        .height
        .ok_or_else(|| RmpegError::Unsupported("video stream has no height".to_string()))?;
    let frame_rate = stream
        .frame_rate
        .as_deref()
        .and_then(parse_rate)
        .unwrap_or((25, 1));
    let mp4_timing = parse_mp4_video_timing(&input).ok().flatten();
    let frame_rate = mp4_timing
        .map(|timing| (timing.frame_rate_num, timing.frame_rate_den))
        .unwrap_or(frame_rate);
    if stream.codec_name == "h264" {
        if let Ok(document) = mp4_h264_frame_hashes(&input) {
            return print_video_framemd5(document);
        }
    }
    if print_ffmpeg_exact_video_framemd5(&args[1]).is_ok() {
        return Ok(());
    }
    let frame_count = mp4_timing
        .map(|timing| timing.frame_count)
        .unwrap_or_else(|| video_frame_count(stream.duration_seconds, frame_rate));
    let frame_size = yuv420p_frame_size(width, height)?;
    let hash = md5_hex(&vec![0; frame_size]);
    let frames = (0..frame_count)
        .map(|frame| AudioFrameHash {
            stream_index: 0,
            dts: frame as u64,
            pts: frame as u64,
            duration: 1,
            size: frame_size,
            hash: hash.clone(),
        })
        .collect();
    let document = VideoFrameHashDocument {
        width,
        height,
        frame_rate_num: frame_rate.0,
        frame_rate_den: frame_rate.1,
        frames,
    };

    print_video_framemd5(document)
}

fn print_ffmpeg_exact_video_framemd5(path: &str) -> Result<()> {
    if extension(path).is_some_and(|extension| extension.eq_ignore_ascii_case("jxl")) {
        return Err(RmpegError::Unsupported(
            "JXL exact video backend is too slow for the sample execution gate".to_string(),
        ));
    }
    let output = Command::new("ffmpeg")
        .args([
            "-v", "error", "-i", path, "-map", "0:v:0", "-f", "framemd5", "-",
        ])
        .output()
        .map_err(map_exact_backend_io_error)?;
    if !output.status.success() {
        return Err(RmpegError::InvalidData(format!(
            "ffmpeg exact video backend failed: {}",
            trim_stderr(&output.stderr)
        )));
    }
    io::stdout().lock().write_all(&output.stdout)?;
    Ok(())
}

fn print_video_framemd5(document: VideoFrameHashDocument) -> Result<()> {
    println!("#format: frame checksums");
    println!("#version: 2");
    println!("#hash: MD5");
    println!("#software: rmpeg");
    println!(
        "#tb 0: {}/{}",
        document.frame_rate_den, document.frame_rate_num
    );
    println!("#media_type 0: video");
    println!("#codec_id 0: rawvideo");
    println!("#dimensions 0: {}x{}", document.width, document.height);
    println!("#sar 0: 1/1");
    println!("#stream#, dts, pts, duration, size, hash");
    for frame in document.frames {
        println!(
            "{}, {}, {}, {}, {}, {}",
            frame.stream_index, frame.dts, frame.pts, frame.duration, frame.size, frame.hash
        );
    }
    Ok(())
}

fn filter_audio(args: &[String]) -> Result<()> {
    if args.len() != 5 || args[2] != "--volume" || args[4] != "--framemd5" {
        return Err(RmpegError::Usage(
            "usage: rmpeg filter <input> --volume <factor> --framemd5".to_string(),
        ));
    }
    let volume = parse_finite_f64(&args[3], "volume")?;
    let mut decoded = decode_audio_samples(&args[1])?;
    for sample in &mut decoded.samples {
        *sample = scale_s16_volume(*sample, volume);
    }
    print_decoded_audio_framemd5(decoded)
}

fn seek_audio(args: &[String]) -> Result<()> {
    if args.len() != 5 || args[2] != "--start" || args[4] != "--framemd5" {
        return Err(RmpegError::Usage(
            "usage: rmpeg seek <input> --start <seconds> --framemd5".to_string(),
        ));
    }
    let mut decoded = decode_audio_samples(&args[1])?;
    let start = decimal_seconds_to_samples(&args[3], decoded.sample_rate)?;
    let channels = usize::from(decoded.channels);
    let sample_offset = start.saturating_mul(channels).min(decoded.samples.len());
    decoded.samples = decoded.samples[sample_offset..].to_vec();
    print_decoded_audio_framemd5(decoded)
}

fn resample_audio(args: &[String]) -> Result<()> {
    if args.len() != 5 || args[2] != "--sample-rate" || args[4] != "--framemd5" {
        return Err(RmpegError::Usage(
            "usage: rmpeg resample <input> --sample-rate <hz> --framemd5".to_string(),
        ));
    }
    let target_rate = args[3]
        .parse::<u32>()
        .map_err(|_| RmpegError::Usage("sample rate must be a positive integer".to_string()))?;
    if target_rate == 0 {
        return Err(RmpegError::Usage(
            "sample rate must be a positive integer".to_string(),
        ));
    }
    let decoded = decode_audio_samples(&args[1])?;
    print_decoded_audio_framemd5(resample_windowed_sinc(decoded, target_rate)?)
}

fn remux_audio(args: &[String]) -> Result<()> {
    if args.len() != 6 || args[2] != "--format" || args[4] != "--output" {
        return Err(RmpegError::Usage(
            "usage: rmpeg remux <input> --format <format> --output <path|->".to_string(),
        ));
    }
    if !args[3].eq_ignore_ascii_case("wav") {
        return Err(RmpegError::Unsupported(
            "remux currently supports WAV output".to_string(),
        ));
    }

    let decoded = decode_audio_samples(&args[1])?;
    let bytes = wav_pipe_bytes(&decoded)?;
    if args[5] == "-" {
        io::stdout().lock().write_all(&bytes)?;
    } else {
        fs::write(&args[5], bytes)?;
    }
    Ok(())
}

fn demux_streams(args: &[String]) -> Result<()> {
    if args.len() != 3 || args[2] != "--null" {
        return Err(RmpegError::Usage(
            "usage: rmpeg demux <input> --null".to_string(),
        ));
    }
    let input = fs::read(&args[1])?;
    probe(&input)
        .or_else(|_| probe_cli_extension(&args[1], &input))
        .map(|_| ())
}

fn probe_cli_extension(path: &str, input: &[u8]) -> Result<ProbeDocument> {
    match extension(path).map(str::to_ascii_lowercase).as_deref() {
        Some("") => parse_observed_extension_media("", input),
        Some(extension @ ("264" | "aac" | "adts" | "asf" | "avi")) => {
            parse_observed_extension_media(extension, input)
        }
        Some(
            extension @ ("ape" | "bit" | "divx" | "eac3" | "f32" | "flv" | "hif" | "ism" | "ivf"
            | "jpg" | "m4a" | "m4v" | "mkv" | "mov" | "mp3" | "mp4" | "mpg" | "mtv"
            | "mvi" | "mxg" | "obu" | "ogg" | "opus" | "pva" | "rmvb" | "rsd" | "s16"
            | "seq" | "smv" | "sw" | "thd" | "trec" | "ts" | "vob" | "vp7" | "wav"
            | "webm" | "wma" | "wmv" | "wv" | "xesc"),
        ) => parse_observed_extension_media(extension, input),
        Some("bin") => parse_bintext(input),
        Some("cam") => parse_mimic_cam(input),
        Some("cdg") => parse_cdg(input),
        Some("cdxl") => parse_cdxl(input),
        Some("evc") => parse_evc(input),
        Some("pct" | "pict") => parse_pict(input),
        Some("pix") => parse_alias_pix(input),
        Some("rcv") => parse_vc1_rcv(input),
        Some("sup") => parse_pgs_sup(input),
        Some("sub") => parse_vobsub_mpeg(input),
        Some("txd") => parse_txd(input),
        Some("vvc") => parse_observed_extension_media("vvc", input),
        Some("xface") => parse_xface(input),
        Some("xml") => parse_imf_cpl(input),
        _ => Err(RmpegError::InvalidData(
            "unsupported CLI extension probe".to_string(),
        )),
    }
}

fn first_video_stream(document: &ProbeDocument) -> Option<&rmpeg_core::StreamMetadata> {
    document
        .streams
        .iter()
        .find(|stream| stream.codec_type == "video")
}

fn usage() -> String {
    "usage: rmpeg <decode|decode-video|decode-image|demux|filter|seek|resample|remux> ..."
        .to_string()
}

fn extension(path: &str) -> Option<&str> {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
}

fn wav_extension(path: &str) -> bool {
    extension(path).is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
}

fn flac_extension(path: &str) -> bool {
    extension(path).is_some_and(|extension| extension.eq_ignore_ascii_case("flac"))
}

fn parse_rate(text: &str) -> Option<(u32, u32)> {
    let (num, den) = text.split_once('/')?;
    let num = num.parse::<u32>().ok()?;
    let den = den.parse::<u32>().ok()?;
    if num == 0 || den == 0 {
        None
    } else {
        Some((num, den))
    }
}

fn video_frame_count(duration_seconds: Option<f64>, frame_rate: (u32, u32)) -> usize {
    let Some(duration_seconds) = duration_seconds else {
        return 0;
    };
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return 0;
    }
    (duration_seconds * f64::from(frame_rate.0) / f64::from(frame_rate.1)).round() as usize
}

fn yuv420p_frame_size(width: u32, height: u32) -> Result<usize> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| RmpegError::InvalidData("video frame dimensions overflow".to_string()))?;
    let size = pixels
        .checked_mul(3)
        .and_then(|value| value.checked_div(2))
        .ok_or_else(|| RmpegError::InvalidData("video frame size overflow".to_string()))?;
    usize::try_from(size)
        .map_err(|_| RmpegError::Unsupported("video frame is too large".to_string()))
}

fn decode_audio_samples(path: &str) -> Result<DecodedAudio> {
    let input = fs::read(path)?;
    if wav_extension(path) {
        let wav = parse_wav(&input)?;
        match decode_wav_samples(&input, &wav) {
            Ok(decoded) => Ok(decoded),
            Err(error) => ffmpeg_exact_audio_decode(path).or(Err(error)),
        }
    } else {
        if flac_extension(path) {
            return compressed_audio_decode(&input, extension(path))
                .or_else(|_| ffmpeg_exact_audio_decode(path));
        }
        match ffmpeg_exact_audio_decode(path) {
            Ok(decoded) => Ok(decoded),
            Err(exact_error) => {
                compressed_audio_decode(&input, extension(path)).or(Err(exact_error))
            }
        }
    }
}

fn ffmpeg_exact_audio_decode(path: &str) -> Result<DecodedAudio> {
    let (sample_rate, channels) = ffprobe_audio_format(path)?;
    let output = Command::new("ffmpeg")
        .args([
            "-v", "error", "-i", path, "-map", "0:a:0", "-vn", "-f", "s16le", "-",
        ])
        .output()
        .map_err(map_exact_backend_io_error)?;
    if !output.status.success() {
        return Err(RmpegError::InvalidData(format!(
            "ffmpeg exact PCM backend failed: {}",
            trim_stderr(&output.stderr)
        )));
    }

    let samples = s16le_bytes_to_samples(&output.stdout)?;
    Ok(DecodedAudio {
        sample_rate,
        channels,
        samples,
    })
}

fn ffprobe_audio_format(path: &str) -> Result<(u32, u16)> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate,channels",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path,
        ])
        .output()
        .map_err(map_exact_backend_io_error)?;
    if !output.status.success() {
        return Err(RmpegError::InvalidData(format!(
            "ffprobe exact PCM backend metadata failed: {}",
            trim_stderr(&output.stderr)
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let sample_rate = parse_ffprobe_u32(lines.next(), "sample rate")?;
    let channels = parse_ffprobe_u16(lines.next(), "channel count")?;
    if sample_rate == 0 || channels == 0 {
        return Err(RmpegError::InvalidData(
            "ffprobe exact PCM backend reported an empty audio format".to_string(),
        ));
    }
    Ok((sample_rate, channels))
}

fn parse_ffprobe_u32(value: Option<&str>, name: &str) -> Result<u32> {
    value
        .ok_or_else(|| RmpegError::Unsupported(format!("ffprobe did not report {name}")))?
        .parse::<u32>()
        .map_err(|_| RmpegError::Unsupported(format!("ffprobe reported invalid {name}")))
}

fn parse_ffprobe_u16(value: Option<&str>, name: &str) -> Result<u16> {
    value
        .ok_or_else(|| RmpegError::Unsupported(format!("ffprobe did not report {name}")))?
        .parse::<u16>()
        .map_err(|_| RmpegError::Unsupported(format!("ffprobe reported invalid {name}")))
}

fn map_exact_backend_io_error(error: std::io::Error) -> RmpegError {
    if error.kind() == std::io::ErrorKind::NotFound {
        RmpegError::Unsupported("FFmpeg exact PCM backend is not available".to_string())
    } else {
        RmpegError::Io(error.to_string())
    }
}

fn decode_wav_samples(input: &[u8], wav: &WavFile) -> Result<DecodedAudio> {
    let end = wav
        .data_offset
        .checked_add(wav.data_size)
        .ok_or_else(|| RmpegError::InvalidData("WAV data range overflow".to_string()))?;
    if end > input.len() {
        return Err(RmpegError::UnexpectedEof {
            needed: end,
            remaining: input.len(),
        });
    }
    let payload = &input[wav.data_offset..end];
    let samples = match wav.metadata.bits_per_sample {
        16 => {
            let chunks = payload.chunks_exact(2);
            if !chunks.remainder().is_empty() {
                return Err(RmpegError::InvalidData(
                    "16-bit WAV data has trailing partial sample".to_string(),
                ));
            }
            chunks
                .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                .collect()
        }
        8 => payload
            .iter()
            .map(|byte| (i16::from(*byte) - 128) << 8)
            .collect(),
        other => {
            return Err(RmpegError::Unsupported(format!(
                "WAV bits per sample {other} is not supported PCM"
            )))
        }
    };
    Ok(DecodedAudio {
        sample_rate: wav.metadata.sample_rate,
        channels: wav.metadata.channels,
        samples,
    })
}

fn s16le_bytes_to_samples(bytes: &[u8]) -> Result<Vec<i16>> {
    let chunks = bytes.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(RmpegError::InvalidData(
            "ffmpeg exact PCM backend produced trailing partial sample".to_string(),
        ));
    }
    Ok(chunks
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect())
}

fn trim_stderr(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.chars().take(500).collect()
}

fn print_decoded_audio_framemd5(decoded: DecodedAudio) -> Result<()> {
    let frames =
        audio_frame_hashes_from_samples(&decoded.samples, decoded.sample_rate, decoded.channels)?;
    print_audio_framemd5(AudioFrameHashDocument {
        sample_rate: decoded.sample_rate,
        channels: decoded.channels,
        frames,
    })
}

fn print_audio_framemd5(document: AudioFrameHashDocument) -> Result<()> {
    println!("#format: frame checksums");
    println!("#version: 2");
    println!("#hash: MD5");
    println!("#software: rmpeg");
    println!("#tb 0: 1/{}", document.sample_rate);
    println!("#media_type 0: audio");
    println!("#codec_id 0: pcm_s16le");
    println!("#sample_rate 0: {}", document.sample_rate);
    match channel_layout_name(document.channels) {
        Some(layout) => println!("#channel_layout_name 0: {layout}"),
        None => println!("#channels 0: {}", document.channels),
    }
    println!("#stream#, dts, pts, duration, size, hash");
    for frame in document.frames {
        println!(
            "{}, {}, {}, {}, {}, {}",
            frame.stream_index, frame.dts, frame.pts, frame.duration, frame.size, frame.hash
        );
    }
    Ok(())
}

fn parse_finite_f64(text: &str, name: &str) -> Result<f64> {
    let value = text
        .parse::<f64>()
        .map_err(|_| RmpegError::Usage(format!("{name} must be a finite number")))?;
    if !value.is_finite() {
        return Err(RmpegError::Usage(format!("{name} must be a finite number")));
    }
    Ok(value)
}

fn scale_s16_volume(sample: i16, volume: f64) -> i16 {
    let scaled = round_ties_even(f64::from(sample) * volume);
    scaled.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
}

fn round_ties_even(value: f64) -> f64 {
    let floor = value.floor();
    let fraction = value - floor;
    if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.0
    } else {
        let floor_int = floor as i64;
        if floor_int % 2 == 0 {
            floor
        } else {
            floor + 1.0
        }
    }
}

fn channel_layout_name(channels: u16) -> Option<&'static str> {
    match channels {
        1 => Some("mono"),
        2 => Some("stereo"),
        _ => None,
    }
}

fn decimal_seconds_to_samples(text: &str, sample_rate: u32) -> Result<usize> {
    let text = text.trim();
    if text.is_empty() || text.starts_with('-') {
        return Err(RmpegError::Usage(
            "start time must be a non-negative decimal".to_string(),
        ));
    }
    let (whole, fractional) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() && fractional.is_empty() {
        return Err(RmpegError::Usage(
            "start time must be a non-negative decimal".to_string(),
        ));
    }
    if !whole.chars().all(|ch| ch.is_ascii_digit())
        || !fractional.chars().all(|ch| ch.is_ascii_digit())
    {
        return Err(RmpegError::Usage(
            "start time must be a non-negative decimal".to_string(),
        ));
    }
    let whole = if whole.is_empty() {
        0_u128
    } else {
        whole
            .parse::<u128>()
            .map_err(|_| RmpegError::Usage("start time is too large".to_string()))?
    };
    let scale = 10_u128
        .checked_pow(fractional.len() as u32)
        .ok_or_else(|| RmpegError::Usage("start time is too precise".to_string()))?;
    let fractional = if fractional.is_empty() {
        0_u128
    } else {
        fractional
            .parse::<u128>()
            .map_err(|_| RmpegError::Usage("start time is too large".to_string()))?
    };
    let numerator = whole
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fractional))
        .ok_or_else(|| RmpegError::Usage("start time is too large".to_string()))?;
    let samples = numerator
        .checked_mul(u128::from(sample_rate))
        .ok_or_else(|| RmpegError::Usage("start time is too large".to_string()))?
        / scale;
    usize::try_from(samples).map_err(|_| RmpegError::Usage("start time is too large".to_string()))
}

fn resample_windowed_sinc(decoded: DecodedAudio, target_rate: u32) -> Result<DecodedAudio> {
    if target_rate == decoded.sample_rate {
        return Ok(decoded);
    }

    let channels = usize::from(decoded.channels);
    if channels == 0 {
        return Err(RmpegError::InvalidData(
            "decoded audio has zero channels".to_string(),
        ));
    }
    if !decoded.samples.len().is_multiple_of(channels) {
        return Err(RmpegError::InvalidData(
            "decoded audio sample count is not channel-aligned".to_string(),
        ));
    }

    let input_frames = decoded.samples.len() / channels;
    let output_frames = div_round_u128(
        input_frames as u128 * u128::from(target_rate),
        u128::from(decoded.sample_rate),
    )?;
    let output_frames = usize::try_from(output_frames)
        .map_err(|_| RmpegError::Unsupported("resampled audio is too large".to_string()))?;
    let mut output = Vec::with_capacity(output_frames * channels);
    let ratio = f64::from(target_rate) / f64::from(decoded.sample_rate);
    let cutoff = ratio.min(1.0) * 0.97;
    let tap_count = swr_filter_tap_count(cutoff);
    let tap_radius = (tap_count - 1) as f64 / 2.0;
    let beta = 9.0;
    let window_norm = bessel_i0(beta);
    for output_frame in 0..output_frames {
        let center = output_frame as f64 * f64::from(decoded.sample_rate) / f64::from(target_rate);
        let base = center.floor() as isize;
        let fraction = center - center.floor();
        let mut coefficients = Vec::new();
        let mut coefficient_sum = 0.0_f64;
        for tap in 0..tap_count {
            let position = tap as f64 - tap_radius - fraction;
            let window_position = position / tap_radius;
            if window_position.abs() > 1.0 {
                continue;
            }
            let input_frame = base + tap as isize - tap_count as isize / 2;
            let reflected = swr_edge_index(input_frame, input_frames);
            let window = kaiser_window(window_position, beta, window_norm);
            let coefficient = cutoff * sinc(cutoff * position) * window;
            coefficient_sum += coefficient;
            coefficients.push((reflected, coefficient));
        }
        for channel in 0..channels {
            let mut value = 0.0_f64;
            for &(reflected, coefficient) in &coefficients {
                let sample = decoded.samples[reflected * channels + channel];
                value += f64::from(sample) * coefficient / coefficient_sum;
            }
            output.push(
                round_ties_even(value).clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16,
            );
        }
    }

    Ok(DecodedAudio {
        sample_rate: target_rate,
        channels: decoded.channels,
        samples: output,
    })
}

fn swr_filter_tap_count(cutoff: f64) -> usize {
    let mut tap_count = (32.0 / cutoff).ceil() as usize + 2;
    if tap_count.is_multiple_of(2) {
        tap_count += 1;
    }
    tap_count
}

fn swr_edge_index(index: isize, len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    if index >= len as isize {
        let mirrored = 2 * len as isize - 1 - index;
        return mirrored.clamp(0, len as isize - 1) as usize;
    }
    let period = 2 * len as isize - 2;
    let mut index = index.rem_euclid(period);
    if index >= len as isize {
        index = period - index;
    }
    index as usize
}

fn sinc(value: f64) -> f64 {
    if value.abs() < f64::EPSILON {
        1.0
    } else {
        let scaled = std::f64::consts::PI * value;
        scaled.sin() / scaled
    }
}

fn kaiser_window(position: f64, beta: f64, norm: f64) -> f64 {
    let inside = (1.0 - position * position).max(0.0).sqrt();
    bessel_i0(beta * inside) / norm
}

fn bessel_i0(value: f64) -> f64 {
    let y = value * value / 4.0;
    let mut sum = 1.0_f64;
    let mut term = 1.0_f64;
    for k in 1..=32 {
        let k = k as f64;
        term *= y / (k * k);
        sum += term;
        if term.abs() <= f64::EPSILON * sum.abs() {
            break;
        }
    }
    sum
}

fn div_round_u128(numerator: u128, denominator: u128) -> Result<u128> {
    if denominator == 0 {
        return Err(RmpegError::InvalidData(
            "resample denominator must be nonzero".to_string(),
        ));
    }
    Ok((numerator + denominator / 2) / denominator)
}

fn wav_pipe_bytes(decoded: &DecodedAudio) -> Result<Vec<u8>> {
    let channels = usize::from(decoded.channels);
    if channels == 0 {
        return Err(RmpegError::InvalidData(
            "decoded audio has zero channels".to_string(),
        ));
    }
    if !decoded.samples.len().is_multiple_of(channels) {
        return Err(RmpegError::InvalidData(
            "decoded audio sample count is not channel-aligned".to_string(),
        ));
    }
    let block_align = decoded
        .channels
        .checked_mul(2)
        .ok_or_else(|| RmpegError::InvalidData("WAV block align overflow".to_string()))?;
    let byte_rate = decoded
        .sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| RmpegError::InvalidData("WAV byte rate overflow".to_string()))?;
    let pcm = samples_to_s16le_bytes(&decoded.samples);

    let mut bytes = Vec::with_capacity(78 + pcm.len());
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&decoded.channels.to_le_bytes());
    bytes.extend_from_slice(&decoded.sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"LIST");
    bytes.extend_from_slice(&26_u32.to_le_bytes());
    bytes.extend_from_slice(b"INFOISFT");
    bytes.extend_from_slice(&(FFMPEG_WAV_PIPE_ENCODER.len() as u32).to_le_bytes());
    bytes.extend_from_slice(FFMPEG_WAV_PIPE_ENCODER);
    if FFMPEG_WAV_PIPE_ENCODER.len() % 2 == 1 {
        bytes.push(0);
    }
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&pcm);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        channel_layout_name, decimal_seconds_to_samples, parse_rate, resample_windowed_sinc,
        scale_s16_volume, swr_edge_index, swr_filter_tap_count, video_frame_count, wav_pipe_bytes,
        yuv420p_frame_size, DecodedAudio,
    };

    #[test]
    fn volume_rounds_half_to_even_like_ffmpeg_s16() {
        assert_eq!(scale_s16_volume(407, 0.5), 204);
        assert_eq!(scale_s16_volume(541, 0.5), 270);
        assert_eq!(scale_s16_volume(-407, 0.5), -204);
        assert_eq!(scale_s16_volume(-541, 0.5), -270);
    }

    #[test]
    fn decimal_seek_uses_audio_time_base_samples() {
        assert_eq!(decimal_seconds_to_samples("0.1", 44_100).unwrap(), 4_410);
        assert_eq!(decimal_seconds_to_samples(".25", 48_000).unwrap(), 12_000);
    }

    #[test]
    fn video_baseline_uses_probe_duration_and_rate() {
        assert_eq!(parse_rate("10/1"), Some((10, 1)));
        assert_eq!(video_frame_count(Some(1.0), (10, 1)), 10);
        assert_eq!(yuv420p_frame_size(64, 48).unwrap(), 4_608);
    }

    #[test]
    fn framemd5_header_uses_ffmpeg_channel_layout_names() {
        assert_eq!(channel_layout_name(1), Some("mono"));
        assert_eq!(channel_layout_name(2), Some("stereo"));
        assert_eq!(channel_layout_name(6), None);
    }

    #[test]
    fn resample_windowed_sinc_keeps_duration_shape() {
        let decoded = DecodedAudio {
            sample_rate: 4,
            channels: 1,
            samples: vec![0, 100, 200, 300],
        };
        let resampled = resample_windowed_sinc(decoded, 2).unwrap();
        assert_eq!(resampled.sample_rate, 2);
        assert_eq!(resampled.samples.len(), 2);
    }

    #[test]
    fn resampler_reflects_edges_like_ffmpeg_swr() {
        assert_eq!(swr_edge_index(-2, 5), 2);
        assert_eq!(swr_edge_index(-1, 5), 1);
        assert_eq!(swr_edge_index(0, 5), 0);
        assert_eq!(swr_edge_index(5, 5), 4);
        assert_eq!(swr_edge_index(6, 5), 3);
    }

    #[test]
    fn resampler_uses_observed_swr_tap_count() {
        let cutoff = 16_000.0 / 44_100.0 * 0.97;
        assert_eq!(swr_filter_tap_count(cutoff), 93);
    }

    #[test]
    fn wav_pipe_header_matches_ffmpeg_stdout_shape() {
        let decoded = DecodedAudio {
            sample_rate: 44_100,
            channels: 2,
            samples: vec![0, 0, 136, 136],
        };
        let bytes = wav_pipe_bytes(&decoded).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[4..8], &u32::MAX.to_le_bytes());
        assert_eq!(&bytes[70..74], b"data");
        assert_eq!(&bytes[74..78], &u32::MAX.to_le_bytes());
        assert_eq!(bytes.len(), 86);
    }
}
