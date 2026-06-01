#[derive(Debug, Clone)]
pub enum AudioError {
    BackendInitFailed(String),
    StreamCreationFailed(String),
    DeviceNotFound(String),
    InternalError(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::BackendInitFailed(s) => write!(f, "Backend initialization failed: {}", s),
            AudioError::StreamCreationFailed(s) => write!(f, "Stream creation failed: {}", s),
            AudioError::DeviceNotFound(s) => write!(f, "Device not found: {}", s),
            AudioError::InternalError(s) => write!(f, "Internal error: {}", s),
        }
    }
}

impl std::error::Error for AudioError {}
