#[derive(Clone, Copy)]
pub struct CrossfadeState {
    pub node_idx: usize,
    pub input_idx: usize,
    pub old_buffer_idx: usize,
    pub new_buffer_idx: usize,
    pub remaining_samples: u32,
    pub total_samples: u32,
}

#[derive(Clone, Copy)]
pub struct NodeRouting {
    pub input_indices: [usize; crate::MAX_CHANNELS],
    pub output_indices: [usize; crate::MAX_CHANNELS],
    pub input_count: usize,
    pub output_count: usize,
}

#[derive(Clone, Copy)]
pub struct GraphTopology {
    pub routing: [NodeRouting; crate::MAX_NODES],
    pub virtual_to_physical: [usize; crate::MAX_NODES],
    pub stages: [[usize; crate::MAX_NODES]; crate::MAX_NODES],
    pub stage_counts: [usize; crate::MAX_NODES],
    pub num_stages: usize,
    pub crossfades: [Option<CrossfadeState>; 8],
    pub node_count: usize,
}
