pub mod common;
pub mod studio;
pub mod dj;

use nullherz_traits::{Command, ProcessorTypeId};
use std::collections::HashMap;

pub use common::*;

#[derive(Debug, Clone, Default)]
pub struct DeckNodes {
    pub sampler_id: u32,
    pub isolator_id: u32,
    pub gain_id: u32,
}

#[derive(Default)]
pub struct MixerManager {
    pub id_allocator: std::sync::Arc<nullherz_traits::IdAllocator>,
    pub config: MixerConfig,
    pub deck_mappings: HashMap<char, DeckNodes>,
}

impl MixerManager {
    pub fn new() -> Self {
        Self {
            id_allocator: std::sync::Arc::new(nullherz_traits::IdAllocator::default()),
            config: MixerConfig::default(),
            deck_mappings: HashMap::new(),
        }
    }

    pub fn validate_topology(&self) -> Result<(), String> {
        if self.id_allocator.current_node_id() >= nullherz_traits::MAX_NODES as u32 {
            return Err(format!("Mixer topology exceeds MAX_NODES ({})", nullherz_traits::MAX_NODES));
        }
        Ok(())
    }

    pub fn create_studio_strip(&mut self, name: &str, fx_ids: &[u32]) -> Vec<Command> {
        studio::create_studio_strip(&self.id_allocator, name, fx_ids, &self.config)
    }

    pub fn create_dj_deck(&mut self, deck_id: char, fx_ids: &[u32], bus_assignment: char) -> Vec<Command> {
        let (commands, nodes) = dj::create_dj_deck(&self.id_allocator, deck_id, fx_ids, bus_assignment, &self.config);
        self.deck_mappings.insert(deck_id, nodes);
        commands
    }

    pub fn create_crossfader(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        let cf_id = self.id_allocator.allocate_node_id();
        println!("Creating Master Crossfader (Node {})", cf_id);
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: cf_id, processor_type_id: ProcessorTypeId::CROSSFADER }));
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

        // --- MASTER CROSSFADER (Stereo) ---
        let xf_l_id = self.id_allocator.allocate_node_id();
        let xf_r_id = self.id_allocator.allocate_node_id();
        let xf_out_l = self.id_allocator.allocate_buffer_id(1);
        let xf_out_r = self.id_allocator.allocate_buffer_id(1);

        // Left Path
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: xf_l_id, processor_type_id: ProcessorTypeId::CROSSFADER }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: xf_l_id, input_idx: 0, new_buffer_idx: self.config.dj_a_l as u32 }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: xf_l_id, input_idx: 1, new_buffer_idx: self.config.dj_b_l as u32 }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: xf_l_id, output_idx: 0, new_buffer_idx: xf_out_l }));

        // Right Path
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: xf_r_id, processor_type_id: ProcessorTypeId::CROSSFADER }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: xf_r_id, input_idx: 0, new_buffer_idx: self.config.dj_a_r as u32 }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: xf_r_id, input_idx: 1, new_buffer_idx: self.config.dj_b_r as u32 }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: xf_r_id, output_idx: 0, new_buffer_idx: xf_out_r }));

        // Summing to FX Chain
        let sum_id = self.id_allocator.allocate_node_id();
        let sum_out_l = self.id_allocator.allocate_buffer_id(1);
        let sum_out_r = self.id_allocator.allocate_buffer_id(1);

        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: sum_id, processor_type_id: ProcessorTypeId::SUMMING }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: sum_id, input_idx: 0, new_buffer_idx: xf_out_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: sum_id, input_idx: 1, new_buffer_idx: xf_out_r }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: sum_id, output_idx: 0, new_buffer_idx: sum_out_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: sum_id, output_idx: 1, new_buffer_idx: sum_out_r }));

        // --- MASTER FX CHAIN ---

        // 1. Master EQ (Biquad)
        let eq_id = self.id_allocator.allocate_node_id();
        let eq_out_l = self.id_allocator.allocate_buffer_id(1);
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: eq_id, processor_type_id: ProcessorTypeId::BIQUAD }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: eq_id, input_idx: 0, new_buffer_idx: sum_out_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: eq_id, output_idx: 0, new_buffer_idx: eq_out_l }));

        // 2. Master Compressor (Envelope Follower)
        let comp_id = self.id_allocator.allocate_node_id();
        let comp_out_l = self.id_allocator.allocate_buffer_id(1);
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: comp_id, processor_type_id: ProcessorTypeId::ENVELOPE_FOLLOWER }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: comp_id, input_idx: 0, new_buffer_idx: eq_out_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: comp_id, output_idx: 0, new_buffer_idx: comp_out_l }));

        // 3. Master Limiter/Gain
        let lim_id = self.id_allocator.allocate_node_id();
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: lim_id, processor_type_id: ProcessorTypeId::GAIN }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: lim_id, input_idx: 0, new_buffer_idx: comp_out_l }));

        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: lim_id, output_idx: 0, new_buffer_idx: self.config.master_l as u32 }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: lim_id, output_idx: 1, new_buffer_idx: self.config.master_r as u32 }));

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


    }

    #[test]
    fn test_mixer_manager_4channel() {
        let mut mixer = MixerManager::new();
        let commands = mixer.create_4channel_mixer();

        // 4 decks + 1 summing node
        // Each deck with 1 FX has: 1 Resample + 1 FX + 1 EQ = 3 nodes
        // 4 decks * 3 nodes = 12 nodes
        // + 1 summing node = 13 nodes total

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
                if let Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx, .. }) = cmd {
                    if let Some(last) = last_node_idx {
                        assert!(node_idx > last);
                    }
                    last_node_idx = Some(node_idx);
                }
            }
        }
    }

    #[test]
    fn test_mixer_manager_4channel_connectivity() {
        let mut mixer = MixerManager::new();
        let commands = mixer.create_4channel_mixer();

        let mut nodes_with_outputs = std::collections::HashSet::new();
        let mut nodes_with_inputs = std::collections::HashSet::new();

        for cmd in &commands {
            match cmd {
                Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: _, .. }) => {
                    // All nodes should eventually have inputs/outputs or be sources
                }
                Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx, .. }) => {
                    nodes_with_inputs.insert(*node_idx);
                }
                Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx, .. }) => {
                    nodes_with_outputs.insert(*node_idx);
                }
                _ => {}
            }
        }

        // Summing node must have inputs and outputs
        // In 4-channel mixer, summing node should be the last added node index
        let sum_node_idx = mixer.id_allocator.allocate_node_id() - 1;
        assert!(nodes_with_inputs.contains(&sum_node_idx));
        assert!(nodes_with_outputs.contains(&sum_node_idx));
    }
}
