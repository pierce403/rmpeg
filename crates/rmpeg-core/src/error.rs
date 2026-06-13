use std::fmt;

pub type Result<T> = std::result::Result<T, RmpegError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RmpegError {
    Io(String),
    InvalidData(String),
    Unsupported(String),
    UnexpectedEof { needed: usize, remaining: usize },
    Usage(String),
}

impl fmt::Display for RmpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "I/O error: {message}"),
            Self::InvalidData(message) => write!(f, "invalid data: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported: {message}"),
            Self::UnexpectedEof { needed, remaining } => {
                write!(
                    f,
                    "unexpected end of input: needed {needed} bytes, had {remaining}"
                )
            }
            Self::Usage(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for RmpegError {}

impl From<std::io::Error> for RmpegError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
