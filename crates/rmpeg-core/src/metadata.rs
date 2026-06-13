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
