use nullherz_topology::GraphCompiler;
use std::sync::Arc;
use nullherz_processors::ProcessorRegistry;
use audio_core::processors::{TopologyMutation, GraphTopology, NodeRouting};
use nullherz_traits::Command;

pub struct TopologyManager {
    pub registry: ProcessorRegistry,
    pub topo_producer: Option<ipc_layer::NonRtProducer<TopologyMutation>>,
    pub current_sample_rate: f32,
    pub current_topology: GraphTopology,
    pub active_node_types: std::collections::HashMap<u32, u32>,
}

impl Default for TopologyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TopologyManager {
    pub fn new() -> Self {
        let mut v2p = [0usize; nullherz_traits::MAX_NODES];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i; }

        Self {
            registry: ProcessorRegistry::new(),
            topo_producer: None,
            current_sample_rate: 44100.0,
            active_node_types: std::collections::HashMap::new(),
            current_topology: GraphTopology {
                routing: [NodeRouting {
                    input_indices: [0; nullherz_traits::MAX_CHANNELS],
                    output_indices: [0; nullherz_traits::MAX_CHANNELS],
                    input_count: 0,
                    output_count: 0
                }; nullherz_traits::MAX_NODES],
                virtual_to_physical: v2p,
                plan: Default::default(),
                crossfades: [None; 8],
                node_count: 0,
            },
        }
    }

    pub fn handle_topology_command(&mut self, cmd: &Command) -> bool {
        let Some(ref mut prod) = self.topo_producer else { return false; };
        let sr = self.current_sample_rate;

        match *cmd {
            Command::AddNode { processor_type_id, node_idx } => {
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
                    let _ = prod.push(TopologyMutation::AddNode { node_idx, processor });
                    return true;
                }
            }
            Command::SwapProcessor { node_idx, processor_type_id } => {
                if let Some(processor) = self.registry.create_by_id(processor_type_id.0, node_idx, sr) {
                    self.active_node_types.insert(node_idx, processor_type_id.0);
                    let _ = prod.push(TopologyMutation::SwapProcessor { node_idx, processor });
                    return true;
                }
            }
            Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let n_idx = node_idx as usize;
                let i_idx = input_idx as usize;
                if n_idx < nullherz_traits::MAX_NODES && i_idx < nullherz_traits::MAX_CHANNELS {
                    self.current_topology.routing[n_idx].input_indices[i_idx] = new_buffer_idx as usize;
                    if i_idx >= self.current_topology.routing[n_idx].input_count {
                        self.current_topology.routing[n_idx].input_count = i_idx + 1;
                    }
                }
                let _ = prod.push(TopologyMutation::UpdateEdge { node_idx, input_idx, new_buffer_idx });
                return true;
            }
            Command::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let n_idx = node_idx as usize;
                let o_idx = output_idx as usize;
                if n_idx < nullherz_traits::MAX_NODES && o_idx < nullherz_traits::MAX_CHANNELS {
                    self.current_topology.routing[n_idx].output_indices[o_idx] = new_buffer_idx as usize;
                    if o_idx >= self.current_topology.routing[n_idx].output_count {
                        self.current_topology.routing[n_idx].output_count = o_idx + 1;
                    }
                }
                let _ = prod.push(TopologyMutation::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx });
                return true;
            }
            Command::CommitTopology => {
                // RT-2: Off-thread compilation
                if let Ok(plan) = GraphCompiler::compile(&self.current_topology) {
                    self.current_topology.plan = plan;
                    let _ = prod.push(TopologyMutation::SetTopology(Arc::new(self.current_topology)));
                    return true;
                } else {
                    eprintln!("Off-thread compilation failed! Cycle detected?");
                }
            }
            _ => {}
        }
        false
    }
}
