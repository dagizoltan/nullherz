pub mod common;
pub mod studio;
pub mod dj;

use nullherz_traits::{Command, ProcessorType};
pub use common::*;

pub struct MixerManager {
    next_node_id: u32,
    buffer_allocator: BufferAllocator,
    pub config: MixerConfig,
}

impl Default for MixerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MixerManager {
    pub fn new() -> Self {
        Self {
            next_node_id: 0,
            buffer_allocator: BufferAllocator::new(12, nullherz_traits::MAX_NODES as u32),
            config: MixerConfig::default(),
        }
    }

    pub fn validate_topology(commands: &[Command]) -> Result<(), String> {
        let mut writes_to: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
        for cmd in commands {
            if let Command::UpdateOutputEdge { node_idx, new_buffer_idx, .. } = cmd {
                writes_to.entry(*new_buffer_idx).or_insert_with(Vec::new).push(*node_idx);
            }
        }

        let mut node_adj: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
        for cmd in commands {
            if let Command::UpdateEdge { node_idx, new_buffer_idx, .. } = cmd {
                if let Some(producers) = writes_to.get(new_buffer_idx) {
                    for &producer in producers {
                        node_adj.entry(producer).or_insert_with(Vec::new).push(*node_idx);
                    }
                }
            }
        }

        let mut visited = std::collections::HashSet::new();
        let mut stack = std::collections::HashSet::new();

        fn has_cycle(
            u: u32,
            adj: &std::collections::HashMap<u32, Vec<u32>>,
            visited: &mut std::collections::HashSet<u32>,
            stack: &mut std::collections::HashSet<u32>,
        ) -> bool {
            visited.insert(u);
            stack.insert(u);

            if let Some(neighbors) = adj.get(&u) {
                for &v in neighbors {
                    if !visited.contains(&v) {
                        if has_cycle(v, adj, visited, stack) { return true; }
                    } else if stack.contains(&v) {
                        return true;
                    }
                }
            }

            stack.remove(&u);
            false
        }

        for &u in node_adj.keys() {
            if !visited.contains(&u) {
                if has_cycle(u, &node_adj, &mut visited, &mut stack) {
                    return Err("Cycle detected in mixer topology".into());
                }
            }
        }

        Ok(())
    }

    pub fn create_studio_strip(&mut self, name: &str, fx_ids: &[u32]) -> Result<Vec<Command>, String> {
        studio::create_studio_strip(&mut self.next_node_id, &mut self.buffer_allocator, name, fx_ids, &self.config)
    }

    pub fn create_dj_deck(&mut self, deck_id: char, fx_ids: &[u32], bus_assignment: char) -> Result<Vec<Command>, String> {
        dj::create_dj_deck(&mut self.next_node_id, &mut self.buffer_allocator, deck_id, fx_ids, bus_assignment, &self.config)
    }

    pub fn create_crossfader(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        let cf_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::AddNode { node_idx: cf_id, processor_type_id: ProcessorType::Crossfader as u32 });
        commands
    }

    pub fn create_4channel_mixer(&mut self) -> Result<Vec<Command>, String> {
        let mut commands = Vec::new();

        let decks = ['A', 'B', 'C', 'D'];
        for &deck in &decks {
            let bus = if deck == 'A' || deck == 'C' { 'A' } else { 'B' };
            commands.extend(self.create_dj_deck(deck, &[1], bus)?);
        }

        let sum_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::AddNode { node_idx: sum_id, processor_type_id: ProcessorType::Summing as u32 });

        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 0, new_buffer_idx: self.config.dj_a_l as u32 });
        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 1, new_buffer_idx: self.config.dj_b_l as u32 });
        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 2, new_buffer_idx: self.config.dj_a_r as u32 });
        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 3, new_buffer_idx: self.config.dj_b_r as u32 });

        commands.push(Command::UpdateOutputEdge { node_idx: sum_id, output_idx: 0, new_buffer_idx: self.config.master_l as u32 });
        commands.push(Command::UpdateOutputEdge { node_idx: sum_id, output_idx: 1, new_buffer_idx: self.config.master_r as u32 });

        Ok(commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixer_manager_ids() {
        let mut mixer = MixerManager::new();
        let commands = mixer.create_studio_strip("Test", &[]).unwrap();
        assert_eq!(commands.len(), 8);
        assert_eq!(mixer.next_node_id, 2);
    }

    #[test]
    fn test_mixer_topology_validation() {
        let mut mixer = MixerManager::new();
        let commands = mixer.create_4channel_mixer().unwrap();
        assert!(MixerManager::validate_topology(&commands).is_ok());

        // Create a cycle: Node 0 -> Buff 10 -> Node 1 -> Buff 11 -> Node 0
        let mut cyclic_cmds = Vec::new();
        cyclic_cmds.push(Command::UpdateOutputEdge { node_idx: 0, output_idx: 0, new_buffer_idx: 10 });
        cyclic_cmds.push(Command::UpdateEdge { node_idx: 1, input_idx: 0, new_buffer_idx: 10 });
        cyclic_cmds.push(Command::UpdateOutputEdge { node_idx: 1, output_idx: 0, new_buffer_idx: 11 });
        cyclic_cmds.push(Command::UpdateEdge { node_idx: 0, input_idx: 0, new_buffer_idx: 11 });

        assert!(MixerManager::validate_topology(&cyclic_cmds).is_err());
    }
}
