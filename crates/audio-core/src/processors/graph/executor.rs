use std::sync::atomic::Ordering;
use ipc_layer::AudioBlock;
use crate::processors::graph::{GraphTopology, TaskPool, Job, ProcessorNode};

pub struct GraphExecutor {}

impl GraphExecutor {
    pub fn resolve_crossfades(
        topologies: &mut [GraphTopology; 2],
        topo_idx: usize,
        offset: usize,
        num_samples: usize,
        old_path_buffers: &[AudioBlock; crate::MAX_NODES],
        buffers: &[AudioBlock; crate::MAX_NODES],
        crossfade_buffers: &mut [AudioBlock; 8],
        block_x_map: &mut [[u8; crate::MAX_CHANNELS]; crate::MAX_NODES]
    ) {
        for i in 0..8 {
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

                if state.node_idx < 64 && state.input_idx < 16 {
                    block_x_map[state.node_idx][state.input_idx] = (64 + x_buf_idx) as u8;
                }

                if state.remaining_samples == 0 { *x_state_opt = None; }
            }
        }
    }

    pub fn execute_stage(
        nodes: &[ProcessorNode; crate::MAX_NODES],
        buffers: &mut [AudioBlock; crate::MAX_NODES],
        crossfade_buffers: &mut [AudioBlock; 8],
        topo: &GraphTopology,
        s_idx: usize,
        num_samples: usize,
        offset: usize,
        block_x_map: &[[u8; crate::MAX_CHANNELS]; crate::MAX_NODES],
        pool: &mut Option<&mut TaskPool>,
        transport: Option<&nullherz_traits::Transport>,
        is_last_sub_block: bool,
        telemetry_node_times_cycles: &[std::sync::atomic::AtomicU64; crate::MAX_NODES],
    ) {
        let stage = &topo.stages[s_idx][..topo.stage_counts[s_idx]];
        // SAFETY: buffers_ptr and x_buffers_ptr are used to reconstruct disjoint slices in worker threads.
        // The topological scheduler (GraphCompiler) guarantees that no two nodes in the same stage
        // read from or write to the same physical buffer in a way that creates hazards.
        let buffers_ptr = buffers.as_mut_ptr();
        let x_buffers_ptr = crossfade_buffers.as_mut_ptr();

        if let Some(pool) = pool.as_mut() {
            let start_count = pool.completion.load(Ordering::Acquire);
            let num_nodes = stage.len();
            let mut workers_to_wake = [false; 64];

            for (i, &n_idx) in stage.iter().enumerate() {
                let worker_idx = i % pool.worker_producers.len();
                workers_to_wake[worker_idx] = true;
                let routing = &topo.routing[n_idx];
                let mut resolved_inputs = [0usize; 16];
                let mut resolved_outputs = [0usize; 16];

                for j in 0..routing.input_count.min(crate::MAX_CHANNELS) {
                    let v_idx = routing.input_indices[j].min(crate::MAX_NODES - 1);
                    let mut p_idx = topo.virtual_to_physical[v_idx];
                    let p_override = block_x_map[n_idx][j];
                    if p_override != 0 { p_idx = p_override as usize; }
                    resolved_inputs[j] = p_idx;
                }

                for (j, resolved_out) in resolved_outputs.iter_mut().enumerate().take(routing.output_count.min(crate::MAX_CHANNELS)) {
                    let v_idx = routing.output_indices[j].min(crate::MAX_NODES - 1);
                    *resolved_out = topo.virtual_to_physical[v_idx];
                }

                // SAFETY: We pass a raw pointer to the ProcessorNode. The lifetime of nodes is guaranteed
                // for the duration of the engine cycle, and the stage fencing in TaskPool ensures
                // exclusive access to the processor.
                let _ = pool.worker_producers[worker_idx].push(Job {
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
                    is_last_sub_block,
                });
            }

            for (idx, &should_wake) in workers_to_wake.iter().enumerate().take(pool.worker_producers.len()) {
                if should_wake { pool.worker_wake_fds[idx].notify(); }
            }

            let target = start_count + num_nodes;
            while pool.completion.load(Ordering::Acquire) < target {
                let _ = pool.completion_fd.wait();
            }
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

                    if p_idx >= 64 {
                        let x_idx = p_idx - 64;
                        if x_idx < 8 {
                            unsafe { node_inputs_storage[i] = &(&(*x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                        }
                    } else if p_idx < 64 {
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

                let mut inner_context = nullherz_traits::ProcessContext { transport, sub_block_offset: offset, is_last_sub_block };
                unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count], &mut inner_context); }

                let elapsed = crate::get_cycles().wrapping_sub(start);
                telemetry_node_times_cycles[n_idx].store(elapsed, Ordering::Relaxed);
            }
        }
    }
}
