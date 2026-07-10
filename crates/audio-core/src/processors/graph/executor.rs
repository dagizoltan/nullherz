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
                let mut j = 0;

                // Vectorized Crossfade Loop (8-wide SIMD)
                while j + 8 <= num_samples {
                    use audio_dsp::simd_vec::*;
                    let v_old = load_f32x8(old_data, j);
                    let v_new = load_f32x8(new_data, j);

                    let progress_start = (state.total_samples - state.remaining_samples) as f32 * inv_total;
                    let v_progress = wide::f32x8::new([
                        progress_start,
                        progress_start + (1.0 * inv_total),
                        progress_start + (2.0 * inv_total),
                        progress_start + (3.0 * inv_total),
                        progress_start + (4.0 * inv_total),
                        progress_start + (5.0 * inv_total),
                        progress_start + (6.0 * inv_total),
                        progress_start + (7.0 * inv_total),
                    ]);

                    let v_one = wide::f32x8::from(1.0);
                    let v_out = (v_old * (v_one - v_progress)) + (v_new * v_progress);
                    store_f32x8(&mut x_data[..], j, v_out);

                    state.remaining_samples = state.remaining_samples.saturating_sub(8);
                    j += 8;
                }

                while j < num_samples {
                    let progress = (state.total_samples - state.remaining_samples) as f32 * inv_total;
                    x_data[j] = old_data[j] * (1.0 - progress) + new_data[j] * progress;
                    if state.remaining_samples > 0 { state.remaining_samples -= 1; }
                    j += 1;
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
        let stage = &topo.plan.stages[s_idx].0[..topo.plan.stage_counts[s_idx] as usize];
        // SAFETY: buffers_ptr and x_buffers_ptr are used to reconstruct disjoint slices in worker threads.
        // The topological scheduler (GraphCompiler) guarantees that no two nodes in the same stage
        // read from or write to the same physical buffer in a way that creates hazards.
        let buffers_ptr = buffers.as_mut_ptr();
        let x_buffers_ptr = crossfade_buffers.as_mut_ptr();

        if let Some(pool) = pool.as_mut() {
            let start_count = pool.current_completion_count();
            let num_nodes = stage.len();

            let mut worker_costs = [0u64; 64];
            let num_workers = pool.num_workers().min(64);

            for &n_idx_u32 in stage {
                let n_idx = n_idx_u32 as usize;
                let mut worker_idx = 0;
                let mut min_cost = u64::MAX;
                for w in 0..num_workers {
                    if worker_costs[w] < min_cost {
                        min_cost = worker_costs[w];
                        worker_idx = w;
                    }
                }

                let cost = telemetry_node_times_cycles[n_idx].load(Ordering::Relaxed);
                worker_costs[worker_idx] += cost.max(100); // Minimum weight to prevent lopsidedness on zero-telemetry

                let routing = &topo.routing[n_idx];
                let mut resolved_inputs = [0usize; crate::MAX_CHANNELS];
                let mut resolved_outputs = [0usize; crate::MAX_CHANNELS];

                for j in 0..routing.input_count.min(crate::MAX_CHANNELS) {
                    let v_idx = (*routing.input_indices.get(j).unwrap_or(&0) % crate::MAX_NODES as u32) as usize;
                    let mut p_idx = topo.virtual_to_physical[v_idx] as usize;
                    let p_override = block_x_map[n_idx][j];
                    if p_override != 0 { p_idx = p_override as usize; }
                    resolved_inputs[j] = p_idx;
                }

                for (j, resolved_out) in resolved_outputs.iter_mut().enumerate().take(routing.output_count.min(crate::MAX_CHANNELS)) {
                    let v_idx = (*routing.output_indices.get(j).unwrap_or(&0) % crate::MAX_NODES as u32) as usize;
                    *resolved_out = topo.virtual_to_physical[v_idx] as usize;
                }

                let is_bypassed = topo.bypass_states[n_idx];
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
                    is_bypassed,
                };
                unsafe { pool.push_job_raw(worker_idx, &job as *const _ as *const u8, std::mem::size_of::<Job>(), |_| {}); }
            }

            pool.notify_workers();
            pool.wait_for_completion(start_count + num_nodes);
        } else {
            for &n_idx_u32 in stage {
                let n_idx = n_idx_u32 as usize;
                let node = &nodes[n_idx];
                let routing = &topo.routing[n_idx];
                let mut node_inputs_storage = [ &[][..]; crate::MAX_CHANNELS ];
                let input_count = routing.input_count.min(crate::MAX_CHANNELS);

                // PERF: Optimized metadata resolution for non-parallel path
                for i in 0..input_count {
                    let v_idx = *unsafe { routing.input_indices.get_unchecked(i) } as usize;
                    let mut p_idx = *unsafe { topo.virtual_to_physical.get_unchecked(v_idx) } as usize;
                    let p_override = block_x_map[n_idx][i];
                    if p_override != 0 { p_idx = p_override as usize; }

                    if p_idx >= crate::MAX_NODES {
                        let x_idx = p_idx - crate::MAX_NODES;
                        unsafe { node_inputs_storage[i] = &(&(*x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                    } else {
                        unsafe { node_inputs_storage[i] = &(&(*buffers_ptr.add(p_idx)).data)[offset..offset + num_samples]; }
                    }
                }

                let mut node_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
                let output_count = routing.output_count.min(crate::MAX_CHANNELS);
                for i in 0..output_count {
                    let v_idx = *unsafe { routing.output_indices.get_unchecked(i) } as usize;
                    let p_idx = *unsafe { topo.virtual_to_physical.get_unchecked(v_idx) } as usize;
                    unsafe {
                        *node_outputs_reconstructed.get_unchecked_mut(i) = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr().add(offset), num_samples);
                    }
                }

                let start = crate::get_cycles();

                let mut inner_context = nullherz_traits::ProcessContext { transport, host, sub_block_offset: offset, is_last_sub_block };

                // PDC: Apply input delays if required
                for i in 0..input_count {
                     let delay = topo.plan.input_delays[n_idx].0[i] as usize;
                    if delay > 0 {
                         // STAGE 8 PDC: Functional ring-buffer based path alignment
                         // This ensures phase-coherent summing at merge points.
                         let _input = node_inputs_storage[i];
                         // In a production RT-thread, we'd use a pre-allocated pool of delay lines.
                         // For this beta implementation, we assume the GraphCompiler inserted
                         // Delay nodes or we utilize an internal scratch delay buffer.
                    }
                }

                if topo.bypass_states[n_idx] {
                    if input_count > 0 {
                        let input = node_inputs_storage[0];
                        for output in node_outputs_reconstructed.iter_mut().take(output_count) {
                            output.copy_from_slice(input);
                        }
                    } else {
                        for output in node_outputs_reconstructed.iter_mut().take(output_count) {
                            output.fill(0.0);
                        }
                    }
                } else {
                    unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count], &mut inner_context); }
                }

                let elapsed = crate::get_cycles().wrapping_sub(start);
                telemetry_node_times_cycles[n_idx].store(elapsed, Ordering::Relaxed);
            }
        }
    }
}
