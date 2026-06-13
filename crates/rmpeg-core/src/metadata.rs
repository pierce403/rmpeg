#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rational {
    pub num: i64,
    pub den: i64,
}

impl Rational {
    pub fn new(num: i64, den: i64) -> Self {
        Self { num, den }
    }

    pub fn as_f64(self) -> Option<f64> {
        if self.den == 0 {
            None
        } else {
            Some(self.num as f64 / self.den as f64)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeDocument {
    pub format: String,
    pub streams: Vec<StreamMetadata>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamMetadata {
    pub index: usize,
    pub codec_type: String,
    pub codec_name: String,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub bits_per_sample: Option<u16>,
    pub duration_seconds: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<String>,
}

impl StreamMetadata {
    pub fn audio(
        index: usize,
        codec_name: impl Into<String>,
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
        duration_seconds: f64,
    ) -> Self {
        Self {
            index,
            codec_type: "audio".to_string(),
            codec_name: codec_name.into(),
            sample_rate: Some(sample_rate),
            channels: Some(channels),
            bits_per_sample: Some(bits_per_sample),
            duration_seconds: Some(duration_seconds),
            width: None,
            height: None,
            frame_rate: None,
        }
    }

    pub fn video(
        index: usize,
        codec_name: impl Into<String>,
        width: u32,
        height: u32,
        duration_seconds: Option<f64>,
        frame_rate: Option<String>,
    ) -> Self {
        Self {
            index,
            codec_type: "video".to_string(),
            codec_name: codec_name.into(),
            sample_rate: None,
            channels: None,
            bits_per_sample: None,
            duration_seconds,
            width: Some(width),
            height: Some(height),
            frame_rate,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioStreamMetadata {
    pub index: usize,
    pub codec_type: String,
    pub codec_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub duration_seconds: f64,
    pub data_size: u32,
    pub block_align: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub stream_index: usize,
    pub pts: u64,
    pub duration: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrameHash {
    pub stream_index: usize,
    pub dts: u64,
    pub pts: u64,
    pub duration: u32,
    pub size: usize,
    pub hash: String,
}
