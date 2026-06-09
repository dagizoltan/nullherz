#[derive(Debug)]
pub enum AudioError {
    BackendInitFailed(String),
    GraphFull,
    InvalidNodeId(u64),
    IpcError(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::BackendInitFailed(s) => write!(f, "Backend initialization failed: {}", s),
            AudioError::GraphFull => write!(f, "Audio graph is full (max {} nodes)", crate::MAX_NODES),
            AudioError::InvalidNodeId(id) => write!(f, "Invalid node ID: {}", id),
            AudioError::IpcError(s) => write!(f, "IPC error: {}", s),
        }
    }
}

impl std::error::Error for AudioError {}
