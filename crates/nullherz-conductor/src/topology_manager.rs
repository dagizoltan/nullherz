use nullherz_processors::ProcessorRegistry;
use audio_core::processors::TopologyMutation;
use nullherz_traits::Command;

pub struct TopologyManager {
    pub registry: ProcessorRegistry,
    pub topo_producer: Option<ipc_layer::NonRtProducer<TopologyMutation>>,
    pub current_sample_rate: f32,
}

impl TopologyManager {
    pub fn new() -> Self {
        Self {
            registry: ProcessorRegistry::new(),
            topo_producer: None,
            current_sample_rate: 44100.0,
        }
    }

    pub fn handle_topology_command(&mut self, cmd: &Command) -> bool {
        let Some(ref mut prod) = self.topo_producer else { return false; };
        let sr = self.current_sample_rate;

        match *cmd {
            Command::AddNode { processor_type_id, node_idx } => {
                if let Some(processor) = self.registry.create_by_id(processor_type_id.0, node_idx, sr) {
                    let _ = prod.push(TopologyMutation::AddNode { node_idx, processor });
                    return true;
                }
            }
            Command::SwapProcessor { node_idx, processor_type_id } => {
                if let Some(processor) = self.registry.create_by_id(processor_type_id.0, node_idx, sr) {
                    let _ = prod.push(TopologyMutation::SwapProcessor { node_idx, processor });
                    return true;
                }
            }
            Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let _ = prod.push(TopologyMutation::UpdateEdge { node_idx, input_idx, new_buffer_idx });
                return true;
            }
            Command::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let _ = prod.push(TopologyMutation::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx });
                return true;
            }
            _ => {}
        }
        false
    }
}
