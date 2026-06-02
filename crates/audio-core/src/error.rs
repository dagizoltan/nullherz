use std::fmt;

#[derive(Debug, Clone)]
pub enum AudioError {
    LibLoadFailed(String),
    BackendInitFailed(String),
    InvalidNodeIndex(usize),
    InvalidChannelIndex(usize),
    InvalidParameterIndex(u32),
    BufferCountMismatch { expected: usize, actual: usize },
    Internal(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::LibLoadFailed(msg) => write!(f, "Library load failed: {}", msg),
            AudioError::BackendInitFailed(msg) => write!(f, "Backend initialization failed: {}", msg),
            AudioError::InvalidNodeIndex(idx) => write!(f, "Invalid node index: {}", idx),
            AudioError::InvalidChannelIndex(idx) => write!(f, "Invalid channel index: {}", idx),
            AudioError::InvalidParameterIndex(idx) => write!(f, "Invalid parameter index: {}", idx),
            AudioError::BufferCountMismatch { expected, actual } => write!(f, "Buffer count mismatch: expected {}, actual {}", expected, actual),
            AudioError::Internal(msg) => write!(f, "Internal audio error: {}", msg),
        }
    }
}

impl std::error::Error for AudioError {}

pub type AudioResult<T> = Result<T, AudioError>;
