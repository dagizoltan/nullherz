use crate::MAX_NODES;

#[derive(Debug)]
pub enum AudioError {
    BackendInitFailed(String),
    GraphFull,
    InvalidNodeId(u64),
    IpcError(String),
    ProcessorError(String),
    ConfigurationError(String),
    Generic(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::BackendInitFailed(s) => write!(f, "Backend initialization failed: {}", s),
            AudioError::GraphFull => write!(f, "Audio graph is full (max {} nodes)", MAX_NODES),
            AudioError::InvalidNodeId(id) => write!(f, "Invalid node ID: {}", id),
            AudioError::IpcError(s) => write!(f, "IPC error: {}", s),
            AudioError::ProcessorError(s) => write!(f, "Processor error: {}", s),
            AudioError::ConfigurationError(s) => write!(f, "Configuration error: {}", s),
            AudioError::Generic(s) => write!(f, "Generic error: {}", s),
        }
    }
}

impl std::error::Error for AudioError {}
