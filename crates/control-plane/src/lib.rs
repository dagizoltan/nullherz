pub use nullherz_traits::{
    Command, TimestampedCommand, TopologyCommand, ParameterMetadata, SidecarMetadata,
};

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
