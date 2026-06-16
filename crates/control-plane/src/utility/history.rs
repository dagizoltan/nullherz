pub struct History {
    pub snapshots: Vec<String>,
    pub current_idx: usize,
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
