use std::sync::Arc;
use crate::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct GraphTopology {
    #[serde(with = "BigArray")]
    pub routing: [NodeRouting; MAX_NODES],
    #[serde(with = "BigArray")]
    pub virtual_to_physical: [u32; MAX_NODES],
    pub plan: CompiledGraphPlan,
    pub crossfades: [Option<CrossfadeState>; 8],
    pub node_count: usize,
    /// Mapping of node_idx to sidecar address or "local" (empty string/zeros).
    #[serde(with = "BigArray")]
    pub node_assignments: [NodeAssignment; MAX_NODES],
    #[serde(with = "BigArray")]
    pub node_positions: [Option<(f32, f32)>; MAX_NODES],
    #[serde(with = "BigArray")]
    pub bypass_states: [bool; MAX_NODES],
}

pub enum TopologyMutation {
    RemoveNode {
        node_idx: u32,
    },
    SetNodePosition {
        node_idx: u32,
        x: f32,
        y: f32,
    },
    UpdateEdge {
        node_idx: u32,
        input_idx: u32,
        new_buffer_idx: u32,
    },
    UpdateOutputEdge {
        node_idx: u32,
        output_idx: u32,
        new_buffer_idx: u32,
    },
    SwapProcessor {
        node_idx: u32,
        processor: Box<dyn AudioProcessor>,
    },
    AddNode {
        node_idx: u32,
        processor: Box<dyn AudioProcessor>,
    },
    AddSource {
        node_idx: u32,
        buffer: Arc<Vec<f32>>,
        sample_id: u64,
        metadata: Option<Arc<SampleMetadata>>,
    },
    UpdateMetadata {
        node_idx: u32,
        metadata: Arc<SampleMetadata>,
    },
    LoadProcessorState {
        node_idx: u32,
        state_data: Arc<Vec<u8>>,
    },
    SetBypass {
        node_idx: u32,
        enabled: bool,
    },
    SetTopology(Arc<GraphTopology>),
}
