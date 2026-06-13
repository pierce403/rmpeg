pub mod error;
pub mod io;
pub mod metadata;

pub use error::{Result, RmpegError};
pub use metadata::{AudioFrameHash, AudioStreamMetadata, Packet, Rational};
