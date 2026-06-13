pub mod mp3;
pub mod mp4;
pub mod probe;
pub mod wav;

pub use mp3::parse_mp3;
pub use mp4::parse_mp4;
pub use probe::probe;
pub use wav::{parse_wav, WavFile};
