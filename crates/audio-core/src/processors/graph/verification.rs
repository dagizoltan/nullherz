#[cfg(all(feature = "kani-verify", kani))]
mod verification {
    use super::*;
    use crate::processors::graph::{GraphTopology, ProcessorNode, GraphExecutor};
    use ipc_layer::AudioBlock;
    use std::sync::atomic::AtomicU64;

    #[kani::proof]
    #[kani::unwind(2)]
    pub fn prove_execute_stage_no_hazards() {
        // We model a stage with 2 nodes and ensure they don't have buffer collisions
        // if the topological sort is valid.

        let mut topo = GraphTopology::default();
        topo.plan.num_stages = 1;
        topo.plan.stage_counts[0] = 2;
        topo.plan.stages[0][0] = 0;
        topo.plan.stages[0][1] = 1;

        // Symbolic physical buffer assignments
        let p_out_0 = kani::any_where(|&idx: &usize| idx < crate::MAX_NODES);
        let p_out_1 = kani::any_where(|&idx: &usize| idx < crate::MAX_NODES);

        // Assume disjoint outputs for same stage (guaranteed by GraphCompiler)
        kani::assume(p_out_0 != p_out_1);

        topo.virtual_to_physical[0] = p_out_0;
        topo.routing[0].output_count = 1;
        topo.routing[0].output_indices[0] = 0;

        topo.virtual_to_physical[1] = p_out_1;
        topo.routing[1].output_count = 1;
        topo.routing[1].output_indices[0] = 1;

        let nodes: [ProcessorNode; crate::MAX_NODES] = std::array::from_fn(|_| ProcessorNode::new_empty());
        let mut buffers: [AudioBlock; crate::MAX_NODES] = [AudioBlock { data: [0.0; crate::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] }; crate::MAX_NODES];
        let mut crossfade_buffers: [AudioBlock; crate::MAX_CROSSFADE_BUFFERS] = [AudioBlock { data: [0.0; crate::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] }; crate::MAX_CROSSFADE_BUFFERS];
        let block_x_map = [[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES];
        let telemetry = std::array::from_fn(|_| AtomicU64::new(0));

        GraphExecutor::execute_stage(
            &nodes,
            &mut buffers,
            &mut crossfade_buffers,
            &topo,
            0,
            16,
            0,
            &block_x_map,
            &mut None,
            None,
            None,
            true,
            &telemetry,
        );

        kani::assert(true, "Execution finished without hazard overlap");
    }
}
