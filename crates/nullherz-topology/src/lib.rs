use nullherz_traits::{ProcessorTypeId, MAX_NODES, MAX_CHANNELS};

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DesiredNode {
    pub type_id: ProcessorTypeId,
    pub input_buffers: [u32; MAX_CHANNELS],
    pub output_buffers: [u32; MAX_CHANNELS],
    pub input_count: u32,
    pub output_count: u32,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DesiredGraphState {
    pub nodes: [Option<DesiredNode>; MAX_NODES],
}

pub struct GraphReconciler;

impl GraphReconciler {
    pub fn reconcile(current: &DesiredGraphState, target: &DesiredGraphState) -> Vec<nullherz_traits::TopologyCommand> {
        let mut commands = Vec::new();

        for i in 0..MAX_NODES {
            match (current.nodes[i], target.nodes[i]) {
                (None, Some(node)) => {
                    commands.push(nullherz_traits::TopologyCommand::AddNode {
                        processor_type_id: node.type_id,
                        node_idx: i as u32,
                    });
                    for j in 0..node.input_count as usize {
                        commands.push(nullherz_traits::TopologyCommand::UpdateEdge {
                            node_idx: i as u32,
                            input_idx: j as u32,
                            new_buffer_idx: node.input_buffers[j],
                        });
                    }
                    for j in 0..node.output_count as usize {
                        commands.push(nullherz_traits::TopologyCommand::UpdateOutputEdge {
                            node_idx: i as u32,
                            output_idx: j as u32,
                            new_buffer_idx: node.output_buffers[j],
                        });
                    }
                }
                (Some(curr), Some(node)) => {
                    if curr.type_id != node.type_id {
                        commands.push(nullherz_traits::TopologyCommand::SwapProcessor {
                            node_idx: i as u32,
                            processor_type_id: node.type_id,
                        });
                    }
                    // Reconcile edges...
                }
                _ => {}
            }
        }

        commands
    }
}
