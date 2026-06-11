/// Represents an action to be performed by the audio engine.
/// Fixed-size strings are used to avoid heap allocations in the RT thread.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    SetParam {
        /// Target ID (e.g. hash of a name or a fixed-size buffer)
        target_id: u64,
        param_id: u32,
        value: f32,
        ramp_duration_samples: u32,
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
    SwapProcessor {
        node_idx: u32,
        // In a real system, we'd pass a factory or ID for the new processor.
        // For this prototype, we'll use a numeric ID.
        processor_type_id: u32,
    },
    Bundle {
        // Flat array of parameter updates: [node_id, param_id, value_bits, ...]
        count: u32,
        data: [u64; 12], // Supports up to 4 bundled SetParam commands
    },
    AddNode {
        processor_type_id: u32,
        node_idx: u32,
    },
    CommitTopology,
    SetSequencerStep {
        track: u32,
        step: u32,
        value: bool,
    },
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TopologyCommand {
    AddNode {
        processor_type_id: u32,
        node_idx: u32,
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
        processor_type_id: u32,
    },
}

/// A command with an associated timestamp for deterministic execution.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
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

pub struct History {
    pub snapshots: Vec<String>,
    pub current_idx: usize,
}

pub struct ModMatrix {
    pub connections: Vec<(u64, u32, u64)>, // source_node, param_id, dest_node
}

impl Default for ModMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl ModMatrix {
    pub fn new() -> Self { Self { connections: Vec::new() } }
    pub fn add_connection(&mut self, src: u64, param: u32, dest: u64) {
        self.connections.push((src, param, dest));
    }
}

impl History {
    pub fn new(initial_state: String) -> Self {
        Self { snapshots: vec![initial_state], current_idx: 0 }
    }

    pub fn push(&mut self, state: String) {
        self.snapshots.truncate(self.current_idx + 1);
        self.snapshots.push(state);
        self.current_idx += 1;
    }

    pub fn undo(&mut self) -> Option<&String> {
        if self.current_idx > 0 {
            self.current_idx -= 1;
            Some(&self.snapshots[self.current_idx])
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<&String> {
        if self.current_idx < self.snapshots.len() - 1 {
            self.current_idx += 1;
            Some(&self.snapshots[self.current_idx])
        } else {
            None
        }
    }
}
