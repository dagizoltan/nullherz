use crate::*;

#[derive(Debug, Clone, Copy)]
pub struct ParameterMetadata {
    pub id: u32,
    pub name: [u8; 32],
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessorMetadata {
    pub processor_id: u64,
    pub num_parameters: u32,
    pub parameters: [ParameterMetadata; 16],
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct StageNodes(#[serde(with = "BigArray")] pub [u32; MAX_NODES]);

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CompiledGraphPlan {
    #[serde(with = "BigArray")]
    pub stages: [StageNodes; MAX_NODES],
    #[serde(with = "BigArray")]
    pub stage_counts: [u32; MAX_NODES],
    pub num_stages: usize,
    /// Disjoint sub-graph identification for partial re-compilation and optimized O(1) swaps.
    #[serde(with = "BigArray")]
    pub node_islands: [u8; MAX_NODES],
    /// Per-node compensation delay in samples.
    #[serde(with = "BigArray")]
    pub node_latencies: [u32; MAX_NODES],
    /// Per-node, per-input compensation delay.
    #[serde(with = "BigArray")]
    pub input_delays: [InputDelays; MAX_NODES],
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InputDelays(#[serde(with = "BigArray")] pub [f32; MAX_CHANNELS]);

impl Default for CompiledGraphPlan {
    fn default() -> Self {
        Self {
            stages: [StageNodes([0; MAX_NODES]); MAX_NODES],
            stage_counts: [0; MAX_NODES],
            num_stages: 0,
            node_islands: [0; MAX_NODES],
            node_latencies: [0; MAX_NODES],
            input_delays: [InputDelays([0.0; MAX_CHANNELS]); MAX_NODES],
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NodeRouting {
    pub input_indices: [BufferId; MAX_CHANNELS],
    pub output_indices: [BufferId; MAX_CHANNELS],
    pub sidechain_indices: [BufferId; MAX_CHANNELS],
    pub input_count: usize,
    pub output_count: usize,
    pub sidechain_count: usize,
    /// Delay compensation required for this node's inputs in samples.
    pub input_delays: [f32; MAX_CHANNELS],
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CrossfadeState {
    pub node_idx: usize,
    pub input_idx: usize,
    pub old_buffer_idx: usize,
    pub new_buffer_idx: usize,
    pub remaining_samples: u32,
    pub total_samples: u32,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub struct NodeAssignment(#[serde(with = "BigArray")] pub [u8; 32]);

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NodeAssignmentArray(#[serde(with = "BigArray")] pub [NodeAssignment; MAX_NODES]);

impl Default for NodeAssignmentArray {
    fn default() -> Self {
        Self([NodeAssignment::default(); MAX_NODES])
    }
}
