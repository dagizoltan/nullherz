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
