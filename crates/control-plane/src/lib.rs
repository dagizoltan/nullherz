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
}

/// A command with an associated timestamp for deterministic execution.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimestampedCommand {
    pub timestamp_samples: u64,
    pub command: Command,
}
