pub mod common;
pub mod studio;
pub mod dj;

use nullherz_traits::{Command, ProcessorTypeId};
use std::collections::HashMap;

pub use common::*;

#[derive(Debug, Clone, Default)]
pub struct DeckNodes {
    pub sampler_id: u32,
    pub out_l: u32,
    pub out_r: u32,
    pub isolator_id: u32,
    pub gain_id: u32,
    pub filter_id: u32,
    pub keysync_id: u32,
    pub stereo_util_id: u32,
    pub dna_morph_id: Option<u32>,
    pub sequencer_id: u32,
    /// Private per-deck cue-send buffers (summed onto the global cue bus).
    pub cue_out_l: u32,
    pub cue_out_r: u32,
}

#[derive(Default)]
pub struct MixerManager {
    pub id_allocator: std::sync::Arc<nullherz_traits::IdAllocator>,
    pub config: MixerConfig,
    pub deck_mappings: HashMap<char, DeckNodes>,
    pub node_names: HashMap<String, u32>,
    /// Which sample id is loaded on each deck (recorded at LoadTrackToDeck).
    /// Needed to resolve a deck's sampler NODE back to its TRACK, e.g. when
    /// persisting hot cues.
    pub deck_samples: HashMap<char, u64>,
}

impl MixerManager {
    pub fn new() -> Self {
        Self {
            id_allocator: std::sync::Arc::new(nullherz_traits::IdAllocator::default()),
            config: MixerConfig::default(),
            deck_mappings: HashMap::new(),
            node_names: HashMap::new(),
            deck_samples: HashMap::new(),
        }
    }

    pub fn validate_topology(&self, commands: &[nullherz_traits::Command]) -> Result<(), String> {
        if self.id_allocator.current_node_id() >= nullherz_traits::MAX_NODES as u32 {
            return Err(format!("Mixer topology exceeds MAX_NODES ({})", nullherz_traits::MAX_NODES));
        }
        if self.id_allocator.current_buffer_id() > nullherz_traits::MAX_BUFFERS as u32 {
            return Err(format!("Mixer topology exceeds MAX_BUFFERS ({})", nullherz_traits::MAX_BUFFERS));
        }

        // Kahn's Algorithm for Cycle Detection
        let mut in_degree = std::collections::HashMap::new();
        let mut adj = std::collections::HashMap::new();
        let mut nodes = std::collections::HashSet::new();

        for cmd in commands {
            if let Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx, .. }) = cmd {
                nodes.insert(*node_idx);
                in_degree.entry(*node_idx).or_insert(0);
            }
        }

        // We need to track buffer producers to find edges between nodes
        let mut buffer_producers = std::collections::HashMap::new();
        for cmd in commands {
             if let Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx, new_buffer_idx, .. }) = cmd {
                 buffer_producers.insert(*new_buffer_idx, *node_idx);
             }
        }

        for cmd in commands {
            if let Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx, new_buffer_idx, .. }) = cmd
                && let Some(&src_node) = buffer_producers.get(new_buffer_idx)
                    && src_node != *node_idx {
                        adj.entry(src_node).or_insert_with(Vec::new).push(*node_idx);
                        *in_degree.entry(*node_idx).or_insert(0) += 1;
                    }
        }

        let mut queue = std::collections::VecDeque::new();
        for (&node, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(node);
            }
        }

        let mut count = 0;
        while let Some(u) = queue.pop_front() {
            count += 1;
            if let Some(neighbors) = adj.get(&u) {
                for &v in neighbors {
                    let degree = in_degree.get_mut(&v).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }

        if count < in_degree.len() {
            return Err("Cycle detected in mixer topology!".to_string());
        }

        Ok(())
    }

    pub fn create_studio_strip(&mut self, name: &str, fx_ids: &[u32]) -> Vec<Command> {
        studio::create_studio_strip(&self.id_allocator, name, fx_ids, &self.config)
    }

    pub fn create_aux_bus(&mut self, name: &str, fx_ids: &[u32]) -> Vec<Command> {
        let mut commands = Vec::new();
        let name_lower = name.to_lowercase();

        // 1. Summing Node (Stereo)
        let sum_l_id = self.id_allocator.allocate_node_id();
        let sum_r_id = self.id_allocator.allocate_node_id();
        let sum_out_l = self.id_allocator.allocate_buffer_id(1);
        let sum_out_r = self.id_allocator.allocate_buffer_id(1);

        self.node_names.insert(format!("aux_{}_sum_l", name_lower), sum_l_id);
        self.node_names.insert(format!("aux_{}_sum_r", name_lower), sum_r_id);

        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: sum_l_id, processor_type_id: ProcessorTypeId::SUMMING }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: sum_l_id, output_idx: 0, new_buffer_idx: sum_out_l }));

        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: sum_r_id, processor_type_id: ProcessorTypeId::SUMMING }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: sum_r_id, output_idx: 0, new_buffer_idx: sum_out_r }));

        // 2. FX Chain
        let mut prev_l = sum_l_id;
        let mut prev_r = sum_r_id;

        for (i, &fx_type) in fx_ids.iter().enumerate() {
            let fx_l_id = self.id_allocator.allocate_node_id();
            let fx_r_id = self.id_allocator.allocate_node_id();
            let fx_buf_l = self.id_allocator.allocate_buffer_id(1);
            let fx_buf_r = self.id_allocator.allocate_buffer_id(1);

            self.node_names.insert(format!("aux_{}_fx{}_l", name_lower, i), fx_l_id);
            self.node_names.insert(format!("aux_{}_fx{}_r", name_lower, i), fx_r_id);

            // Left
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: fx_l_id, processor_type_id: ProcessorTypeId(fx_type) }));
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: prev_l, output_idx: 0, new_buffer_idx: fx_buf_l }));
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: fx_l_id, input_idx: 0, new_buffer_idx: fx_buf_l }));

            // Right
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: fx_r_id, processor_type_id: ProcessorTypeId(fx_type) }));
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: prev_r, output_idx: 0, new_buffer_idx: fx_buf_r }));
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: fx_r_id, input_idx: 0, new_buffer_idx: fx_buf_r }));

            prev_l = fx_l_id;
            prev_r = fx_r_id;
        }

        // 3. Final Aux Output (Return to Master Sum by default)
        // Note: The actual return routing might be handled by the caller or another component.
        // For now we just ensure the chain is closed.

        commands
    }

    pub fn create_dj_deck(&mut self, deck_id: char, fx_ids: &[u32], bus_assignment: char) -> Vec<Command> {
        let (commands, nodes) = dj::create_dj_deck(&self.id_allocator, deck_id, fx_ids, bus_assignment, &self.config);
        self.deck_mappings.insert(deck_id, nodes.clone());
        let id_lower = deck_id.to_lowercase();
        self.node_names.insert(format!("deck_{}_sampler", id_lower), nodes.sampler_id);
        self.node_names.insert(format!("deck_{}_gain", id_lower), nodes.gain_id);
        self.node_names.insert(format!("deck_{}_filter", id_lower), nodes.filter_id);
        self.node_names.insert(format!("deck_{}_sequencer", id_lower), nodes.sequencer_id);
        // Strip end: the per-deck level the UI meters should watch.
        self.node_names.insert(format!("deck_{}_isolator", id_lower), nodes.isolator_id);
        commands
    }

    pub fn create_crossfader(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        let cf_id = self.id_allocator.allocate_node_id();
        self.node_names.insert("master_crossfader".to_string(), cf_id);
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

        // Bus summing: each deck renders to its own buffers; SUMMING nodes mix
        // them onto the shared bus. Two decks writing the same buffer would
        // OVERWRITE each other (executor gives exclusive write slices) — the
        // silent deck erases the playing one ("buses always zero" bug).
        {
            let deck_out = |mapping: &std::collections::HashMap<char, DeckNodes>, d: char| {
                let n = &mapping[&d];
                (n.out_l, n.out_r)
            };
            let (a_l, a_r) = deck_out(&self.deck_mappings, 'A');
            let (c_l, c_r) = deck_out(&self.deck_mappings, 'C');
            let (b_l, b_r) = deck_out(&self.deck_mappings, 'B');
            let (d_l, d_r) = deck_out(&self.deck_mappings, 'D');
            let bus_sum = |in_a: u32, in_b: u32, out: u32, commands: &mut Vec<Command>| {
                let id = self.id_allocator.allocate_node_id();
                commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: id, processor_type_id: ProcessorTypeId::SUMMING }));
                commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: id, input_idx: 0, new_buffer_idx: in_a }));
                commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: id, input_idx: 1, new_buffer_idx: in_b }));
                commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: id, output_idx: 0, new_buffer_idx: out }));
            };
            bus_sum(a_l, c_l, self.config.dj_a_l as u32, &mut commands);
            bus_sum(a_r, c_r, self.config.dj_a_r as u32, &mut commands);
            bus_sum(b_l, d_l, self.config.dj_b_l as u32, &mut commands);
            bus_sum(b_r, d_r, self.config.dj_b_r as u32, &mut commands);
        }

        // --- MASTER CROSSFADER (Stereo) ---
        let xf_l_id = self.id_allocator.allocate_node_id();
        let xf_r_id = self.id_allocator.allocate_node_id();
        let xf_out_l = self.id_allocator.allocate_buffer_id(1);
        let xf_out_r = self.id_allocator.allocate_buffer_id(1);

        // Named: the console crossfader control resolves these. (It used to
        // target "master_crossfader", a name only the STANDALONE crossfader
        // builder registers — the lookup fell back to node 0 and the UI
        // crossfader silently set a parameter on deck A's sampler.)
        self.node_names.insert("master_xf_l".to_string(), xf_l_id);
        self.node_names.insert("master_xf_r".to_string(), xf_r_id);

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

        // --- PREVIEW NODE ---
        // Renders planar stereo into its own buffer pair, mixed in by the
        // master sums below. It must NOT write the sum output buffers
        // directly: that makes it a second producer on the same physical
        // buffer, and whichever stage runs later erases the other (the old
        // wiring was only inaudible because Kahn happened to schedule the
        // preview first).
        // A real, legal graph index — NOT NodeConventions::PREVIEW, which is a
        // logical sentinel (111 >= MAX_NODES; as a graph index it was silently
        // dropped and preview never made a sound). The conductor translates
        // the sentinel to this node via node_names.
        let preview_id = self.id_allocator.allocate_node_id();
        self.node_names.insert("preview_node".to_string(), preview_id);
        let preview_l = self.id_allocator.allocate_buffer_id(2);
        let preview_r = preview_l + 1;
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: preview_id, processor_type_id: ProcessorTypeId::SAMPLER }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: preview_id, output_idx: 0, new_buffer_idx: preview_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: preview_id, output_idx: 1, new_buffer_idx: preview_r }));

        // Summing to FX Chain: one summing node PER SIDE. SummingProcessor
        // mixes all its inputs into outputs[0] only — a single node wired
        // L,R -> 0,1 mono-folds the crossfader mix onto the left buffer and
        // never writes the right one.
        let sum_l_id = self.id_allocator.allocate_node_id();
        let sum_r_id = self.id_allocator.allocate_node_id();
        let sum_out_l = self.id_allocator.allocate_buffer_id(2);
        let sum_out_r = sum_out_l + 1;
        // Named: these are the only per-side nodes at the master, so they are
        // the observation points for channel-identity tests and metering.
        self.node_names.insert("master_sum_l".to_string(), sum_l_id);
        self.node_names.insert("master_sum_r".to_string(), sum_r_id);

        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: sum_l_id, processor_type_id: ProcessorTypeId::SUMMING }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: sum_l_id, input_idx: 0, new_buffer_idx: xf_out_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: sum_l_id, input_idx: 1, new_buffer_idx: preview_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: sum_l_id, output_idx: 0, new_buffer_idx: sum_out_l }));

        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: sum_r_id, processor_type_id: ProcessorTypeId::SUMMING }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: sum_r_id, input_idx: 0, new_buffer_idx: xf_out_r }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: sum_r_id, input_idx: 1, new_buffer_idx: preview_r }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: sum_r_id, output_idx: 0, new_buffer_idx: sum_out_r }));

        // --- MASTER FX CHAIN ---

        // Master FX chain runs STEREO end-to-end: every node reads and writes
        // both channels (MultiChannelDspProcessor handles N channels). Mono
        // wiring here was the "right channel silent" bug — the limiter wrote
        // out ch1 from an input that never existed.

        // 1. Master EQ (Biquad)
        let eq_id = self.id_allocator.allocate_node_id();
        let eq_out_l = self.id_allocator.allocate_buffer_id(1);
        let eq_out_r = self.id_allocator.allocate_buffer_id(1);
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: eq_id, processor_type_id: ProcessorTypeId::BIQUAD }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: eq_id, input_idx: 0, new_buffer_idx: sum_out_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: eq_id, input_idx: 1, new_buffer_idx: sum_out_r }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: eq_id, output_idx: 0, new_buffer_idx: eq_out_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: eq_id, output_idx: 1, new_buffer_idx: eq_out_r }));

        // 2. Master Limiter/Gain — directly after the EQ.
        // NOTE: the former "Master Compressor" (ENVELOPE_FOLLOWER) is removed
        // from the audio path: an envelope follower OUTPUTS the envelope
        // (near-DC control signal), not audio — in-path it replaced the mix
        // with a ~constant level (the frozen peak_L=0.0069 bug). Real dynamics
        // control on the master is the limiter's job; the follower belongs in
        // sidechain/metering roles.
        let lim_id = self.id_allocator.allocate_node_id();
        self.node_names.insert("master_limiter".to_string(), lim_id);
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: lim_id, processor_type_id: ProcessorTypeId::LIMITER }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: lim_id, input_idx: 0, new_buffer_idx: eq_out_l }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: lim_id, input_idx: 1, new_buffer_idx: eq_out_r }));

        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: lim_id, output_idx: 0, new_buffer_idx: self.config.master_l as u32 }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: lim_id, output_idx: 1, new_buffer_idx: self.config.master_r as u32 }));

        // --- CUE BUS SUMMING ---
        // One summing node per side mixes the four private per-deck cue
        // sends onto the global cue buffers. Decks writing cue_l/r directly
        // was a four-producer overwrite (only deck D survived).
        {
            let outs: Vec<(u32, u32)> = ['A', 'B', 'C', 'D']
                .iter()
                .map(|d| {
                    let n = &self.deck_mappings[d];
                    (n.cue_out_l, n.cue_out_r)
                })
                .collect();
            let cue_sum_l = self.id_allocator.allocate_node_id();
            let cue_sum_r = self.id_allocator.allocate_node_id();
            self.node_names.insert("cue_sum_l".to_string(), cue_sum_l);
            self.node_names.insert("cue_sum_r".to_string(), cue_sum_r);
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: cue_sum_l, processor_type_id: ProcessorTypeId::SUMMING }));
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: cue_sum_r, processor_type_id: ProcessorTypeId::SUMMING }));
            for (i, (l, r)) in outs.iter().enumerate() {
                commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: cue_sum_l, input_idx: i as u32, new_buffer_idx: *l }));
                commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: cue_sum_r, input_idx: i as u32, new_buffer_idx: *r }));
            }
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: cue_sum_l, output_idx: 0, new_buffer_idx: self.config.cue_l as u32 }));
            commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: cue_sum_r, output_idx: 0, new_buffer_idx: self.config.cue_r as u32 }));
        }

        // --- CAPTURE NODE (master resample tap) ---
        // Reads the finished master (post-limiter) as a parallel consumer;
        // NO outputs, so it cannot alter the audio path (golden hashes
        // untouched). Recording is armed via param 3; snapshots register
        // into the SampleRegistry as playable planar-stereo sources. Named
        // so the sampler view's capture controls (which already resolve
        // "capture_node" and skip while it is missing) come alive.
        let cap_id = self.id_allocator.allocate_node_id();
        self.node_names.insert("capture_node".to_string(), cap_id);
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: cap_id, processor_type_id: ProcessorTypeId::CAPTURE }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: cap_id, input_idx: 0, new_buffer_idx: self.config.master_l as u32 }));
        commands.push(Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: cap_id, input_idx: 1, new_buffer_idx: self.config.master_r as u32 }));

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
        // 3. UpdateOutputEdge (Gain Out L)
        // 4. UpdateOutputEdge (Gain Out R)
        // 5. AddNode (Fader)
        // 6. UpdateEdge (Fader L)
        // 7. UpdateEdge (Fader R)
        // 8. UpdateOutputEdge (Master L)
        // 9. UpdateOutputEdge (Master R)
        assert_eq!(commands.len(), 9);


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

    /// CLASS KILLER: no buffer in the generated console may have two
    /// producers. The executor gives each stage exclusive write slices, so
    /// a second producer does not mix — it OVERWRITES, and whichever stage
    /// runs last wins silently. This shipped three times: the preview node
    /// erased the master sum, and both cue buffers only ever carried
    /// deck D. Mixing is what SUMMING nodes are for.
    #[test]
    fn test_bootstrap_has_single_producer_per_buffer() {
        let mut mixer = MixerManager::new();
        let commands = mixer.create_4channel_mixer();

        let mut producers: HashMap<u32, Vec<u32>> = HashMap::new();
        for cmd in &commands {
            if let Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx, new_buffer_idx, .. }) = cmd {
                producers.entry(*new_buffer_idx).or_default().push(*node_idx);
            }
        }
        for (buf, nodes) in &producers {
            assert!(
                nodes.len() == 1,
                "buffer {} has {} producers ({:?}) — a second producer silently overwrites the first; mix through a SUMMING node instead",
                buf,
                nodes.len(),
                nodes
            );
        }
    }

    #[test]
    fn test_topology_cycle_detection() {
        let mixer = MixerManager::new();
        let node_a = 0;
        let node_b = 1;
        let buf_a = 10;
        let buf_b = 11;

        let mut commands = vec![
            Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: node_a, processor_type_id: ProcessorTypeId::GAIN }),
            Command::Topology(nullherz_traits::TopologyCommand::AddNode { node_idx: node_b, processor_type_id: ProcessorTypeId::GAIN }),
            // A -> B
            Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: node_a, output_idx: 0, new_buffer_idx: buf_a }),
            Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: node_b, input_idx: 0, new_buffer_idx: buf_a }),
            // B -> A (Cycle!)
            Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge { node_idx: node_b, output_idx: 0, new_buffer_idx: buf_b }),
            Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge { node_idx: node_a, input_idx: 0, new_buffer_idx: buf_b }),
        ];

        let res = mixer.validate_topology(&commands);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Cycle detected in mixer topology!");

        // Remove the back edge to break the cycle
        commands.pop();
        assert!(mixer.validate_topology(&commands).is_ok());
    }

    #[test]
    fn test_mixer_manager_4channel_connectivity() {
        let mut mixer = MixerManager::new();
        let commands = mixer.create_4channel_mixer();
        assert!(mixer.validate_topology(&commands).is_ok());

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

        // The master limiter must have inputs and outputs.
        let lim_idx = *mixer.node_names.get("master_limiter").expect("limiter named");
        assert!(nodes_with_inputs.contains(&lim_idx));
        assert!(nodes_with_outputs.contains(&lim_idx));

        // The capture tap is BY DESIGN a pure consumer: inputs from the
        // master buffers, NO outputs (so it cannot alter the audio path).
        let cap_idx = *mixer.node_names.get("capture_node").expect("capture named");
        assert!(nodes_with_inputs.contains(&cap_idx));
        assert!(!nodes_with_outputs.contains(&cap_idx));
    }
}
