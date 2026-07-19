// Non-RT plane (topology backpressure pacing): thread sleep is sanctioned here.
#![allow(clippy::disallowed_methods)]
use nullherz_topology::GraphCompiler;
use std::sync::Arc;
use nullherz_processors::ProcessorRegistry;
use audio_core::processors::{TopologyMutation, GraphTopology, NodeRouting};
use nullherz_traits::Command;


/// Push a topology mutation with backpressure. The engine drains a bounded
/// number of mutations per audio block, so a large bootstrap (4 decks + buses
/// + master chain) can outrun the ring; dropping a structural mutation leaves
/// the graph permanently half-built (the "no master chain = eternal silence"
/// bug). This is the non-RT side, so briefly waiting is correct.
fn push_mutation(prod: &mut ipc_layer::NonRtProducer<TopologyMutation>, mut m: TopologyMutation) {
    for _ in 0..500 {
        match prod.push(m) {
            Ok(()) => return,
            Err(back) => {
                m = back;
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }
    eprintln!("TopologyManager: DROPPED mutation after 1s of backpressure — graph may be incomplete!");
}

pub struct TopologyManager {
    pub registry: ProcessorRegistry,
    pub topo_producer: Option<ipc_layer::NonRtProducer<TopologyMutation>>,
    pub current_sample_rate: f32,
    pub current_topology: GraphTopology,
    pub active_node_types: std::collections::HashMap<u32, u32>,
    pub id_allocator: nullherz_traits::IdAllocator,
}

impl Default for TopologyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TopologyManager {
    pub fn new() -> Self {
        let mut v2p = [0u32; nullherz_traits::MAX_BUFFERS];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i as u32; }

        Self {
            registry: ProcessorRegistry::new(),
            topo_producer: None,
            current_sample_rate: 44100.0,
            active_node_types: std::collections::HashMap::new(),
            id_allocator: nullherz_traits::IdAllocator::new(100, 100),
            current_topology: GraphTopology {
                routing: [NodeRouting {
                    input_indices: [0; nullherz_traits::MAX_CHANNELS],
                    output_indices: [0; nullherz_traits::MAX_CHANNELS],
                    sidechain_indices: [0; nullherz_traits::MAX_CHANNELS],
                    input_count: 0,
                    output_count: 0,
                    sidechain_count: 0,
                    input_delays: [0.0; nullherz_traits::MAX_CHANNELS],
                }; nullherz_traits::MAX_NODES],
                virtual_to_physical: v2p,
                plan: Default::default(),
                crossfades: [None; 8],
                node_count: 0,
                node_assignments: [nullherz_traits::NodeAssignment([0; 32]); nullherz_traits::MAX_NODES],
                node_positions: [None; nullherz_traits::MAX_NODES],
                bypass_states: [false; nullherz_traits::MAX_NODES],
            },
        }
    }

    pub fn handle_topology_command(&mut self, cmd: &Command) -> bool {
        let Some(ref mut prod) = self.topo_producer else { return false; };
        let sr = self.current_sample_rate;

        match *cmd {
            Command::Topology(nullherz_traits::TopologyCommand::RemoveNode { node_idx }) => {
                let idx = node_idx as usize;
                if idx < nullherz_traits::MAX_NODES {
                    self.active_node_types.remove(&node_idx);

                    // Note: Indices allocated by IdAllocator are monotonically increasing and are never reused
                    // for safety and simplicity, avoiding index collision issues.

                    let mut buffers_to_clear = std::collections::HashSet::new();
                    let r = &self.current_topology.routing[idx];
                    for &buf_idx in r.output_indices.iter().take(r.output_count) {
                        if buf_idx != 0 {
                            buffers_to_clear.insert(buf_idx);
                        }
                    }
                    for &buf_idx in r.input_indices.iter().take(r.input_count) {
                        if buf_idx != 0 {
                            buffers_to_clear.insert(buf_idx);
                        }
                    }

                    self.current_topology.routing[idx].input_indices.fill(0);
                    self.current_topology.routing[idx].output_indices.fill(0);
                    self.current_topology.routing[idx].sidechain_indices.fill(0);
                    self.current_topology.routing[idx].input_count = 0;
                    self.current_topology.routing[idx].output_count = 0;
                    self.current_topology.routing[idx].sidechain_count = 0;
                    self.current_topology.routing[idx].input_delays.fill(0.0);

                    for other_idx in 0..nullherz_traits::MAX_NODES {
                        if other_idx == idx { continue; }
                        let other_routing = &mut self.current_topology.routing[other_idx];
                        for i in 0..other_routing.input_count {
                            if buffers_to_clear.contains(&other_routing.input_indices[i]) {
                                other_routing.input_indices[i] = 0;
                            }
                        }
                        for i in 0..other_routing.output_count {
                            if buffers_to_clear.contains(&other_routing.output_indices[i]) {
                                other_routing.output_indices[i] = 0;
                            }
                        }
                    }

                    self.current_topology.node_positions[idx] = None;
                    self.current_topology.bypass_states[idx] = false;
                    self.current_topology.node_assignments[idx] = nullherz_traits::NodeAssignment([0; 32]);

                    let mut max_topo_idx = 0;
                    for i in (0..self.current_topology.node_count).rev() {
                        if self.active_node_types.contains_key(&(i as u32)) {
                            max_topo_idx = i + 1;
                            break;
                        }
                    }
                    self.current_topology.node_count = max_topo_idx;

                    push_mutation(prod, TopologyMutation::RemoveNode { node_idx });
                    return true;
                }
            }
            Command::Topology(nullherz_traits::TopologyCommand::AddNode {  processor_type_id, node_idx }) => {
                if let Some(processor) = self.registry.create_by_id(processor_type_id.0, node_idx, sr) {
                    self.active_node_types.insert(node_idx, processor_type_id.0);
                    let idx = node_idx as usize;
                    if idx < nullherz_traits::MAX_NODES {
                        self.current_topology.routing[idx].input_count = 0;
                        self.current_topology.routing[idx].output_count = 0;
                        if idx >= self.current_topology.node_count {
                            self.current_topology.node_count = idx + 1;
                        }
                    }
                    push_mutation(prod, TopologyMutation::AddNode { node_idx, processor });
                    return true;
                }
            }
            Command::Topology(nullherz_traits::TopologyCommand::SwapProcessor {  node_idx, processor_type_id }) => {
                if let Some(processor) = self.registry.create_by_id(processor_type_id.0, node_idx, sr) {
                    self.active_node_types.insert(node_idx, processor_type_id.0);
                    push_mutation(prod, TopologyMutation::SwapProcessor { node_idx, processor });
                    return true;
                }
            }
            Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge {   node_idx, input_idx, new_buffer_idx }) => {
                // Out-of-range buffer ids used to be clamped/wrapped downstream,
                // silently aliasing two edges onto one buffer. Reject them here,
                // loudly, where returning an error is free.
                if new_buffer_idx >= nullherz_traits::MAX_BUFFERS as u32 {
                    eprintln!("TopologyManager: REJECTED UpdateEdge node {} input {}: buffer {} out of range (MAX_BUFFERS = {})",
                        node_idx, input_idx, new_buffer_idx, nullherz_traits::MAX_BUFFERS);
                    return false;
                }
                let n_idx = node_idx as usize;
                let i_idx = input_idx as usize;
                if n_idx < nullherz_traits::MAX_NODES && i_idx < nullherz_traits::MAX_CHANNELS {
                    self.current_topology.routing[n_idx].input_indices[i_idx] = new_buffer_idx;
                    if i_idx >= self.current_topology.routing[n_idx].input_count {
                        self.current_topology.routing[n_idx].input_count = i_idx + 1;
                    }
                }
                push_mutation(prod, TopologyMutation::UpdateEdge { node_idx, input_idx, new_buffer_idx });
                return true;
            }
            Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge {   node_idx, output_idx, new_buffer_idx }) => {
                if new_buffer_idx >= nullherz_traits::MAX_BUFFERS as u32 {
                    eprintln!("TopologyManager: REJECTED UpdateOutputEdge node {} output {}: buffer {} out of range (MAX_BUFFERS = {})",
                        node_idx, output_idx, new_buffer_idx, nullherz_traits::MAX_BUFFERS);
                    return false;
                }
                let n_idx = node_idx as usize;
                let o_idx = output_idx as usize;
                if n_idx < nullherz_traits::MAX_NODES && o_idx < nullherz_traits::MAX_CHANNELS {
                    self.current_topology.routing[n_idx].output_indices[o_idx] = new_buffer_idx;
                    if o_idx >= self.current_topology.routing[n_idx].output_count {
                        self.current_topology.routing[n_idx].output_count = o_idx + 1;
                    }
                }
                push_mutation(prod, TopologyMutation::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx });
                return true;
            }
            Command::Topology(nullherz_traits::TopologyCommand::Connect { src_node_idx, src_output_idx, dst_node_idx, dst_input_idx }) => {
                // Find existing buffer if output already connected
                let mut buffer_idx = 0;
                let src_n = src_node_idx as usize;
                let src_o = src_output_idx as usize;
                if src_n < nullherz_traits::MAX_NODES && src_o < nullherz_traits::MAX_CHANNELS {
                    if src_o < self.current_topology.routing[src_n].output_count {
                         buffer_idx = self.current_topology.routing[src_n].output_indices[src_o] as u32;
                    }
                }

                if buffer_idx == 0 {
                    buffer_idx = self.id_allocator.allocate_buffer_id(1);
                }

                self.handle_topology_command(&Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge {
                    node_idx: src_node_idx,
                    output_idx: src_output_idx,
                    new_buffer_idx: buffer_idx,
                }));
                self.handle_topology_command(&Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge {
                    node_idx: dst_node_idx,
                    input_idx: dst_input_idx,
                    new_buffer_idx: buffer_idx,
                }));
                return true;
            }
            Command::Topology(nullherz_traits::TopologyCommand::Disconnect { node_idx, input_idx }) => {
                 return self.handle_topology_command(&Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge {
                    node_idx,
                    input_idx,
                    new_buffer_idx: 0,
                }));
            }
            Command::Topology(nullherz_traits::TopologyCommand::SetBypass { node_idx, enabled }) => {
                let n_idx = node_idx as usize;
                if n_idx < nullherz_traits::MAX_NODES {
                    self.current_topology.bypass_states[n_idx] = enabled;
                }
                push_mutation(prod, TopologyMutation::SetBypass { node_idx, enabled });
                return true;
            }
            Command::Topology(nullherz_traits::TopologyCommand::SetNodePosition { node_idx, x, y }) => {
                let n_idx = node_idx as usize;
                if n_idx < nullherz_traits::MAX_NODES {
                    self.current_topology.node_positions[n_idx] = Some((x, y));
                }
                push_mutation(prod, TopologyMutation::SetNodePosition { node_idx, x, y });
                return true;
            }
            Command::Topology(nullherz_traits::TopologyCommand::MigrateNode { node_idx, destination }) => {
                let n_idx = node_idx as usize;
                if n_idx < nullherz_traits::MAX_NODES {
                    self.current_topology.node_assignments[n_idx].0.copy_from_slice(&destination);
                }
                // Trigger topology commit to update proxy nodes
                self.handle_topology_command(&Command::Core(nullherz_traits::CoreCommand::CommitTopology));
                return true;
            }
            Command::Core(nullherz_traits::CoreCommand::CommitTopology) => {
                // RT-2: Off-thread compilation
                match GraphCompiler::compile(&self.current_topology) {
                    Ok(plan) => {
                        self.current_topology.plan = plan;
                        push_mutation(prod, TopologyMutation::SetTopology(Arc::new(self.current_topology.clone())));
                        return true;
                    }
                    Err(e) => {
                        eprintln!("Off-thread compilation failed: {}", e);
                    }
                }
            }
            _ => {}
        }
        false
    }
}
