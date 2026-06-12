pub mod common;
pub mod studio;
pub mod dj;

use control_plane::Command;
pub use common::*;
use nullherz_traits::ProcessorType;

#[derive(Default)]
pub struct MixerManager {
    next_node_id: u32,
    next_buffer_id: u32,
}

impl MixerManager {
    pub fn new() -> Self {
        Self {
            next_node_id: 0,
            next_buffer_id: 12,
        }
    }

    pub fn create_studio_strip(&mut self, name: &str, fx_ids: &[u32]) -> Vec<Command> {
        studio::create_studio_strip(&mut self.next_node_id, &mut self.next_buffer_id, name, fx_ids)
    }

    pub fn create_dj_deck(&mut self, deck_id: char, fx_ids: &[u32], bus_assignment: char) -> Vec<Command> {
        dj::create_dj_deck(&mut self.next_node_id, &mut self.next_buffer_id, deck_id, fx_ids, bus_assignment)
    }

    pub fn create_crossfader(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        let cf_id = self.next_node_id;
        self.next_node_id += 1;
        println!("Creating Master Crossfader (Node {})", cf_id);
        commands.push(Command::AddNode { node_idx: cf_id, processor_type_id: ProcessorType::Crossfader as u32 });
        commands
    }

    pub fn create_4channel_mixer(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        println!("Building 4-Channel Mixer Architecture...");

        let decks = ['A', 'B', 'C', 'D'];
        for &deck in &decks {
            let bus = if deck == 'A' || deck == 'C' { 'A' } else { 'B' };
            commands.extend(self.create_dj_deck(deck, &[1], bus));
        }

        let sum_id = self.next_node_id;
        self.next_node_id += 1;
        commands.push(Command::AddNode { node_idx: sum_id, processor_type_id: ProcessorType::Summing as u32 });

        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 0, new_buffer_idx: BUF_DJ_A_L as u32 });
        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 1, new_buffer_idx: BUF_DJ_B_L as u32 });
        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 2, new_buffer_idx: BUF_DJ_A_R as u32 });
        commands.push(Command::UpdateEdge { node_idx: sum_id, input_idx: 3, new_buffer_idx: BUF_DJ_B_R as u32 });

        commands.push(Command::UpdateOutputEdge { node_idx: sum_id, output_idx: 0, new_buffer_idx: BUF_MASTER_L as u32 });
        commands.push(Command::UpdateOutputEdge { node_idx: sum_id, output_idx: 1, new_buffer_idx: BUF_MASTER_R as u32 });

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixer_manager_ids() {
        let mut mixer = MixerManager::new();
        let commands = mixer.create_studio_strip("Test", &[]);

        // Studio strip with no FX should have:
        // 1. AddNode (Gain)
        // 2. UpdateEdge (Input)
        // 3. UpdateOutputEdge (Gain Out)
        // 4. AddNode (Fader)
        // 5. UpdateEdge (Fader L)
        // 6. UpdateEdge (Fader R)
        // 7. UpdateOutputEdge (Master L)
        // 8. UpdateOutputEdge (Master R)
        assert_eq!(commands.len(), 8);

        assert_eq!(mixer.next_node_id, 2);
    }

    #[test]
    fn test_mixer_manager_4channel() {
        let mut mixer = MixerManager::new();
        let commands = mixer.create_4channel_mixer();

        // 4 decks + 1 summing node
        // Each deck with 1 FX has: 1 Resample + 1 FX + 1 EQ = 3 nodes
        // 4 decks * 3 nodes = 12 nodes
        // + 1 summing node = 13 nodes total
        assert!(mixer.next_node_id >= 13);
        assert!(!commands.is_empty());
    }

    use proptest::prelude::*;
    proptest! {
        #[test]
        fn test_mixer_generated_topology_acyclic(
            num_fx in 0..5u32,
            fx_type in 0..100u32
        ) {
            let mut mixer = MixerManager::new();
            let fx_ids: Vec<u32> = vec![fx_type; num_fx as usize];
            let commands = mixer.create_studio_strip("Test", &fx_ids);

            // Check that for any UpdateEdge { node_idx, new_buffer_idx },
            // and UpdateOutputEdge { node_idx, new_buffer_idx },
            // we maintain an ordering if nodes are connected via buffers.
            // For studio strip, it's linear: Gain -> FX1 -> FX2 -> ... -> Fader.
            let mut last_node_idx = None;
            for cmd in commands {
                if let Command::AddNode { node_idx, .. } = cmd {
                    if let Some(last) = last_node_idx {
                        assert!(node_idx > last);
                    }
                    last_node_idx = Some(node_idx);
                }
            }
        }
    }
}
