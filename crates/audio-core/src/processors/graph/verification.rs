#[cfg(all(feature = "kani-verify", kani))]
mod verification {
    use super::*;
    use crate::processors::graph::{GraphTopology, ProcessorNode, GraphExecutor};
    use ipc_layer::AudioBlock;
    use std::sync::atomic::AtomicU64;

    #[kani::proof]
    #[kani::unwind(2)]
    pub fn prove_execute_stage_no_hazards() {
        let mut topo = GraphTopology::default();
        topo.plan.num_stages = 1;
        topo.plan.stage_counts[0] = 2;
        topo.plan.stages[0][0] = 0;
        topo.plan.stages[0][1] = 1;

        let p_out_0 = kani::any_where(|&idx: &usize| idx < crate::MAX_NODES);
        let p_out_1 = kani::any_where(|&idx: &usize| idx < crate::MAX_NODES);
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

        // Verify Disjoint Output Invariant:
        // Parallel execution is only safe if all nodes in a stage write to disjoint physical buffers.
        let stage_nodes = &topo.plan.stages[0].0[..topo.plan.stage_counts[0] as usize];
        for (i, &u_idx) in stage_nodes.iter().enumerate() {
            for &v_idx in stage_nodes.iter().skip(i + 1) {
                let u_routing = &topo.routing[u_idx as usize];
                let v_routing = &topo.routing[v_idx as usize];

                for k in 0..u_routing.output_count {
                    for l in 0..v_routing.output_count {
                        let u_phys = topo.virtual_to_physical[u_routing.output_indices[k] as usize];
                        let v_phys = topo.virtual_to_physical[v_routing.output_indices[l] as usize];
                        kani::assert(u_phys != v_phys, "Write hazard detected: parallel nodes write to same physical buffer");
                    }
                }
            }
        }
    }

    #[kani::proof]
    #[kani::unwind(2)]
    pub fn prove_execute_stage_bounds_safety() {
        let topo = GraphTopology::default();
        let nodes: [ProcessorNode; crate::MAX_NODES] = std::array::from_fn(|_| ProcessorNode::new_empty());
        let mut buffers: [AudioBlock; crate::MAX_NODES] = [AudioBlock { data: [0.0; crate::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] }; crate::MAX_NODES];
        let mut crossfade_buffers: [AudioBlock; crate::MAX_CROSSFADE_BUFFERS] = [AudioBlock { data: [0.0; crate::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] }; crate::MAX_CROSSFADE_BUFFERS];
        let block_x_map = [[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES];
        let telemetry = std::array::from_fn(|_| AtomicU64::new(0));

        // Symbolic sub-block params
        let num_samples = kani::any_where(|&n: &usize| n <= crate::MAX_BLOCK_SIZE);
        let offset = kani::any_where(|&o: &usize| o + num_samples <= crate::MAX_BLOCK_SIZE);

        GraphExecutor::execute_stage(
            &nodes,
            &mut buffers,
            &mut crossfade_buffers,
            &topo,
            0,
            num_samples,
            offset,
            &block_x_map,
            &mut None,
            None,
            None,
            true,
            &telemetry,
        );
    }
}
