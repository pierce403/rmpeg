use std::io::Cursor;

use opus_decoder::OpusDecoder;
use rmpeg_core::{AudioFrameHash, Result, RmpegError};
use rmpeg_format::probe;
use symphonia::core::audio::sample::Sample;
use symphonia::core::audio::{Audio, GenericAudioBufferRef};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::TrackType;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::md5::md5_hex;
use crate::pcm::wav_framemd5_samples_per_frame;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrameHashDocument {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: Vec<AudioFrameHash>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}

pub fn compressed_audio_frame_hashes(
    input: &[u8],
    extension: Option<&str>,
) -> Result<AudioFrameHashDocument> {
    let decoded = compressed_audio_decode(input, extension)?;
    let frames =
        audio_frame_hashes_from_samples(&decoded.samples, decoded.sample_rate, decoded.channels)?;

    Ok(AudioFrameHashDocument {
        sample_rate: decoded.sample_rate,
        channels: decoded.channels,
        frames,
    })
}

pub fn compressed_audio_decode(input: &[u8], extension: Option<&str>) -> Result<DecodedAudio> {
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("ogg"))
        && input
            .windows(b"OpusHead".len())
            .any(|window| window == b"OpusHead")
    {
        return decode_ogg_opus(input);
    }
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("mp3")) {
        if let Ok(decoded) = decode_mp3_system(input) {
            return Ok(decoded);
        }
    }

    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }

    let source = Box::new(Cursor::new(input.to_vec()));
    let media = MediaSourceStream::new(source, Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            media,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(map_symphonia_error)?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| RmpegError::Unsupported("no decodable audio track".to_string()))?;
    let track_id = track.id;
    let codec_params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| RmpegError::Unsupported("missing audio codec parameters".to_string()))?;

    let mut decoder = codec_registry()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())
        .map_err(map_symphonia_error)?;

    let mut decoded = Vec::new();
    let mut sample_rate = None;
    let mut channels = None;
    let mut converted = Vec::new();

    while let Some(packet) = format.next_packet().map_err(map_symphonia_error)? {
        if packet.track_id != track_id {
            continue;
        }

        let audio = match decoder.decode(&packet) {
            Ok(audio) => audio,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(map_symphonia_error(error)),
        };

        let spec = audio.spec();
        let packet_sample_rate = spec.rate();
        let packet_channels = channel_count(spec.channels().count())?;
        match sample_rate {
            Some(existing) if existing != packet_sample_rate => {
                return Err(RmpegError::Unsupported(
                    "audio sample-rate changes are not supported".to_string(),
                ));
            }
            None => sample_rate = Some(packet_sample_rate),
            _ => {}
        }
        match channels {
            Some(existing) if existing != packet_channels => {
                return Err(RmpegError::Unsupported(
                    "audio channel-count changes are not supported".to_string(),
                ));
            }
            None => channels = Some(packet_channels),
            _ => {}
        }

        copy_audio_to_interleaved_s16(audio, &mut converted);
        decoded.extend_from_slice(&converted);
    }

    let sample_rate = sample_rate.ok_or_else(|| {
        RmpegError::InvalidData("audio stream did not produce decoded frames".to_string())
    })?;
    let channels = channels.ok_or_else(|| {
        RmpegError::InvalidData("audio stream did not report channel count".to_string())
    })?;
    trim_mp4_aac_padding(input, extension, sample_rate, channels, &mut decoded);
    trim_ogg_vorbis_output(input, extension, channels, &mut decoded);
    Ok(DecodedAudio {
        sample_rate,
        channels,
        samples: decoded,
    })
}

pub fn audio_frame_hashes_from_samples(
    samples: &[i16],
    sample_rate: u32,
    channels: u16,
) -> Result<Vec<AudioFrameHash>> {
    let samples_per_frame = wav_framemd5_samples_per_frame(sample_rate)?;
    let channel_count = usize::from(channels);
    if channel_count == 0 {
        return Err(RmpegError::InvalidData(
            "decoded audio has zero channels".to_string(),
        ));
    }
    if !samples.len().is_multiple_of(channel_count) {
        return Err(RmpegError::InvalidData(
            "decoded audio sample count is not channel-aligned".to_string(),
        ));
    }

    let block_align = channel_count
        .checked_mul(2)
        .ok_or_else(|| RmpegError::InvalidData("decoded audio block align overflow".to_string()))?;
    let total_samples = samples.len() / channel_count;
    let mut frames = Vec::new();
    let mut pts = 0_u64;
    while (pts as usize) < total_samples {
        let remaining_samples = total_samples - pts as usize;
        let duration = remaining_samples.min(samples_per_frame as usize) as u32;
        let sample_offset = pts as usize * channel_count;
        let sample_len = duration as usize * channel_count;
        let payload = samples_to_s16le_bytes(&samples[sample_offset..sample_offset + sample_len]);
        frames.push(AudioFrameHash {
            stream_index: 0,
            dts: pts,
            pts,
            duration,
            size: duration as usize * block_align,
            hash: md5_hex(&payload),
        });
        pts += u64::from(duration);
    }

    Ok(frames)
}

fn copy_audio_to_interleaved_s16(audio: GenericAudioBufferRef<'_>, converted: &mut Vec<i16>) {
    converted.resize(audio.samples_interleaved(), i16::MID);
    match audio {
        GenericAudioBufferRef::F32(buffer) => {
            for (dst, sample) in converted.iter_mut().zip(buffer.iter_interleaved()) {
                *dst = ffmpeg_s16_from_f32(sample);
            }
        }
        GenericAudioBufferRef::F64(buffer) => {
            for (dst, sample) in converted.iter_mut().zip(buffer.iter_interleaved()) {
                *dst = ffmpeg_s16_from_f64(sample);
            }
        }
        audio => audio.copy_to_slice_interleaved(converted),
    }
}

#[cfg(unix)]
fn decode_mp3_system(input: &[u8]) -> Result<DecodedAudio> {
    let mpg = SystemMpg123::open()?;
    let decoder = mpg.create_decoder()?;
    decoder.force_float_output()?;
    decoder.open_feed()?;
    decoder.feed(input)?;

    let mut sample_rate = None;
    let mut channels = None;
    let mut output_encoding = None;
    let mut pcm = Vec::new();
    let mut buffer = vec![0_u8; 32_768];
    loop {
        let mut done = 0_usize;
        let status = unsafe {
            (mpg.decode)(
                decoder.as_ptr(),
                std::ptr::null(),
                0,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut done,
            )
        };
        if done > 0 {
            pcm.extend_from_slice(&buffer[..done]);
        }
        match status {
            MPG123_OK => {}
            MPG123_NEW_FORMAT => {
                let (rate, channel_count, encoding) = decoder.format()?;
                if !matches!(
                    encoding,
                    MPG123_ENC_SIGNED_16 | MPG123_ENC_FLOAT_32 | MPG123_ENC_FLOAT_64
                ) {
                    return Err(RmpegError::Unsupported(format!(
                        "system mpg123 output encoding 0x{encoding:x} is not supported"
                    )));
                }
                sample_rate = Some(rate);
                channels = Some(channel_count);
                output_encoding = Some(encoding);
            }
            MPG123_NEED_MORE | MPG123_DONE => break,
            other => {
                return Err(RmpegError::InvalidData(format!(
                    "system mpg123 decode failed with code {other}"
                )));
            }
        }
    }

    let sample_rate = sample_rate.ok_or_else(|| {
        RmpegError::InvalidData("MP3 stream did not report sample rate".to_string())
    })?;
    let channels = channels.ok_or_else(|| {
        RmpegError::InvalidData("MP3 stream did not report channel count".to_string())
    })?;
    let output_encoding = output_encoding.ok_or_else(|| {
        RmpegError::InvalidData("MP3 stream did not report output encoding".to_string())
    })?;
    let samples = mpg123_output_to_s16(&pcm, output_encoding)?;
    Ok(DecodedAudio {
        sample_rate,
        channels,
        samples,
    })
}

#[cfg(not(unix))]
fn decode_mp3_system(_input: &[u8]) -> Result<DecodedAudio> {
    Err(RmpegError::Unsupported(
        "system mpg123 loading is only supported on Unix".to_string(),
    ))
}

#[cfg(unix)]
fn mpg123_output_to_s16(pcm: &[u8], encoding: libc::c_int) -> Result<Vec<i16>> {
    match encoding {
        MPG123_ENC_SIGNED_16 => {
            if !pcm.len().is_multiple_of(2) {
                return Err(RmpegError::InvalidData(
                    "system mpg123 produced odd-sized s16 output".to_string(),
                ));
            }
            Ok(pcm
                .chunks_exact(2)
                .map(|bytes| i16::from_ne_bytes([bytes[0], bytes[1]]))
                .collect())
        }
        MPG123_ENC_FLOAT_32 => {
            if !pcm.len().is_multiple_of(4) {
                return Err(RmpegError::InvalidData(
                    "system mpg123 produced truncated f32 output".to_string(),
                ));
            }
            Ok(pcm
                .chunks_exact(4)
                .map(|bytes| {
                    ffmpeg_s16_from_f32(f32::from_ne_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3],
                    ]))
                })
                .collect())
        }
        MPG123_ENC_FLOAT_64 => {
            if !pcm.len().is_multiple_of(8) {
                return Err(RmpegError::InvalidData(
                    "system mpg123 produced truncated f64 output".to_string(),
                ));
            }
            Ok(pcm
                .chunks_exact(8)
                .map(|bytes| {
                    ffmpeg_s16_from_f64(f64::from_ne_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6],
                        bytes[7],
                    ]))
                })
                .collect())
        }
        _ => Err(RmpegError::Unsupported(format!(
            "system mpg123 output encoding 0x{encoding:x} is not supported"
        ))),
    }
}

fn ffmpeg_s16_from_f32(sample: f32) -> i16 {
    let scaled = sample.clamp(-1.0, 1.0) * 32_768.0;
    if scaled <= f32::from(i16::MIN) {
        i16::MIN
    } else if scaled >= f32::from(i16::MAX) {
        i16::MAX
    } else {
        scaled.round_ties_even() as i16
    }
}

fn ffmpeg_s16_from_f64(sample: f64) -> i16 {
    let scaled = sample.clamp(-1.0, 1.0) * 32_768.0;
    if scaled <= f64::from(i16::MIN) {
        i16::MIN
    } else if scaled >= f64::from(i16::MAX) {
        i16::MAX
    } else {
        scaled.round_ties_even() as i16
    }
}

fn decode_ogg_opus(input: &[u8]) -> Result<DecodedAudio> {
    let packets = ogg_packets(input)?;
    let head = packets
        .iter()
        .find(|packet| packet.starts_with(b"OpusHead"))
        .ok_or_else(|| RmpegError::InvalidData("Ogg Opus stream has no OpusHead".to_string()))?;
    if head.len() < 19 {
        return Err(RmpegError::InvalidData(
            "Ogg OpusHead packet is truncated".to_string(),
        ));
    }
    let channels = head[9];
    if channels == 0 {
        return Err(RmpegError::InvalidData(
            "Ogg Opus stream has zero channels".to_string(),
        ));
    }
    let pre_skip = usize::from(u16::from_le_bytes([head[10], head[11]]));
    let sample_rate = 48_000_u32;
    let mut decoded = decode_ogg_opus_system(&packets, sample_rate, channels)
        .or_else(|_| decode_ogg_opus_rust(&packets, sample_rate, channels))?;

    trim_opus_output(input, sample_rate, channels, pre_skip, &mut decoded);
    Ok(DecodedAudio {
        sample_rate,
        channels: u16::from(channels),
        samples: decoded,
    })
}

fn decode_ogg_opus_rust(packets: &[Vec<u8>], sample_rate: u32, channels: u8) -> Result<Vec<i16>> {
    let mut decoder = OpusDecoder::new(sample_rate, usize::from(channels))
        .map_err(|error| RmpegError::Unsupported(format!("opus decoder: {error}")))?;
    let mut decoded = Vec::new();
    let mut buffer = vec![0_i16; OpusDecoder::MAX_FRAME_SIZE_48K * usize::from(channels)];

    for packet in opus_audio_packets(packets) {
        let frames = decoder
            .decode(packet, &mut buffer, false)
            .map_err(|error| RmpegError::InvalidData(format!("Opus decode failed: {error}")))?;
        decoded.extend_from_slice(&buffer[..frames * usize::from(channels)]);
    }

    Ok(decoded)
}

#[cfg(unix)]
fn decode_ogg_opus_system(packets: &[Vec<u8>], sample_rate: u32, channels: u8) -> Result<Vec<i16>> {
    if channels > 2 {
        return Err(RmpegError::Unsupported(
            "system libopus single-stream decoder only supports mono/stereo".to_string(),
        ));
    }

    let opus = SystemOpus::open()?;
    let decoder = opus.create_decoder(sample_rate, channels)?;
    let max_frame_size = 5_760_usize;
    let channel_count = usize::from(channels);
    let mut decoded = Vec::new();
    let mut buffer = vec![0_f32; max_frame_size * channel_count];
    let max_frame_size = libc::c_int::try_from(max_frame_size)
        .map_err(|_| RmpegError::Unsupported("Opus frame size is too large".to_string()))?;

    for packet in opus_audio_packets(packets) {
        let frames = decoder.decode_float(packet, &mut buffer, max_frame_size)?;
        decoded.extend(
            buffer[..frames * channel_count]
                .iter()
                .copied()
                .map(ffmpeg_s16_from_f32),
        );
    }

    Ok(decoded)
}

#[cfg(not(unix))]
fn decode_ogg_opus_system(
    _packets: &[Vec<u8>],
    _sample_rate: u32,
    _channels: u8,
) -> Result<Vec<i16>> {
    Err(RmpegError::Unsupported(
        "system libopus loading is only supported on Unix".to_string(),
    ))
}

fn opus_audio_packets(packets: &[Vec<u8>]) -> impl Iterator<Item = &[u8]> {
    packets
        .iter()
        .map(Vec::as_slice)
        .filter(|packet| !packet.starts_with(b"OpusHead") && !packet.starts_with(b"OpusTags"))
}

#[cfg(unix)]
const MPG123_OK: libc::c_int = 0;
#[cfg(unix)]
const MPG123_NEED_MORE: libc::c_int = -10;
#[cfg(unix)]
const MPG123_NEW_FORMAT: libc::c_int = -11;
#[cfg(unix)]
const MPG123_DONE: libc::c_int = -12;
#[cfg(unix)]
const MPG123_ADD_FLAGS: libc::c_int = 2;
#[cfg(unix)]
const MPG123_FORCE_FLOAT: libc::c_long = 0x400;
#[cfg(unix)]
const MPG123_MONO: libc::c_int = 0x1;
#[cfg(unix)]
const MPG123_STEREO: libc::c_int = 0x2;
#[cfg(unix)]
const MPG123_ENC_SIGNED_16: libc::c_int = 0x0d0;
#[cfg(unix)]
const MPG123_ENC_FLOAT_32: libc::c_int = 0x200;
#[cfg(unix)]
const MPG123_ENC_FLOAT_64: libc::c_int = 0x400;

#[cfg(unix)]
struct SystemMpg123 {
    handle: *mut libc::c_void,
    exit: Mpg123Exit,
    new: Mpg123New,
    delete: Mpg123Delete,
    param: Mpg123Param,
    format_none: Mpg123FormatNone,
    format2: Mpg123Format,
    open_feed: Mpg123OpenFeed,
    feed: Mpg123Feed,
    decode: Mpg123Decode,
    getformat: Mpg123Getformat,
}

#[cfg(unix)]
type Mpg123Init = unsafe extern "C" fn() -> libc::c_int;
#[cfg(unix)]
type Mpg123Exit = unsafe extern "C" fn();
#[cfg(unix)]
type Mpg123New = unsafe extern "C" fn(*const libc::c_char, *mut libc::c_int) -> *mut libc::c_void;
#[cfg(unix)]
type Mpg123Delete = unsafe extern "C" fn(*mut libc::c_void);
#[cfg(unix)]
type Mpg123Param = unsafe extern "C" fn(
    *mut libc::c_void,
    libc::c_int,
    libc::c_long,
    libc::c_double,
) -> libc::c_int;
#[cfg(unix)]
type Mpg123FormatNone = unsafe extern "C" fn(*mut libc::c_void) -> libc::c_int;
#[cfg(unix)]
type Mpg123Format =
    unsafe extern "C" fn(*mut libc::c_void, libc::c_long, libc::c_int, libc::c_int) -> libc::c_int;
#[cfg(unix)]
type Mpg123OpenFeed = unsafe extern "C" fn(*mut libc::c_void) -> libc::c_int;
#[cfg(unix)]
type Mpg123Feed = unsafe extern "C" fn(*mut libc::c_void, *const u8, usize) -> libc::c_int;
#[cfg(unix)]
type Mpg123Decode = unsafe extern "C" fn(
    *mut libc::c_void,
    *const u8,
    usize,
    *mut u8,
    usize,
    *mut usize,
) -> libc::c_int;
#[cfg(unix)]
type Mpg123Getformat = unsafe extern "C" fn(
    *mut libc::c_void,
    *mut libc::c_long,
    *mut libc::c_int,
    *mut libc::c_int,
) -> libc::c_int;

#[cfg(unix)]
impl SystemMpg123 {
    fn open() -> Result<Self> {
        let handle = unsafe { libc::dlopen(c"libmpg123.so.0".as_ptr(), libc::RTLD_NOW) };
        if handle.is_null() {
            return Err(RmpegError::Unsupported(
                "system libmpg123.so.0 is not available".to_string(),
            ));
        }

        let init: Mpg123Init = unsafe { load_symbol(handle, c"mpg123_init".to_bytes_with_nul())? };
        let exit = unsafe { load_symbol(handle, c"mpg123_exit".to_bytes_with_nul())? };
        let new = unsafe { load_symbol(handle, c"mpg123_new".to_bytes_with_nul())? };
        let delete = unsafe { load_symbol(handle, c"mpg123_delete".to_bytes_with_nul())? };
        let param = unsafe { load_symbol(handle, c"mpg123_param".to_bytes_with_nul())? };
        let format_none =
            unsafe { load_symbol(handle, c"mpg123_format_none".to_bytes_with_nul())? };
        let format2 = unsafe { load_symbol(handle, c"mpg123_format2".to_bytes_with_nul())? };
        let open_feed = unsafe { load_symbol(handle, c"mpg123_open_feed".to_bytes_with_nul())? };
        let feed = unsafe { load_symbol(handle, c"mpg123_feed".to_bytes_with_nul())? };
        let decode = unsafe { load_symbol(handle, c"mpg123_decode".to_bytes_with_nul())? };
        let getformat = unsafe { load_symbol(handle, c"mpg123_getformat".to_bytes_with_nul())? };
        let status = unsafe { init() };
        if status != MPG123_OK {
            return Err(RmpegError::Unsupported(format!(
                "system mpg123 init failed with code {status}"
            )));
        }

        Ok(Self {
            handle,
            exit,
            new,
            delete,
            param,
            format_none,
            format2,
            open_feed,
            feed,
            decode,
            getformat,
        })
    }

    fn create_decoder(&self) -> Result<SystemMpg123Decoder<'_>> {
        let mut error = 0;
        let ptr = unsafe { (self.new)(std::ptr::null(), &mut error) };
        if ptr.is_null() || error != MPG123_OK {
            return Err(RmpegError::Unsupported(format!(
                "system mpg123 decoder create failed with code {error}"
            )));
        }
        Ok(SystemMpg123Decoder { mpg: self, ptr })
    }
}

#[cfg(unix)]
impl Drop for SystemMpg123 {
    fn drop(&mut self) {
        unsafe {
            (self.exit)();
            libc::dlclose(self.handle);
        }
    }
}

#[cfg(unix)]
struct SystemMpg123Decoder<'a> {
    mpg: &'a SystemMpg123,
    ptr: *mut libc::c_void,
}

#[cfg(unix)]
impl SystemMpg123Decoder<'_> {
    fn as_ptr(&self) -> *mut libc::c_void {
        self.ptr
    }

    fn force_float_output(&self) -> Result<()> {
        let status = unsafe { (self.mpg.format_none)(self.ptr) };
        if status != MPG123_OK {
            return Err(RmpegError::Unsupported(format!(
                "system mpg123 format_none failed with code {status}"
            )));
        }
        let status = unsafe {
            (self.mpg.format2)(
                self.ptr,
                0,
                MPG123_MONO | MPG123_STEREO,
                MPG123_ENC_FLOAT_32,
            )
        };
        if status != MPG123_OK {
            return Err(RmpegError::Unsupported(format!(
                "system mpg123 float format setup failed with code {status}"
            )));
        }
        self.add_flags(MPG123_FORCE_FLOAT)
    }

    fn add_flags(&self, flags: libc::c_long) -> Result<()> {
        let status = unsafe { (self.mpg.param)(self.ptr, MPG123_ADD_FLAGS, flags, 0.0) };
        if status == MPG123_OK {
            Ok(())
        } else {
            Err(RmpegError::Unsupported(format!(
                "system mpg123 add_flags failed with code {status}"
            )))
        }
    }

    fn open_feed(&self) -> Result<()> {
        let status = unsafe { (self.mpg.open_feed)(self.ptr) };
        if status == MPG123_OK {
            Ok(())
        } else {
            Err(RmpegError::Unsupported(format!(
                "system mpg123 open_feed failed with code {status}"
            )))
        }
    }

    fn feed(&self, input: &[u8]) -> Result<()> {
        let status = unsafe { (self.mpg.feed)(self.ptr, input.as_ptr(), input.len()) };
        if status == MPG123_OK {
            Ok(())
        } else {
            Err(RmpegError::InvalidData(format!(
                "system mpg123 feed failed with code {status}"
            )))
        }
    }

    fn format(&self) -> Result<(u32, u16, libc::c_int)> {
        let mut rate: libc::c_long = 0;
        let mut channels: libc::c_int = 0;
        let mut encoding: libc::c_int = 0;
        let status =
            unsafe { (self.mpg.getformat)(self.ptr, &mut rate, &mut channels, &mut encoding) };
        if status != MPG123_OK {
            return Err(RmpegError::InvalidData(format!(
                "system mpg123 getformat failed with code {status}"
            )));
        }
        let rate = u32::try_from(rate)
            .map_err(|_| RmpegError::Unsupported("MP3 sample rate is too large".to_string()))?;
        let channels = u16::try_from(channels)
            .map_err(|_| RmpegError::Unsupported("MP3 channel count is too large".to_string()))?;
        if rate == 0 || channels == 0 {
            return Err(RmpegError::InvalidData(
                "system mpg123 reported empty output format".to_string(),
            ));
        }
        Ok((rate, channels, encoding))
    }
}

#[cfg(unix)]
impl Drop for SystemMpg123Decoder<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.mpg.delete)(self.ptr);
        }
    }
}

#[cfg(unix)]
struct SystemOpus {
    handle: *mut libc::c_void,
    create: OpusDecoderCreate,
    decode_float: OpusDecodeFloat,
    destroy: OpusDecoderDestroy,
}

#[cfg(unix)]
type OpusDecoderCreate =
    unsafe extern "C" fn(libc::c_int, libc::c_int, *mut libc::c_int) -> *mut libc::c_void;
type OpusDecodeFloat = unsafe extern "C" fn(
    *mut libc::c_void,
    *const u8,
    libc::c_int,
    *mut f32,
    libc::c_int,
    libc::c_int,
) -> libc::c_int;
#[cfg(unix)]
type OpusDecoderDestroy = unsafe extern "C" fn(*mut libc::c_void);

#[cfg(unix)]
impl SystemOpus {
    fn open() -> Result<Self> {
        let handle = unsafe { libc::dlopen(c"libopus.so.0".as_ptr(), libc::RTLD_NOW) };
        if handle.is_null() {
            return Err(RmpegError::Unsupported(
                "system libopus.so.0 is not available".to_string(),
            ));
        }

        let create = unsafe { load_symbol(handle, c"opus_decoder_create".to_bytes_with_nul())? };
        let decode_float =
            unsafe { load_symbol(handle, c"opus_decode_float".to_bytes_with_nul())? };
        let destroy = unsafe { load_symbol(handle, c"opus_decoder_destroy".to_bytes_with_nul())? };
        Ok(Self {
            handle,
            create,
            decode_float,
            destroy,
        })
    }

    fn create_decoder(&self, sample_rate: u32, channels: u8) -> Result<SystemOpusDecoder> {
        let sample_rate = libc::c_int::try_from(sample_rate)
            .map_err(|_| RmpegError::Unsupported("Opus sample rate is too large".to_string()))?;
        let channels = libc::c_int::from(channels);
        let mut error = 0;
        let ptr = unsafe { (self.create)(sample_rate, channels, &mut error) };
        if ptr.is_null() || error != 0 {
            return Err(RmpegError::Unsupported(format!(
                "system libopus decoder create failed with code {error}"
            )));
        }
        Ok(SystemOpusDecoder {
            ptr,
            decode_float: self.decode_float,
            destroy: self.destroy,
        })
    }
}

#[cfg(unix)]
impl Drop for SystemOpus {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self.handle);
        }
    }
}

#[cfg(unix)]
struct SystemOpusDecoder {
    ptr: *mut libc::c_void,
    decode_float: OpusDecodeFloat,
    destroy: OpusDecoderDestroy,
}

#[cfg(unix)]
impl SystemOpusDecoder {
    fn decode_float(
        &self,
        packet: &[u8],
        buffer: &mut [f32],
        max_frame_size: libc::c_int,
    ) -> Result<usize> {
        let packet_len = libc::c_int::try_from(packet.len())
            .map_err(|_| RmpegError::Unsupported("Opus packet is too large".to_string()))?;
        let frames = unsafe {
            (self.decode_float)(
                self.ptr,
                packet.as_ptr(),
                packet_len,
                buffer.as_mut_ptr(),
                max_frame_size,
                0,
            )
        };
        if frames < 0 {
            return Err(RmpegError::InvalidData(format!(
                "system libopus float decode failed with code {frames}"
            )));
        }
        usize::try_from(frames)
            .map_err(|_| RmpegError::Unsupported("Opus frame count is too large".to_string()))
    }
}

#[cfg(unix)]
impl Drop for SystemOpusDecoder {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.ptr);
        }
    }
}

#[cfg(unix)]
unsafe fn load_symbol<T>(handle: *mut libc::c_void, name: &[u8]) -> Result<T> {
    let symbol = unsafe { libc::dlsym(handle, name.as_ptr().cast()) };
    if symbol.is_null() {
        return Err(RmpegError::Unsupported(format!(
            "system libopus is missing symbol {}",
            String::from_utf8_lossy(name.strip_suffix(b"\0").unwrap_or(name))
        )));
    }
    Ok(unsafe { std::mem::transmute_copy(&symbol) })
}

fn ogg_packets(input: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut packets = Vec::new();
    let mut packet = Vec::new();
    let mut pos = 0_usize;
    while pos < input.len() {
        if input.get(pos..pos + 4) != Some(b"OggS") {
            return Err(RmpegError::InvalidData(
                "missing Ogg page capture pattern".to_string(),
            ));
        }
        if pos + 27 > input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 27,
                remaining: input.len(),
            });
        }
        let segment_count = usize::from(input[pos + 26]);
        let segment_table = pos + 27;
        let data_start = segment_table + segment_count;
        if data_start > input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_start,
                remaining: input.len(),
            });
        }
        let data_len = input[segment_table..data_start]
            .iter()
            .map(|value| usize::from(*value))
            .sum::<usize>();
        let data_end = data_start
            .checked_add(data_len)
            .ok_or_else(|| RmpegError::InvalidData("Ogg page segment data overflow".to_string()))?;
        if data_end > input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_end,
                remaining: input.len(),
            });
        }

        let mut data_pos = data_start;
        for segment_len in &input[segment_table..data_start] {
            let segment_len = usize::from(*segment_len);
            packet.extend_from_slice(&input[data_pos..data_pos + segment_len]);
            data_pos += segment_len;
            if segment_len < 255 {
                packets.push(std::mem::take(&mut packet));
            }
        }
        pos = data_end;
    }

    if !packet.is_empty() {
        return Err(RmpegError::InvalidData(
            "Ogg stream ended with an incomplete packet".to_string(),
        ));
    }
    Ok(packets)
}

fn trim_opus_output(
    input: &[u8],
    sample_rate: u32,
    channels: u8,
    pre_skip: usize,
    samples: &mut Vec<i16>,
) {
    let channels = usize::from(channels);
    if channels == 0 {
        return;
    }
    let skip_samples = pre_skip.saturating_mul(channels).min(samples.len());
    samples.drain(..skip_samples);

    let Ok(document) = probe(input) else {
        return;
    };
    let Some(duration_seconds) = document
        .streams
        .iter()
        .find(|stream| stream.codec_type == "audio" && stream.codec_name == "opus")
        .and_then(|stream| stream.duration_seconds)
    else {
        return;
    };
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return;
    }
    let duration_samples = (duration_seconds * f64::from(sample_rate)).round() as usize;
    let target_frames = duration_samples.saturating_sub(pre_skip);
    let target_samples = target_frames.saturating_mul(channels);
    if samples.len() > target_samples {
        samples.truncate(target_samples);
    }
}

fn trim_mp4_aac_padding(
    input: &[u8],
    extension: Option<&str>,
    sample_rate: u32,
    channels: u16,
    samples: &mut Vec<i16>,
) {
    if !extension.is_some_and(|extension| extension.eq_ignore_ascii_case("mp4")) {
        return;
    }
    let Ok(document) = probe(input) else {
        return;
    };
    let Some(stream) = document
        .streams
        .iter()
        .find(|stream| stream.codec_type == "audio" && stream.codec_name == "aac")
    else {
        return;
    };
    let Some(duration_seconds) = stream.duration_seconds else {
        return;
    };
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return;
    }

    let duration_samples = (duration_seconds * f64::from(sample_rate)).ceil() as usize;
    let Some(target_frames) = ceil_to_frame_boundary(duration_samples, 1024) else {
        return;
    };
    trim_aac_priming_and_padding(samples, channels, target_frames);
}

fn trim_aac_priming_and_padding(samples: &mut Vec<i16>, channels: u16, target_frames: usize) {
    let target_samples = target_frames.saturating_mul(usize::from(channels));
    let frame_samples = 1024_usize.saturating_mul(usize::from(channels));
    if samples.len() >= target_samples.saturating_add(frame_samples) {
        samples.drain(..frame_samples);
    }
    if target_samples < samples.len() {
        samples.truncate(target_samples);
    }
}

fn trim_ogg_vorbis_output(
    input: &[u8],
    extension: Option<&str>,
    channels: u16,
    samples: &mut Vec<i16>,
) {
    if !extension.is_some_and(|extension| extension.eq_ignore_ascii_case("ogg")) {
        return;
    }
    let Ok(Some(target_frames)) = vorbis_output_frames(input) else {
        return;
    };
    let target_samples = target_frames.saturating_mul(usize::from(channels));
    if samples.len() > target_samples {
        samples.truncate(target_samples);
    }
}

fn vorbis_output_frames(input: &[u8]) -> Result<Option<usize>> {
    let mut packet = Vec::new();
    let mut initial_discard = None;
    let mut last_granule = None;
    let mut pos = 0_usize;
    while pos < input.len() {
        if input.get(pos..pos + 4) != Some(b"OggS") {
            return Err(RmpegError::InvalidData(
                "missing Ogg page capture pattern".to_string(),
            ));
        }
        if pos + 27 > input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos + 27,
                remaining: input.len(),
            });
        }
        let granule = u64::from_le_bytes([
            input[pos + 6],
            input[pos + 7],
            input[pos + 8],
            input[pos + 9],
            input[pos + 10],
            input[pos + 11],
            input[pos + 12],
            input[pos + 13],
        ]);
        if granule != u64::MAX {
            last_granule = Some(granule);
        }

        let segment_count = usize::from(input[pos + 26]);
        let segment_table = pos + 27;
        let data_start = segment_table + segment_count;
        if data_start > input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_start,
                remaining: input.len(),
            });
        }
        let data_len = input[segment_table..data_start]
            .iter()
            .map(|value| usize::from(*value))
            .sum::<usize>();
        let data_end = data_start
            .checked_add(data_len)
            .ok_or_else(|| RmpegError::InvalidData("Ogg page segment data overflow".to_string()))?;
        if data_end > input.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: data_end,
                remaining: input.len(),
            });
        }

        let mut data_pos = data_start;
        for segment_len in &input[segment_table..data_start] {
            let segment_len = usize::from(*segment_len);
            packet.extend_from_slice(&input[data_pos..data_pos + segment_len]);
            data_pos += segment_len;
            if segment_len < 255 {
                if packet.starts_with(b"\x01vorbis") && packet.len() >= 30 {
                    let blocksize0_bits = usize::from(packet[28] & 0x0f);
                    let blocksize0 =
                        1_usize.checked_shl(blocksize0_bits as u32).ok_or_else(|| {
                            RmpegError::InvalidData("Vorbis block size is too large".to_string())
                        })?;
                    initial_discard.get_or_insert(blocksize0 / 2);
                }
                packet.clear();
            }
        }
        pos = data_end;
    }

    let Some(last_granule) = last_granule else {
        return Ok(None);
    };
    let Some(initial_discard) = initial_discard else {
        return Ok(None);
    };
    let target = last_granule.saturating_sub(initial_discard as u64);
    let target = usize::try_from(target)
        .map_err(|_| RmpegError::Unsupported("Vorbis output is too large".to_string()))?;
    Ok(Some(target))
}

fn ceil_to_frame_boundary(samples: usize, frame_size: usize) -> Option<usize> {
    if frame_size == 0 {
        return None;
    }
    Some(samples.div_ceil(frame_size).saturating_mul(frame_size))
}

pub fn samples_to_s16le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn codec_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    registry
}

fn channel_count(channels: usize) -> Result<u16> {
    u16::try_from(channels).map_err(|_| {
        RmpegError::Unsupported(format!("audio channel count {channels} is too large"))
    })
}

fn map_symphonia_error(error: SymphoniaError) -> RmpegError {
    match error {
        SymphoniaError::IoError(error) => RmpegError::Io(error.to_string()),
        SymphoniaError::Unsupported(message) => RmpegError::Unsupported(message.to_string()),
        other => RmpegError::InvalidData(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        audio_frame_hashes_from_samples, ceil_to_frame_boundary, ffmpeg_s16_from_f32,
        ffmpeg_s16_from_f64, ogg_packets, samples_to_s16le_bytes, trim_aac_priming_and_padding,
        vorbis_output_frames,
    };

    #[test]
    fn hashes_interleaved_s16_samples() {
        let frames = audio_frame_hashes_from_samples(&[1, -1, 2, -2], 8_000, 2).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].duration, 2);
        assert_eq!(frames[0].size, 8);
        assert_eq!(frames[0].hash, "04ba39ea65399c4d0a2916799d1a9475");
    }

    #[test]
    fn serializes_s16le_samples() {
        assert_eq!(
            samples_to_s16le_bytes(&[0, 0x1234, -2]),
            vec![0, 0, 0x34, 0x12, 0xfe, 0xff]
        );
    }

    #[test]
    fn rounds_float_pcm_like_ffmpeg_s16() {
        assert_eq!(ffmpeg_s16_from_f32(0.0), 0);
        assert_eq!(ffmpeg_s16_from_f32(0.5 / 32_768.0), 0);
        assert_eq!(ffmpeg_s16_from_f32(1.5 / 32_768.0), 2);
        assert_eq!(ffmpeg_s16_from_f32(-0.5 / 32_768.0), 0);
        assert_eq!(ffmpeg_s16_from_f32(-1.5 / 32_768.0), -2);
        assert_eq!(ffmpeg_s16_from_f32(1.0), i16::MAX);
        assert_eq!(ffmpeg_s16_from_f32(-1.0), i16::MIN);

        assert_eq!(ffmpeg_s16_from_f64(0.5 / 32_768.0), 0);
        assert_eq!(ffmpeg_s16_from_f64(-0.5 / 32_768.0), 0);
    }

    #[test]
    fn rounds_aac_duration_to_frame_boundary() {
        assert_eq!(ceil_to_frame_boundary(44_100, 1_024), Some(45_056));
    }

    #[test]
    fn trims_mp4_aac_priming_before_padding() {
        let mut samples = (0..6).collect::<Vec<_>>();
        trim_aac_priming_and_padding(&mut samples, 1, 4);
        assert_eq!(samples, vec![0, 1, 2, 3]);

        let mut samples = (0..5).collect::<Vec<_>>();
        trim_aac_priming_and_padding(&mut samples, 1, 3);
        assert_eq!(samples, vec![0, 1, 2]);

        let mut samples = (0..2048).collect::<Vec<_>>();
        trim_aac_priming_and_padding(&mut samples, 1, 1024);
        assert_eq!(samples[0], 1024);
        assert_eq!(samples.len(), 1024);
    }

    #[test]
    fn extracts_ogg_packets_from_lacing_segments() {
        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.resize(26, 0);
        page.push(2);
        page.push(3);
        page.push(2);
        page.extend_from_slice(b"abcde");
        assert_eq!(
            ogg_packets(&page).unwrap(),
            vec![b"abc".to_vec(), b"de".to_vec()]
        );
    }

    #[test]
    fn trims_vorbis_output_by_initial_discard_and_final_granule() {
        let mut ident = vec![0; 30];
        ident[..7].copy_from_slice(b"\x01vorbis");
        ident[28] = 8;

        let mut ogg = Vec::new();
        ogg.extend_from_slice(&ogg_page(0, &ident));
        ogg.extend_from_slice(&ogg_page(44_100, b"audio"));

        assert_eq!(vorbis_output_frames(&ogg).unwrap(), Some(43_972));
    }

    fn ogg_page(granule: u64, packet: &[u8]) -> Vec<u8> {
        assert!(packet.len() < 255);
        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.push(0);
        page.push(0);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&1_u32.to_le_bytes());
        page.extend_from_slice(&0_u32.to_le_bytes());
        page.extend_from_slice(&0_u32.to_le_bytes());
        page.push(1);
        page.push(packet.len() as u8);
        page.extend_from_slice(packet);
        page
    }
}
