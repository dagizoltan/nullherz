use serde::{Serialize, Deserialize};
use serde_with::serde_as;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct NodeState {
    pub id: u32,
    pub type_id: u32,
    pub params: Vec<(u32, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct EdgeState {
    pub node_idx: u32,
    pub input_idx: u32,
    pub buffer_idx: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct OutputEdgeState {
    pub node_idx: u32,
    pub output_idx: u32,
    pub buffer_idx: u32,
}

#[serde_as]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct SequencerPatternState {
    #[serde_as(as = "[[_; 64]; 16]")]
    pub grid: [[bool; 64]; 16],
    pub len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct SequencerNodeState {
    pub node_idx: u32,
    pub patterns: Vec<SequencerPatternState>,
    pub active_pattern: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct ProcessorState {
    pub node_idx: u32,
    pub state_data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct ProjectState {
    pub nodes: Vec<NodeState>,
    pub edges: Vec<EdgeState>,
    pub output_edges: Vec<OutputEdgeState>,
    pub sequencers: Vec<SequencerNodeState>,
    pub processor_states: Vec<ProcessorState>,
    pub modulation_matrix: crate::modulation_matrix::ModulationMatrix,
    pub arrangement: crate::pattern_manager::SongArrangement,
    pub clip_grid: crate::clip_orchestrator::ClipGrid,
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
            processor_states: Vec::new(),
            modulation_matrix: crate::modulation_matrix::ModulationMatrix::default(),
            arrangement: crate::pattern_manager::SongArrangement::default(),
            clip_grid: crate::clip_orchestrator::ClipGrid::default(),
            bpm: 120.0,
            transport_playing: false,
        }
    }

    pub fn save_to_file(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| std::io::Error::other(e))?;
        std::fs::write(path, json)
    }

    pub fn load_from_file(path: &str) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| std::io::Error::other(e))
    }

    pub fn save_binary(&self, path: &str) -> std::io::Result<()> {
        use rkyv::ser::Serializer;
        use rkyv::ser::serializers::AllocSerializer;

        let mut serializer = AllocSerializer::<4096>::default();
        serializer.serialize_value(self).map_err(|e| std::io::Error::other(e))?;
        let bytes = serializer.into_serializer().into_inner();
        std::fs::write(path, bytes)
    }
}
