pub mod compiler;
pub use compiler::GraphCompiler;

#[cfg(all(feature = "kani-verify", kani))]
mod verification {
    use super::*;
    use nullherz_traits::{GraphTopology, NodeRouting, CompiledGraphPlan};

    #[kani::proof]
    #[kani::unwind(3)]
    pub fn prove_hazard_verification_detects_overlaps() {
        let mut topo = GraphTopology::default();
        topo.node_count = 2;
        for i in 0..64 { topo.virtual_to_physical[i] = i; }

        // Symbolic shared physical buffer
        let shared_p_idx = kani::any_where(|&idx: &usize| idx < 64);

        // Node 0 writes to shared_p_idx
        topo.routing[0].output_indices[0] = shared_p_idx;
        topo.routing[0].output_count = 1;

        // Node 1 reads from shared_p_idx
        topo.routing[1].input_indices[0] = shared_p_idx;
        topo.routing[1].input_count = 1;

        // Construct a hazardous single-stage plan
        let mut plan = CompiledGraphPlan::default();
        plan.num_stages = 1;
        plan.stage_counts[0] = 2;
        plan.stages[0][0] = 0;
        plan.stages[0][1] = 1;

        let result = GraphCompiler::verify_no_hazards(&topo, &plan);
        kani::assert(result.is_err(), "Must detect RAW hazard for concurrent access to same buffer");
    }
}
use nullherz_traits::{ProcessorTypeId, MAX_NODES, MAX_CHANNELS};
use serde_big_array::BigArray;

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
    #[serde(with = "BigArray")]
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

                    // Reconcile edges
                    for j in 0..node.input_count as usize {
                        if j >= curr.input_count as usize || curr.input_buffers[j] != node.input_buffers[j] {
                            commands.push(nullherz_traits::TopologyCommand::UpdateEdge {
                                node_idx: i as u32,
                                input_idx: j as u32,
                                new_buffer_idx: node.input_buffers[j],
                            });
                        }
                    }
                    for j in 0..node.output_count as usize {
                        if j >= curr.output_count as usize || curr.output_buffers[j] != node.output_buffers[j] {
                            commands.push(nullherz_traits::TopologyCommand::UpdateOutputEdge {
                                node_idx: i as u32,
                                output_idx: j as u32,
                                new_buffer_idx: node.output_buffers[j],
                            });
                        }
                    }
                }
                (Some(_curr), None) => {
                    // Node removal is currently handled by swapping to Dummy/Nothing or just unlinking.
                    // For now, we don't have a specific RemoveNode command, but we could add one.
                }
                _ => {}
            }
        }

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::ProcessorTypeId;

    #[test]
    fn test_reconciliation_minimal_mutations() {
        let mut current = DesiredGraphState { nodes: [None; MAX_NODES] };
        let mut target = DesiredGraphState { nodes: [None; MAX_NODES] };

        let node_a = DesiredNode {
            type_id: ProcessorTypeId::GAIN,
            input_buffers: [0; MAX_CHANNELS],
            output_buffers: [0; MAX_CHANNELS],
            input_count: 1,
            output_count: 1,
        };

        target.nodes[0] = Some(node_a);

        let commands = GraphReconciler::reconcile(&current, &target);
        assert_eq!(commands.len(), 3); // AddNode + UpdateEdge + UpdateOutputEdge

        current.nodes[0] = Some(node_a);
        let commands2 = GraphReconciler::reconcile(&current, &target);
        assert_eq!(commands2.len(), 0); // No changes needed
    }
}
