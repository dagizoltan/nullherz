/// Represents an action to be performed by the audio engine.
/// Fixed-size strings are used to avoid heap allocations in the RT thread.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    SetParam {
        /// Target ID (e.g. hash of a name or a fixed-size buffer)
        target_id: u64,
        param_id: u32,
        value: f32,
    },
    Play,
    Stop,
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
    UpdateEdgeCrossfaded {
        node_idx: u32,
        input_idx: u32,
        new_buffer_idx: u32,
        duration_samples: u32,
    },
}

/// A command with an associated timestamp for deterministic execution.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimestampedCommand {
    pub timestamp_samples: u64,
    pub command: Command,
}

#[repr(C)]
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
pub struct SidecarMetadata {
    pub sidecar_id: u64,
    pub num_parameters: u32,
    pub parameters: [ParameterMetadata; 16],
}
