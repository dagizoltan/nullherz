use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    pub id: u32,
    pub type_id: u32,
    pub params: Vec<(u32, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeState {
    pub node_idx: u32,
    pub input_idx: u32,
    pub buffer_idx: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEdgeState {
    pub node_idx: u32,
    pub output_idx: u32,
    pub buffer_idx: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SequencerPatternState {
    pub grid: [[bool; 16]; 16],
    pub len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencerNodeState {
    pub node_idx: u32,
    pub patterns: Vec<SequencerPatternState>,
    pub active_pattern: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    pub nodes: Vec<NodeState>,
    pub edges: Vec<EdgeState>,
    pub output_edges: Vec<OutputEdgeState>,
    pub sequencers: Vec<SequencerNodeState>,
    pub bpm: f32,
    pub transport_playing: bool,
}

impl ProjectState {
    pub fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            output_edges: Vec::new(),
            sequencers: Vec::new(),
            bpm: 120.0,
            transport_playing: false,
        }
    }

    pub fn save_to_file(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    pub fn load_from_file(path: &str) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}
