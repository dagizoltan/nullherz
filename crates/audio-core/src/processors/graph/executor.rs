use std::sync::atomic::Ordering;
use ipc_layer::AudioBlock;
use crate::processors::graph::{GraphTopology, Job, ProcessorNode};

pub struct GraphExecutor {}

impl GraphExecutor {
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_crossfades(
        topologies: &mut [GraphTopology; 2],
        topo_idx: usize,
        offset: usize,
        num_samples: usize,
        old_path_buffers: &[AudioBlock; crate::MAX_NODES],
        buffers: &[AudioBlock; crate::MAX_NODES],
        crossfade_buffers: &mut [AudioBlock; crate::MAX_CROSSFADE_BUFFERS],
        block_x_map: &mut [[u8; crate::MAX_CHANNELS]; crate::MAX_NODES]
    ) {
        for i in 0..crate::MAX_CROSSFADE_BUFFERS {
            let x_state_opt = &mut topologies[topo_idx].crossfades[i];
            if let Some(state) = x_state_opt {
                let x_buf_idx = i;
                let old_data = &old_path_buffers[state.old_buffer_idx].data[offset..offset + num_samples];
                let new_data = &buffers[state.new_buffer_idx].data[offset..offset + num_samples];
                let x_data = &mut crossfade_buffers[x_buf_idx].data[..num_samples];

                let inv_total = 1.0 / state.total_samples as f32;
                for j in 0..num_samples {
                    let progress = (state.total_samples - state.remaining_samples) as f32 * inv_total;
                    x_data[j] = old_data[j] * (1.0 - progress) + new_data[j] * progress;
                    if state.remaining_samples > 0 { state.remaining_samples -= 1; }
                }

                if state.node_idx < crate::MAX_NODES && state.input_idx < crate::MAX_CHANNELS {
                    block_x_map[state.node_idx][state.input_idx] = (crate::MAX_NODES + x_buf_idx) as u8;
                }

                if state.remaining_samples == 0 { *x_state_opt = None; }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute_stage(
        nodes: &[ProcessorNode; crate::MAX_NODES],
        buffers: &mut [AudioBlock; crate::MAX_NODES],
        crossfade_buffers: &mut [AudioBlock; crate::MAX_CROSSFADE_BUFFERS],
        topo: &GraphTopology,
        s_idx: usize,
        num_samples: usize,
        offset: usize,
        block_x_map: &[[u8; crate::MAX_CHANNELS]; crate::MAX_NODES],
        pool: &mut Option<&mut (dyn nullherz_traits::ParallelExecutor + '_)>,
        transport: Option<&nullherz_traits::Transport>,
        host: Option<&dyn nullherz_traits::Host>,
        is_last_sub_block: bool,
        telemetry_node_times_cycles: &[std::sync::atomic::AtomicU64; crate::MAX_NODES],
    ) {
        let stage = &topo.plan.stages[s_idx][..topo.plan.stage_counts[s_idx]];
        // SAFETY: buffers_ptr and x_buffers_ptr are used to reconstruct disjoint slices in worker threads.
        // The topological scheduler (GraphCompiler) guarantees that no two nodes in the same stage
        // read from or write to the same physical buffer in a way that creates hazards.
        let buffers_ptr = buffers.as_mut_ptr();
        let x_buffers_ptr = crossfade_buffers.as_mut_ptr();

        if let Some(pool) = pool.as_mut() {
            let start_count = pool.current_completion_count();
            let num_nodes = stage.len();

            for (i, &n_idx) in stage.iter().enumerate() {
                let worker_idx = i % pool.num_workers();
                let routing = &topo.routing[n_idx];
                let mut resolved_inputs = [0usize; crate::MAX_CHANNELS];
                let mut resolved_outputs = [0usize; crate::MAX_CHANNELS];

                for j in 0..routing.input_count.min(crate::MAX_CHANNELS) {
                    let v_idx = *routing.input_indices.get(j).unwrap_or(&0) % crate::MAX_NODES;
                    let mut p_idx = topo.virtual_to_physical[v_idx];
                    let p_override = block_x_map[n_idx][j];
                    if p_override != 0 { p_idx = p_override as usize; }
                    resolved_inputs[j] = p_idx;
                }

                for (j, resolved_out) in resolved_outputs.iter_mut().enumerate().take(routing.output_count.min(crate::MAX_CHANNELS)) {
                    let v_idx = *routing.output_indices.get(j).unwrap_or(&0) % crate::MAX_NODES;
                    *resolved_out = topo.virtual_to_physical[v_idx];
                }

                let job = Job {
                    node_ptr: &nodes[n_idx] as *const _,
                    num_samples,
                    sub_block_offset: offset,
                    buffers_ptr,
                    x_buffers_ptr,
                    input_indices: resolved_inputs,
                    output_indices: resolved_outputs,
                    input_count: routing.input_count,
                    output_count: routing.output_count,
                    node_idx: n_idx,
                    telemetry_ptr: telemetry_node_times_cycles as *const _,
                    transport: transport.copied(),
                    host_ptr: host.map(|h| h as *const dyn nullherz_traits::Host),
                    is_last_sub_block,
                };
                let _ = pool.push_job(worker_idx, Box::new(job));
            }

            pool.notify_workers();
            pool.wait_for_completion(start_count + num_nodes);
        } else {
            for &n_idx in stage {
                let node = &nodes[n_idx];
                let routing = &topo.routing[n_idx];
                let mut node_inputs_storage = [ &[][..]; crate::MAX_CHANNELS ];
                let input_count = routing.input_count.min(crate::MAX_CHANNELS);
                for i in 0..input_count {
                    let v_idx = routing.input_indices.get(i).copied().unwrap_or(0).min(crate::MAX_NODES - 1);
                    let mut p_idx = topo.virtual_to_physical[v_idx];
                    let p_override = block_x_map[n_idx][i];
                    if p_override != 0 { p_idx = p_override as usize; }

                    if p_idx >= crate::MAX_NODES {
                        let x_idx = p_idx - crate::MAX_NODES;
                        if x_idx < crate::MAX_CROSSFADE_BUFFERS {
                            unsafe { node_inputs_storage[i] = &(&(*x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                        }
                    } else if p_idx < crate::MAX_NODES {
                        unsafe { node_inputs_storage[i] = &(&(*buffers_ptr.add(p_idx)).data)[offset..offset + num_samples]; }
                    }
                }
                let mut node_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
                let output_count = routing.output_count.min(crate::MAX_CHANNELS);
                for (i, node_out) in node_outputs_reconstructed.iter_mut().enumerate().take(output_count) {
                    let v_idx = routing.output_indices.get(i).copied().unwrap_or(0).min(crate::MAX_NODES - 1);
                    let p_idx = topo.virtual_to_physical.get(v_idx).copied().unwrap_or(0).min(crate::MAX_NODES - 1);
                    unsafe {
                        *node_out = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr().add(offset), num_samples);
                    }
                }

                let start = crate::get_cycles();

                let mut inner_context = nullherz_traits::ProcessContext { transport, host, sub_block_offset: offset, is_last_sub_block };
                unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count], &mut inner_context); }

                let elapsed = crate::get_cycles().wrapping_sub(start);
                telemetry_node_times_cycles[n_idx].store(elapsed, Ordering::Relaxed);
            }
        }
    }
}
