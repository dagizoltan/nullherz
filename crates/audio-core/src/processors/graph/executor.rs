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
        old_path_buffers: &[AudioBlock; crate::MAX_BUFFERS],
        buffers: &[AudioBlock; crate::MAX_BUFFERS],
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
                    // Constant-Power (Square-Root) Crossfade
                    let v_gain_old = (v_one - v_progress).sqrt();
                    let v_gain_new = v_progress.sqrt();
                    let v_out = (v_old * v_gain_old) + (v_new * v_gain_new);
                    store_f32x8(&mut x_data[..], j, v_out);

                    state.remaining_samples = state.remaining_samples.saturating_sub(8);
                    j += 8;
                }

                while j < num_samples {
                    let progress = (state.total_samples - state.remaining_samples) as f32 * inv_total;
                    let gain_old = (1.0 - progress).sqrt();
                    let gain_new = progress.sqrt();
                    x_data[j] = old_data[j] * gain_old + new_data[j] * gain_new;
                    if state.remaining_samples > 0 { state.remaining_samples -= 1; }
                    j += 1;
                }

                if state.node_idx < crate::MAX_NODES && state.input_idx < crate::MAX_CHANNELS {
                    block_x_map[state.node_idx][state.input_idx] = nullherz_traits::BufferSlot::encode_crossfade(x_buf_idx);
                }

                if state.remaining_samples == 0 { *x_state_opt = None; }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute_stage(
        nodes: &[ProcessorNode; crate::MAX_NODES],
        buffers: &mut [AudioBlock; crate::MAX_BUFFERS],
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
        pdc_lines: &mut crate::processors::graph::buffer_pool::PdcLines,
        pdc_write_pos: usize,
        faulted_states: &[std::sync::atomic::AtomicBool; crate::MAX_NODES],
    ) {
        let stage = &topo.plan.stages[s_idx].0[..topo.plan.stage_counts[s_idx] as usize];
        // SAFETY: buffers_ptr and x_buffers_ptr are used to reconstruct disjoint slices in worker threads.
        // The topological scheduler (GraphCompiler) guarantees that no two nodes in the same stage
        // read from or write to the same physical buffer in a way that creates hazards.
        let buffers_ptr = buffers.as_mut_ptr();
        let x_buffers_ptr = crossfade_buffers.as_mut_ptr();

        // Per-stage cost gate: dispatch to the pool only when the stage's own
        // work out-costs dispatch overhead. Cost is the telemetry-measured
        // sum of the stage's node times; cold start reads 0 → serial (safe
        // default), and a stage escalates to the pool only once measured cost
        // proves it worthwhile. Single-node stages can never parallelize. The
        // generic (test-double) executor keeps the old always-pool behavior.
        let use_pool = match pool.as_mut() {
            None => false,
            Some(_) if stage.len() < 2 => false,
            Some(pool_dyn) => {
                if let Some(tp) = pool_dyn.as_any().downcast_mut::<crate::processors::graph::TaskPool>() {
                    let stage_cost: u64 = stage
                        .iter()
                        .map(|&n| telemetry_node_times_cycles[n as usize].load(Ordering::Relaxed))
                        .sum();
                    stage_cost >= tp.parallel_threshold_cycles
                } else {
                    true
                }
            }
        };

        if use_pool {
            let pool_dyn = pool.as_mut().unwrap();
            let start_count = pool_dyn.current_completion_count();
            let num_nodes = stage.len();

            let mut worker_costs = [0u64; 64];

            // STAGE 7: Latency-Aware Critical-Path Scheduler
            // Identify critical path (highest latency chain) and pin it to worker 0
            // while distributing other nodes to balance load.
            let mut critical_node = None;
            let mut max_node_lat = 0u32;
            for &n_idx_u32 in stage {
                let lat = topo.plan.node_latencies[n_idx_u32 as usize];
                if lat > max_node_lat {
                    max_node_lat = lat;
                    critical_node = Some(n_idx_u32);
                }
            }

            // Job assembly shared by both pool flavours below.
            let pdc_lines_ptr: *mut crate::processors::graph::buffer_pool::PdcLines = pdc_lines;
            let build_job = |n_idx: usize, telemetry_ptr: *mut [std::sync::atomic::AtomicU64; crate::MAX_NODES]| -> Job {
                let routing = &topo.routing[n_idx];
                let mut resolved_inputs = [0usize; crate::MAX_CHANNELS];
                let mut resolved_sidechains = [0usize; crate::MAX_CHANNELS];
                let mut resolved_outputs = [0usize; crate::MAX_CHANNELS];

                for j in 0..routing.input_count.min(crate::MAX_CHANNELS) {
                    let v_idx = routing.input_indices.get(j).copied().unwrap_or_default().index();
                    let mut p_idx = topo.virtual_to_physical[v_idx].index();
                    // block_x_map overrides carry the crossfade sentinel
                    // (MAX_BUFFERS + k); consumers decode via BufferSlot.
                    let p_override = block_x_map[n_idx][j];
                    if p_override != 0 { p_idx = p_override as usize; }
                    resolved_inputs[j] = p_idx;
                }

                for j in 0..routing.sidechain_count.min(crate::MAX_CHANNELS) {
                    let v_idx = routing.sidechain_indices.get(j).copied().unwrap_or_default().index();
                    resolved_sidechains[j] = topo.virtual_to_physical[v_idx].index();
                }

                for (j, resolved_out) in resolved_outputs.iter_mut().enumerate().take(routing.output_count.min(crate::MAX_CHANNELS)) {
                    let v_idx = routing.output_indices.get(j).copied().unwrap_or_default().index();
                    *resolved_out = topo.virtual_to_physical[v_idx].index();
                }

                let is_bypassed = topo.bypass_states[n_idx] || faulted_states[n_idx].load(Ordering::Relaxed);

                Job {
                    node_ptr: &nodes[n_idx] as *const _,
                    num_samples,
                    sub_block_offset: offset,
                    buffers_ptr,
                    x_buffers_ptr,
                    input_indices: resolved_inputs,
                    sidechain_indices: resolved_sidechains,
                    input_delays: topo.plan.input_delays[n_idx].0,
                    output_indices: resolved_outputs,
                    input_count: routing.input_count,
                    output_count: routing.output_count,
                    sidechain_count: routing.sidechain_count,
                    node_idx: n_idx,
                    telemetry_ptr,
                    transport: transport.copied(),
                    host_ptr: host.map(|h| h as *const dyn nullherz_traits::Host),
                    is_last_sub_block,
                    is_bypassed,
                    bypass_state_ptr: &faulted_states[n_idx] as *const std::sync::atomic::AtomicBool,
                    pdc_lines_ptr,
                    pdc_write_pos,
                }
            };

            // Hoisted TaskPool downcast: this dynamic cast used to run up to
            // three times per node, per stage, per sub-block.
            if let Some(tp) = pool_dyn.as_any().downcast_mut::<crate::processors::graph::TaskPool>() {
                let num_workers = tp.worker_producers.len().min(64);
                let mut wake_mask: u64 = 0;

                for &n_idx_u32 in stage {
                    let n_idx = n_idx_u32 as usize;
                    let mut worker_idx = 0usize;

                    // Priority: Pin critical node to a dedicated high-performance worker (idx 0)
                    if Some(n_idx_u32) == critical_node {
                        worker_idx = 0;
                    } else if let Some(assignment) = tp.assignment_cache[n_idx] {
                        worker_idx = assignment.worker_idx as usize;
                    } else {
                        let mut min_cost = u64::MAX;
                        for w in 0..num_workers {
                            if worker_costs[w] < min_cost {
                                min_cost = worker_costs[w];
                                worker_idx = w;
                            }
                        }
                        // Cache the new assignment
                        tp.assignment_cache[n_idx] = Some(crate::processors::graph::pool::StaticAssignment {
                            node_idx: n_idx as u32,
                            worker_idx: worker_idx as u8,
                        });
                    }

                    let cost = telemetry_node_times_cycles[n_idx].load(Ordering::Relaxed);
                    worker_costs[worker_idx] += cost.max(100); // Minimum weight to prevent lopsidedness on zero-telemetry

                    let telemetry_ptr = &tp.worker_telemetry[worker_idx] as *const _ as *mut _;
                    let _ = tp.worker_producers[worker_idx].push(build_job(n_idx, telemetry_ptr));
                    wake_mask |= 1u64 << (worker_idx as u32 & 63);
                }

                tp.notify_workers_masked(wake_mask);
                nullherz_traits::ParallelExecutor::wait_for_completion(tp, start_count + num_nodes);
            } else {
                // Generic ParallelExecutor: no assignment cache or per-worker
                // telemetry storage; least-loaded placement, broadcast wake.
                let num_workers = pool_dyn.num_workers().min(64);
                for &n_idx_u32 in stage {
                    let n_idx = n_idx_u32 as usize;
                    let mut worker_idx = 0usize;
                    if Some(n_idx_u32) != critical_node {
                        let mut min_cost = u64::MAX;
                        for w in 0..num_workers {
                            if worker_costs[w] < min_cost {
                                min_cost = worker_costs[w];
                                worker_idx = w;
                            }
                        }
                    }
                    let cost = telemetry_node_times_cycles[n_idx].load(Ordering::Relaxed);
                    worker_costs[worker_idx] += cost.max(100);

                    let job = build_job(n_idx, telemetry_node_times_cycles as *const _ as *mut _);
                    unsafe { pool_dyn.push_job_raw(worker_idx, &job as *const _ as *const u8, std::mem::size_of::<Job>(), |_| {}); }
                }
                pool_dyn.notify_workers();
                pool_dyn.wait_for_completion(start_count + num_nodes);
            }
        } else {
            for &n_idx_u32 in stage {
                let n_idx = n_idx_u32 as usize;
                let node = &nodes[n_idx];
                let routing = &topo.routing[n_idx];
                let mut node_inputs_storage = [ &[][..]; crate::MAX_CHANNELS * 2 ];
                let input_count = routing.input_count.min(crate::MAX_CHANNELS);
                let sidechain_count = routing.sidechain_count.min(crate::MAX_CHANNELS);


                let mut node_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
                let output_count = routing.output_count.min(crate::MAX_CHANNELS);
                for i in 0..output_count {
                    let v_idx = unsafe { routing.output_indices.get_unchecked(i) }.index();
                    let p_idx = unsafe { topo.virtual_to_physical.get_unchecked(v_idx) }.index();
                    unsafe {
                        *node_outputs_reconstructed.get_unchecked_mut(i) = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr().add(offset), num_samples);
                    }
                }

                let start = crate::get_cycles();

                let mut inner_context = nullherz_traits::ProcessContext { transport, host, sub_block_offset: offset, is_last_sub_block };

                // PDC: Apply input delays if required
                for i in 0..input_count + sidechain_count {
                    let v_idx = if i < input_count {
                        unsafe { routing.input_indices.get_unchecked(i) }.index()
                    } else {
                        unsafe { routing.sidechain_indices.get_unchecked(i - input_count) }.index()
                    };
                    let mut p_idx = unsafe { topo.virtual_to_physical.get_unchecked(v_idx) }.index();
                    if i < input_count {
                        let p_override = block_x_map[n_idx][i];
                        if p_override != 0 { p_idx = p_override as usize; }
                    }

                    match nullherz_traits::BufferSlot::from_raw(p_idx) {
                        nullherz_traits::BufferSlot::Crossfade(x_idx) => {
                            unsafe { node_inputs_storage[i] = &(&(*x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                        }
                        nullherz_traits::BufferSlot::Pool(p_idx) => {
                            unsafe { node_inputs_storage[i] = &(&(*buffers_ptr.add(p_idx)).data)[offset..offset + num_samples]; }
                        }
                    }
                }

                for i in 0..input_count {
                    let delay_f = topo.plan.input_delays[n_idx].0[i];
                    if delay_f > 0.0 && delay_f < (crate::processors::graph::buffer_pool::MAX_PDC_SAMPLES as f32 - 4.0) {
                        // STAGE 8 PDC: Functional ring-buffer based path alignment
                        let input = node_inputs_storage[i];
                        let max_len = crate::processors::graph::buffer_pool::MAX_PDC_SAMPLES;

                        // Write to delay line
                        let mut w_pos = (pdc_write_pos.wrapping_sub(num_samples)) % max_len;
                        for &sample in input {
                            pdc_lines.set_sample(n_idx, i, w_pos, sample);
                            w_pos = (w_pos + 1) % max_len;
                        }

                        let delay_int = delay_f.floor() as usize;
                        let delay_frac = delay_f - delay_f.floor();

                        // Read from delay line with offset (into the
                        // pool-owned scratch; rows are fully overwritten
                        // before they are read below)
                        let mut r_pos = (pdc_write_pos.wrapping_sub(num_samples).wrapping_sub(delay_int)) % max_len;
                        for j in 0..num_samples {
                            pdc_lines.scratch[i][j] = pdc_lines.get_sample_interpolated(n_idx, i, r_pos, delay_frac);
                            r_pos = (r_pos + 1) % max_len;
                        }
                    }
                }

                for i in 0..input_count {
                    let delay_f = topo.plan.input_delays[n_idx].0[i];
                    if delay_f > 0.0 && delay_f < (crate::processors::graph::buffer_pool::MAX_PDC_SAMPLES as f32 - 4.0) {
                        node_inputs_storage[i] = &pdc_lines.scratch[i][..num_samples];
                    }
                }

                if topo.bypass_states[n_idx] || faulted_states[n_idx].load(Ordering::Relaxed) {
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
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        unsafe {
                            (*node.processor.get()).process(
                                &node_inputs_storage[..input_count + sidechain_count],
                                &mut node_outputs_reconstructed[..output_count],
                                &mut inner_context
                            );
                        }
                    }));

                    if result.is_err() {
                        // Debug-only: `eprintln!` is a blocking write(2) under the
                        // stderr lock — never on the release RT thread. The fault
                        // is recorded lock-free in `faulted_states` below (the
                        // state the engine actually acts on); this line is just a
                        // dev diagnostic, gated like `assert_finite_block!`.
                        #[cfg(debug_assertions)]
                        eprintln!(
                            "Audio Engine: caught panic in process() of node_idx {} (processor type: '{}')",
                            n_idx,
                            unsafe { (*node.processor.get()).processor_type() }
                        );

                        // Zero-fill reconstructed outputs
                        for output in node_outputs_reconstructed.iter_mut().take(output_count) {
                            output.fill(0.0);
                        }

                        // Permanently bypass the node
                        faulted_states[n_idx].store(true, Ordering::Relaxed);
                    } else {
                        for output in node_outputs_reconstructed.iter().take(output_count) {
                            crate::assert_finite_block!(output, n_idx);
                        }
                    }
                }

                let elapsed = crate::get_cycles().wrapping_sub(start);
                telemetry_node_times_cycles[n_idx].store(elapsed, Ordering::Relaxed);
            }
        }
    }
}
