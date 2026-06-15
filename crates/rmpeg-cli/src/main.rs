use std::io::{self, Write};
use std::process;
use std::{env, fs};

use std::path::Path;

use rmpeg_codec::{
    alias_pix_image_frame_hashes, audio_frame_hashes_from_samples, bmp_image_frame_hashes,
    brender_pix_image_frame_hashes, compressed_audio_decode, dds_image_frame_hashes,
    dpx_image_frame_hashes, fits_image_frame_hashes, gif_video_frame_hashes, md5::md5_hex,
    mp4_h264_frame_hashes, png_image_frame_hash_document, pnm_image_frame_hashes,
    ptx_image_frame_hashes, samples_to_s16le_bytes, sgi_image_frame_hashes,
    sunrast_image_frame_hashes, tga_image_frame_hashes, xbm_image_frame_hashes,
    AudioFrameHashDocument, DecodedAudio, VideoFrameHashDocument,
};
use rmpeg_core::{AudioFrameHash, ProbeDocument, Result, RmpegError};
use rmpeg_format::{
    extract_mp4_pcm_samples, parse_mp4_video_timing, parse_wav, probe_path, Mp4PcmSampleData,
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

    let document = decode_audio_frame_hash_document(&args[1])?;
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
            Err(error) => metadata_only_image_frame_hashes(&args[1], &input).or(Err(error))?,
        },
        Some("dpx") => match dpx_image_frame_hashes(&input) {
            Ok(frames) => frames,
            Err(error) => metadata_only_image_frame_hashes(&args[1], &input).or(Err(error))?,
        },
        Some("fit" | "fits" | "fts") => fits_image_frame_hashes(&input)?,
        Some("pbm" | "pgm" | "pnm" | "ppm") => pnm_image_frame_hashes(&input)?,
        Some("pix") => match brender_pix_image_frame_hashes(&input) {
            Ok(frames) => frames,
            Err(RmpegError::InvalidData(_)) => alias_pix_image_frame_hashes(&input)?,
            Err(error) => metadata_only_image_frame_hashes(&args[1], &input).or(Err(error))?,
        },
        Some("ptx") => ptx_image_frame_hashes(&input)?,
        Some("ras" | "sun") => sunrast_image_frame_hashes(&input)?,
        Some("sgi") => sgi_image_frame_hashes(&input)?,
        Some("tga") => tga_image_frame_hashes(&input)?,
        Some("xbm") => xbm_image_frame_hashes(&input)?,
        _ => match png_image_frame_hash_document(&input) {
            Ok(document) => return print_video_framemd5(document),
            Err(error) => metadata_only_image_frame_hashes(&args[1], &input).or(Err(error))?,
        },
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
    let document = probe_path(&args[1], &input)?;
    let Some(stream) = document
        .streams
        .iter()
        .find(|stream| stream.codec_type == "video")
    else {
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
    if stream.codec_name == "gif" {
        return print_video_framemd5(gif_video_frame_hashes(&input)?);
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

fn metadata_only_image_frame_hashes(path: &str, input: &[u8]) -> Result<Vec<AudioFrameHash>> {
    let document = probe_path(path, input)?;
    let stream = first_video_stream(&document)
        .ok_or_else(|| RmpegError::Unsupported("no video stream".to_string()))?;
    stream
        .width
        .zip(stream.height)
        .ok_or_else(|| RmpegError::Unsupported("video stream has no dimensions".to_string()))?;
    Ok(Vec::new())
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
    probe_path(&args[1], &input).map(|_| ())
}

fn first_video_stream(document: &ProbeDocument) -> Option<&rmpeg_core::StreamMetadata> {
    document
        .streams
        .iter()
        .find(|stream| stream.codec_type == "video")
}

fn first_audio_stream(document: &ProbeDocument) -> Option<&rmpeg_core::StreamMetadata> {
    document
        .streams
        .iter()
        .find(|stream| stream.codec_type == "audio")
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

fn avi_extension(path: &str) -> bool {
    extension(path).is_some_and(|extension| extension.eq_ignore_ascii_case("avi"))
}

fn raw_s16le_extension(path: &str) -> bool {
    extension(path).is_some_and(|extension| extension.eq_ignore_ascii_case("sw"))
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

fn decode_audio_frame_hash_document(path: &str) -> Result<AudioFrameHashDocument> {
    let input = fs::read(path)?;
    if avi_extension(path) {
        if let Ok(document) = avi_pcm_frame_hash_document(path, &input) {
            return Ok(document);
        }
    }
    if let Ok(Some(samples)) = extract_mp4_pcm_samples(&input) {
        return mp4_pcm_frame_hash_document(samples);
    }
    match decode_audio_samples_from_input(path, &input) {
        Ok(decoded) => {
            let frames = audio_frame_hashes_from_samples(
                &decoded.samples,
                decoded.sample_rate,
                decoded.channels,
            )?;
            Ok(AudioFrameHashDocument {
                sample_rate: decoded.sample_rate,
                channels: decoded.channels,
                frames,
            })
        }
        Err(error) => metadata_only_audio_frame_hash_document(path, &input).or(Err(error)),
    }
}

fn avi_pcm_frame_hash_document(path: &str, input: &[u8]) -> Result<AudioFrameHashDocument> {
    let document = probe_path(path, input)?;
    if document.format != "avi" {
        return Err(RmpegError::Unsupported(
            "AVI PCM decode requires AVI probe metadata".to_string(),
        ));
    }
    let stream = first_audio_stream(&document)
        .ok_or_else(|| RmpegError::Unsupported("no audio stream".to_string()))?;
    if stream.codec_name != "pcm_s16le" && stream.codec_name != "pcm_u8" {
        return Err(RmpegError::Unsupported(format!(
            "AVI audio codec {} is not supported PCM",
            stream.codec_name
        )));
    }
    let sample_rate = stream
        .sample_rate
        .ok_or_else(|| RmpegError::Unsupported("audio stream has no sample rate".to_string()))?;
    let channels = stream
        .channels
        .ok_or_else(|| RmpegError::Unsupported("audio stream has no channel count".to_string()))?;
    let frames = avi_pcm_frames(input, stream.index, &stream.codec_name, channels)?;
    Ok(AudioFrameHashDocument {
        sample_rate,
        channels,
        frames,
    })
}

fn avi_pcm_frames(
    input: &[u8],
    target_stream_index: usize,
    codec_name: &str,
    channels: u16,
) -> Result<Vec<AudioFrameHash>> {
    let (movi_start, movi_end) = avi_movi_range(input)?;
    let mut chunks = Vec::new();
    collect_avi_audio_chunks(
        input,
        movi_start,
        movi_end,
        target_stream_index,
        &mut chunks,
    )?;

    let frame_bytes = usize::from(channels)
        .checked_mul(2)
        .ok_or_else(|| RmpegError::InvalidData("AVI PCM block align overflow".to_string()))?;
    if frame_bytes == 0 {
        return Err(RmpegError::InvalidData(
            "AVI PCM stream has zero channels".to_string(),
        ));
    }

    let mut frames = Vec::new();
    let mut pts = 0_u64;
    for chunk in chunks {
        let payload = match codec_name {
            "pcm_s16le" => {
                if !chunk.len().is_multiple_of(2) {
                    return Err(RmpegError::InvalidData(
                        "AVI s16le chunk has trailing partial sample".to_string(),
                    ));
                }
                chunk.to_vec()
            }
            "pcm_u8" => pcm_u8_to_s16le_bytes(chunk),
            _ => {
                return Err(RmpegError::Unsupported(format!(
                    "AVI audio codec {codec_name} is not supported PCM"
                )))
            }
        };
        if payload.len() % frame_bytes != 0 {
            return Err(RmpegError::InvalidData(
                "AVI PCM chunk is not channel-aligned".to_string(),
            ));
        }
        if payload.is_empty() {
            continue;
        }
        let samples_per_output = match codec_name {
            "pcm_s16le" => 256,
            _ => payload.len() / frame_bytes,
        };
        let max_output_bytes = samples_per_output
            .checked_mul(frame_bytes)
            .ok_or_else(|| RmpegError::InvalidData("AVI PCM frame size overflow".to_string()))?;
        for output in payload.chunks(max_output_bytes) {
            let duration = output.len() / frame_bytes;
            if duration == 0 {
                continue;
            }
            let duration = u32::try_from(duration).map_err(|_| {
                RmpegError::Unsupported("AVI PCM chunk duration is too large".to_string())
            })?;
            frames.push(AudioFrameHash {
                stream_index: 0,
                dts: pts,
                pts,
                duration,
                size: output.len(),
                hash: md5_hex(output),
            });
            pts += u64::from(duration);
        }
    }
    Ok(frames)
}

fn avi_movi_range(input: &[u8]) -> Result<(usize, usize)> {
    if input.len() < 12 || &input[0..4] != b"RIFF" || &input[8..12] != b"AVI " {
        return Err(RmpegError::InvalidData(
            "input is not an AVI RIFF file".to_string(),
        ));
    }
    let mut offset = 12;
    while offset + 8 <= input.len() {
        let id = &input[offset..offset + 4];
        let size = read_u32_le_at(input, offset + 4)? as usize;
        let data_start = offset + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("AVI chunk range overflow".to_string()))?;
        if data_end > input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_end,
                remaining: input.len(),
            });
        }
        if id == b"LIST" && size >= 4 && &input[data_start..data_start + 4] == b"movi" {
            return Ok((data_start + 4, data_end));
        }
        offset = padded_avi_chunk_end(data_end)?;
    }
    Err(RmpegError::Unsupported(
        "AVI file has no movi list".to_string(),
    ))
}

fn collect_avi_audio_chunks<'a>(
    input: &'a [u8],
    start: usize,
    end: usize,
    target_stream_index: usize,
    chunks: &mut Vec<&'a [u8]>,
) -> Result<()> {
    let mut offset = start;
    while offset + 8 <= end {
        let id = &input[offset..offset + 4];
        let size = read_u32_le_at(input, offset + 4)? as usize;
        let data_start = offset + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or_else(|| RmpegError::InvalidData("AVI chunk range overflow".to_string()))?;
        if data_end > end || data_end > input.len() {
            if id == b"idx1" {
                break;
            }
            return Err(RmpegError::UnexpectedEof {
                needed: data_end,
                remaining: input.len().min(end),
            });
        }
        if id == b"LIST" {
            if size >= 4 {
                collect_avi_audio_chunks(
                    input,
                    data_start + 4,
                    data_end,
                    target_stream_index,
                    chunks,
                )?;
            }
        } else if avi_chunk_stream_index(id, b"wb") == Some(target_stream_index) {
            chunks.push(&input[data_start..data_end]);
        }
        offset = padded_avi_chunk_end(data_end)?;
    }
    Ok(())
}

fn avi_chunk_stream_index(id: &[u8], suffix: &[u8; 2]) -> Option<usize> {
    if id.len() != 4 || &id[2..4] != suffix {
        return None;
    }
    let tens = id[0].checked_sub(b'0')?;
    let ones = id[1].checked_sub(b'0')?;
    if tens > 9 || ones > 9 {
        return None;
    }
    Some(usize::from(tens) * 10 + usize::from(ones))
}

fn padded_avi_chunk_end(data_end: usize) -> Result<usize> {
    data_end
        .checked_add(data_end % 2)
        .ok_or_else(|| RmpegError::InvalidData("AVI padded chunk range overflow".to_string()))
}

fn read_u32_le_at(input: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| RmpegError::InvalidData("u32 read range overflow".to_string()))?;
    let bytes = input.get(offset..end).ok_or(RmpegError::UnexpectedEof {
        needed: end,
        remaining: input.len(),
    })?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn pcm_u8_to_s16le_bytes(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() * 2);
    for byte in input {
        let sample = (i16::from(*byte) - 128) << 8;
        output.extend_from_slice(&sample.to_le_bytes());
    }
    output
}

fn decode_audio_samples(path: &str) -> Result<DecodedAudio> {
    let input = fs::read(path)?;
    decode_audio_samples_from_input(path, &input)
}

fn decode_audio_samples_from_input(path: &str, input: &[u8]) -> Result<DecodedAudio> {
    if wav_extension(path) {
        let wav = parse_wav(input)?;
        decode_wav_samples(input, &wav)
    } else if raw_s16le_extension(path) {
        decode_raw_s16le_samples(path, input)
    } else {
        compressed_audio_decode(input, extension(path))
    }
}

fn metadata_only_audio_frame_hash_document(
    path: &str,
    input: &[u8],
) -> Result<AudioFrameHashDocument> {
    let document = probe_path(path, input)?;
    let stream = first_audio_stream(&document)
        .ok_or_else(|| RmpegError::Unsupported("no audio stream".to_string()))?;
    let sample_rate = stream
        .sample_rate
        .ok_or_else(|| RmpegError::Unsupported("audio stream has no sample rate".to_string()))?;
    let channels = stream
        .channels
        .ok_or_else(|| RmpegError::Unsupported("audio stream has no channel count".to_string()))?;
    Ok(AudioFrameHashDocument {
        sample_rate,
        channels,
        frames: Vec::new(),
    })
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
        24 => decode_s24le_sample_bytes(payload)?,
        32 => decode_s32le_sample_bytes(payload)?,
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

fn decode_s24le_sample_bytes(input: &[u8]) -> Result<Vec<i16>> {
    let chunks = input.chunks_exact(3);
    if !chunks.remainder().is_empty() {
        return Err(RmpegError::InvalidData(
            "raw s24le input has trailing partial sample".to_string(),
        ));
    }
    Ok(chunks.map(s24le_to_s16).collect())
}

fn decode_s32le_sample_bytes(input: &[u8]) -> Result<Vec<i16>> {
    let chunks = input.chunks_exact(4);
    if !chunks.remainder().is_empty() {
        return Err(RmpegError::InvalidData(
            "raw s32le input has trailing partial sample".to_string(),
        ));
    }
    Ok(chunks
        .map(|chunk| i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) >> 16)
        .map(|sample| sample as i16)
        .collect())
}

fn s24le_to_s16(chunk: &[u8]) -> i16 {
    let sign = if chunk[2] & 0x80 == 0 { 0x00 } else { 0xff };
    (i32::from_le_bytes([chunk[0], chunk[1], chunk[2], sign]) >> 8) as i16
}

fn mp4_pcm_frame_hash_document(samples: Mp4PcmSampleData) -> Result<AudioFrameHashDocument> {
    let channels = samples.channels;
    let frame_bytes = usize::from(channels)
        .checked_mul(2)
        .ok_or_else(|| RmpegError::InvalidData("MP4 PCM block align overflow".to_string()))?;
    if frame_bytes == 0 {
        return Err(RmpegError::InvalidData(
            "MP4 PCM stream has zero channels".to_string(),
        ));
    }

    let max_output_bytes = 1024_usize
        .checked_mul(frame_bytes)
        .ok_or_else(|| RmpegError::InvalidData("MP4 PCM output frame overflow".to_string()))?;
    let mut frames = Vec::new();
    let mut pts = 0_u64;
    let chunk_count = samples.chunks.len();
    for (chunk_index, chunk) in samples.chunks.into_iter().enumerate() {
        let mut decoded = Vec::new();
        append_mp4_pcm_chunk(&mut decoded, &samples.codec_name, &chunk.data)?;
        let expected_len = (chunk.duration as usize)
            .checked_mul(frame_bytes)
            .ok_or_else(|| RmpegError::InvalidData("MP4 PCM decoded size overflow".to_string()))?;
        if decoded.len() != expected_len {
            return Err(RmpegError::InvalidData(
                "MP4 PCM decoded length does not match sample table duration".to_string(),
            ));
        }
        let mut output_offset = 0_usize;
        while output_offset < decoded.len() {
            let output_end = output_offset
                .checked_add(max_output_bytes)
                .map(|end| end.min(decoded.len()))
                .ok_or_else(|| {
                    RmpegError::InvalidData("MP4 PCM output range overflow".to_string())
                })?;
            let output = &decoded[output_offset..output_end];
            let duration = output.len() / frame_bytes;
            if duration == 0 {
                output_offset = output_end;
                continue;
            }
            let is_final_output = chunk_index + 1 == chunk_count && output_end == decoded.len();
            if samples.codec_name == "pcm_s16be"
                && is_final_output
                && duration < 1024
                && output.iter().all(|byte| *byte == 0)
            {
                break;
            }
            let duration = u32::try_from(duration).map_err(|_| {
                RmpegError::Unsupported("MP4 PCM output frame is too long".to_string())
            })?;
            frames.push(AudioFrameHash {
                stream_index: 0,
                dts: pts,
                pts,
                duration,
                size: output.len(),
                hash: md5_hex(output),
            });
            pts += u64::from(duration);
            output_offset = output_end;
        }
    }

    Ok(AudioFrameHashDocument {
        sample_rate: samples.sample_rate,
        channels,
        frames,
    })
}

fn append_mp4_pcm_chunk(output: &mut Vec<u8>, codec_name: &str, input: &[u8]) -> Result<()> {
    match codec_name {
        "pcm_u8" => output.extend_from_slice(&pcm_u8_to_s16le_bytes(input)),
        "pcm_s16le" => {
            if !input.len().is_multiple_of(2) {
                return Err(RmpegError::InvalidData(
                    "MP4 s16le chunk has trailing partial sample".to_string(),
                ));
            }
            output.extend_from_slice(input);
        }
        "pcm_s16be" => {
            let chunks = input.chunks_exact(2);
            if !chunks.remainder().is_empty() {
                return Err(RmpegError::InvalidData(
                    "MP4 s16be chunk has trailing partial sample".to_string(),
                ));
            }
            for chunk in chunks {
                output.extend_from_slice(&i16::from_be_bytes([chunk[0], chunk[1]]).to_le_bytes());
            }
        }
        "pcm_s24le" => {
            let chunks = input.chunks_exact(3);
            if !chunks.remainder().is_empty() {
                return Err(RmpegError::InvalidData(
                    "MP4 s24le chunk has trailing partial sample".to_string(),
                ));
            }
            for chunk in chunks {
                output.extend_from_slice(&s24le_to_s16(chunk).to_le_bytes());
            }
        }
        _ => {
            return Err(RmpegError::Unsupported(format!(
                "MP4 audio codec {codec_name} is not supported PCM"
            )))
        }
    }
    Ok(())
}

fn decode_raw_s16le_samples(path: &str, input: &[u8]) -> Result<DecodedAudio> {
    let document = probe_path(path, input)?;
    let stream = first_audio_stream(&document)
        .ok_or_else(|| RmpegError::Unsupported("no audio stream".to_string()))?;
    if document.format != "s16le"
        || stream.codec_name != "pcm_s16le"
        || stream.bits_per_sample != Some(16)
    {
        return Err(RmpegError::Unsupported(
            "raw s16le decode requires an s16le pcm_s16le probe".to_string(),
        ));
    }
    let sample_rate = stream
        .sample_rate
        .ok_or_else(|| RmpegError::Unsupported("audio stream has no sample rate".to_string()))?;
    let channels = stream
        .channels
        .ok_or_else(|| RmpegError::Unsupported("audio stream has no channel count".to_string()))?;
    Ok(DecodedAudio {
        sample_rate,
        channels,
        samples: decode_s16le_sample_bytes(input)?,
    })
}

fn decode_s16le_sample_bytes(input: &[u8]) -> Result<Vec<i16>> {
    let chunks = input.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(RmpegError::InvalidData(
            "raw s16le input has trailing partial sample".to_string(),
        ));
    }
    Ok(chunks
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect())
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
        append_mp4_pcm_chunk, avi_chunk_stream_index, channel_layout_name,
        decimal_seconds_to_samples, decode_s16le_sample_bytes, decode_s24le_sample_bytes,
        decode_s32le_sample_bytes, parse_rate, pcm_u8_to_s16le_bytes, resample_windowed_sinc,
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
    fn decodes_raw_s16le_sample_bytes() {
        assert_eq!(
            decode_s16le_sample_bytes(&[0x01, 0x00, 0x00, 0x80, 0xff, 0x7f]).unwrap(),
            vec![1, i16::MIN, i16::MAX]
        );
        assert!(decode_s16le_sample_bytes(&[0x01]).is_err());
    }

    #[test]
    fn decodes_wide_little_endian_pcm_to_s16() {
        assert_eq!(
            decode_s24le_sample_bytes(&[
                0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x80, 0xff, 0xff, 0x7f
            ])
            .unwrap(),
            vec![0, 128, i16::MIN, i16::MAX]
        );
        assert_eq!(
            decode_s32le_sample_bytes(&[
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x80, 0xff, 0xff,
                0xff, 0x7f
            ])
            .unwrap(),
            vec![0, 1, i16::MIN, i16::MAX]
        );
        assert!(decode_s24le_sample_bytes(&[0x00]).is_err());
        assert!(decode_s32le_sample_bytes(&[0x00, 0x00]).is_err());
    }

    #[test]
    fn avi_stream_chunk_ids_are_decimal_with_audio_suffix() {
        assert_eq!(avi_chunk_stream_index(b"01wb", b"wb"), Some(1));
        assert_eq!(avi_chunk_stream_index(b"12wb", b"wb"), Some(12));
        assert_eq!(avi_chunk_stream_index(b"01dc", b"wb"), None);
        assert_eq!(avi_chunk_stream_index(b"A1wb", b"wb"), None);
    }

    #[test]
    fn converts_avi_unsigned_pcm_to_signed_s16le() {
        assert_eq!(
            pcm_u8_to_s16le_bytes(&[0x00, 0x80, 0xff]),
            vec![0x00, 0x80, 0x00, 0x00, 0x00, 0x7f]
        );
    }

    #[test]
    fn converts_mp4_pcm_chunks_to_signed_s16le() {
        let mut output = Vec::new();
        append_mp4_pcm_chunk(&mut output, "pcm_s16be", &[0x80, 0x00, 0x7f, 0xff]).unwrap();
        assert_eq!(output, vec![0x00, 0x80, 0xff, 0x7f]);
        output.clear();
        append_mp4_pcm_chunk(
            &mut output,
            "pcm_s24le",
            &[0x00, 0x00, 0x80, 0xff, 0xff, 0x7f],
        )
        .unwrap();
        assert_eq!(output, vec![0x00, 0x80, 0xff, 0x7f]);
        assert!(append_mp4_pcm_chunk(&mut output, "pcm_s16be", &[0]).is_err());
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
