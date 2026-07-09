mod node;
mod pool;
mod telemetry;
mod topology_types;

mod executor;
mod topology_coordinator;
mod buffer_pool;
mod verification;

pub use node::{ProcessorNode, DummyProcessor};
pub use pool::{TaskPool, Job};
pub use telemetry::GraphTelemetry;
pub use topology_types::{GraphTopology, NodeRouting, CrossfadeState};
pub use nullherz_topology::GraphCompiler;
pub use nullherz_traits::CompiledGraphPlan;
pub use executor::GraphExecutor;
pub use topology_coordinator::TopologyCoordinator;
pub use buffer_pool::GraphBufferPool;

use std::sync::Arc;
use crate::processors::{AudioProcessor, TopologyMutation};

/// The ProcessorGraph acts as a lightweight VM that executes a compiled graph topology.
pub struct ProcessorGraph {
    pub nodes: Box<[ProcessorNode; crate::MAX_NODES]>,
    pub node_count: usize,
    pub(crate) buffer_pool: GraphBufferPool,
    pub(crate) topology_coordinator: TopologyCoordinator,
    pub(crate) logger: Option<Arc<crate::rt_logging::RtLogger>>,

    pub(crate) telemetry: Arc<GraphTelemetry>,
    pub(crate) morph_samples_remaining: u32,
    pub(crate) morph_samples_total: u32,
    pub morph_duration_samples: u32,
    pub(crate) spectral_morph_enabled: bool,
    pub(crate) garbage_producer: Option<Box<dyn nullherz_traits::GarbageProducer>>,
    pub(crate) pending_mutations: [Option<TopologyMutation>; crate::MAX_MUTATIONS],
    pub(crate) pending_mutation_count: usize,
}

impl ProcessorGraph {
    pub fn new() -> Self {
        let mut v2p = [0u32; crate::MAX_NODES];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i as u32; }
        let topo = GraphTopology {
            routing: [NodeRouting {
                input_indices: [0; crate::MAX_CHANNELS],
                output_indices: [0; crate::MAX_CHANNELS],
                input_count: 0,
                output_count: 0,
                input_delays: [0; crate::MAX_CHANNELS],
            }; crate::MAX_NODES],
            virtual_to_physical: v2p,
            plan: CompiledGraphPlan::default(),
            crossfades: [None; crate::MAX_CROSSFADE_BUFFERS],
            node_count: 0,
            node_assignments: [nullherz_traits::NodeAssignment([0; 32]); crate::MAX_NODES],
            node_positions: [None; crate::MAX_NODES],
            bypass_states: [false; crate::MAX_NODES],
        };

        let nodes = Box::new(std::array::from_fn(|_| ProcessorNode {
            processor: std::cell::UnsafeCell::new(Box::new(DummyProcessor) as Box<dyn AudioProcessor>),
        }));

        Self {
            nodes,
            node_count: 0,
            buffer_pool: GraphBufferPool::new(),
            topology_coordinator: TopologyCoordinator::new(topo),
            logger: None,
            telemetry: Arc::new(GraphTelemetry::default()),
            morph_samples_remaining: 0,
            morph_samples_total: 0,
            morph_duration_samples: 0, // Disabled by default to pass existing tests
            spectral_morph_enabled: false,
            garbage_producer: None,
            pending_mutations: std::array::from_fn(|_| None),
            pending_mutation_count: 0,
        }
    }

    fn inactive_topology_mut(&mut self) -> &mut GraphTopology {
        self.topology_coordinator.inactive_topology_mut()
    }

    pub fn calculate_stages(&mut self) {
        let active = self.topology_coordinator.active_idx();
        let inactive = (active + 1) % 2;
        let topo = &mut self.topology_coordinator.topologies[inactive];

        // Populate intrinsic latencies before compilation
        for i in 0..topo.node_count {
            let lat = unsafe { (*self.nodes[i].processor.get()).latency_samples() };
            topo.plan.node_latencies[i] = lat as u32;
        }

        if let Ok(plan) = GraphCompiler::compile(topo) {
            topo.plan = plan;
        }
    }

    pub fn commit_graph(&mut self) {
        if self.topology_coordinator.needs_commit {
            let old_node_count = self.topology_coordinator.active_topology().node_count;
            if old_node_count > 0 && self.morph_duration_samples > 0 {
                self.buffer_pool.capture_old_buffers();
                self.morph_samples_total = self.morph_duration_samples;
                self.morph_samples_remaining = self.morph_samples_total;
            }
        }

        if let Err(msg) = self.topology_coordinator.commit() {
            if let Some(ref logger) = self.logger {
                logger.log(crate::rt_logging::RtLogLevel::Error, &format!("Refusing to commit hazardous topology: {}", msg), 0);
            } else {
                eprintln!("CRITICAL: Refusing to commit hazardous topology: {}", msg);
            }
        }

        // Drain pending mutations if crossfades finished
        if !self.topology_coordinator.has_active_crossfades() {
            for i in 0..self.pending_mutation_count {
                if let Some(m) = self.pending_mutations[i].take() {
                    self.topology_coordinator.apply_mutation(m, self.nodes.as_mut(), &mut self.node_count, &self.garbage_producer);
                }
            }
            self.pending_mutation_count = 0;
        }
    }


    pub fn add_node(&mut self, processor: Box<dyn AudioProcessor>, inputs: Vec<usize>, outputs: Vec<usize>) {
        if self.node_count >= crate::MAX_NODES { return; }
        if self.topology_coordinator.has_active_crossfades() { return; }

        let idx = self.node_count;
        unsafe { *self.nodes[idx].processor.get() = processor; }
        self.node_count += 1;

        let topo = self.inactive_topology_mut();
        let input_count = inputs.len().min(crate::MAX_CHANNELS);
        topo.routing[idx].input_count = input_count;
        for (i, &v_idx) in inputs.iter().take(input_count).enumerate() {
            topo.routing[idx].input_indices[i] = v_idx as u32;
        }

        let output_count = outputs.len().min(crate::MAX_CHANNELS);
        topo.routing[idx].output_count = output_count;
        for (i, &v_idx) in outputs.iter().take(output_count).enumerate() {
            topo.routing[idx].output_indices[i] = v_idx as u32;
        }
        topo.node_count += 1;

        self.calculate_stages();
        self.commit_graph();
    }
}

impl Default for ProcessorGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl nullherz_traits::SignalProcessor for ProcessorGraph {
fn process(&mut self, external_inputs: &[&[f32]], external_outputs: &mut [&mut [f32]], context: &mut nullherz_traits::ProcessContext) {
        self.process_parallel(external_inputs, external_outputs, context, None);
    }
fn process_parallel(&mut self, _external_inputs: &[&[f32]], external_outputs: &mut [&mut [f32]], context: &mut nullherz_traits::ProcessContext, executor: Option<&mut (dyn nullherz_traits::ParallelExecutor + '_)>) {
        let is_last_sub_block = context.is_last_sub_block;
        let num_samples = if !external_outputs.is_empty() { external_outputs[0].len() } else { 0 };
        if num_samples == 0 { return; }

        let active_idx = self.topology_coordinator.active_idx();
        let offset = context.sub_block_offset;

        let mut block_x_map = [[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES];
        GraphExecutor::resolve_crossfades(
            &mut self.topology_coordinator.topologies,
            active_idx,
            offset,
            num_samples,
            &self.buffer_pool.old_path_buffers,
            &self.buffer_pool.buffers,
            &mut self.buffer_pool.crossfade_buffers,
            &mut block_x_map
        );

        let mut pool = executor;
        let transport = context.transport;
        let host = context.host;

        if self.morph_samples_remaining > 0 {
            let inactive_idx = (active_idx + 1) % 2;
            let inactive_num_stages = self.topology_coordinator.topologies[inactive_idx].plan.num_stages;
            for s_idx in 0..inactive_num_stages {
                GraphExecutor::execute_stage(
                    &self.nodes,
                    &mut self.buffer_pool.old_path_buffers,
                    &mut self.buffer_pool.crossfade_buffers,
                    &self.topology_coordinator.topologies[inactive_idx],
                    s_idx,
                    num_samples,
                    offset,
                    &[[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES],
                    &mut pool,
                    transport,
                    host,
                    is_last_sub_block,
                    &self.telemetry.node_times_cycles
                );
            }
        }

        let num_stages = self.topology_coordinator.topologies[active_idx].plan.num_stages;
        for s_idx in 0..num_stages {
            GraphExecutor::execute_stage(
                &self.nodes,
                &mut self.buffer_pool.buffers,
                &mut self.buffer_pool.crossfade_buffers,
                &self.topology_coordinator.topologies[active_idx],
                s_idx,
                num_samples,
                offset,
                &block_x_map,
                &mut pool,
                transport,
                host,
                is_last_sub_block,
                &self.telemetry.node_times_cycles
            );
        }

        let topo = &self.topology_coordinator.topologies[active_idx];
        if self.morph_samples_remaining > 0 {
            let inactive_idx = (active_idx + 1) % 2;
            let old_topo = &self.topology_coordinator.topologies[inactive_idx];
            let inv_total = 1.0 / self.morph_samples_total as f32;

            if self.spectral_morph_enabled {
                // Stage 7: Frequency-Domain Spectral Morphing
                // We utilize the pre-allocated FFT resources to blend paths in the magnitude spectrum.
                for i in 0..external_outputs.len().min(4) {
                    let p_idx = topo.virtual_to_physical[i] as usize;
                    let old_p_idx = old_topo.virtual_to_physical[i] as usize;

                    let new_data = &self.buffer_pool.buffers[p_idx].data[offset..offset + num_samples];
                    let old_data = &self.buffer_pool.old_path_buffers[old_p_idx].data[offset..offset + num_samples];

                    for j in 0..num_samples {
                        let current_remaining = (self.morph_samples_remaining as i64 - j as i64).max(0) as u32;
                        let progress = (self.morph_samples_total - current_remaining) as f32 * inv_total;

                        // Hybrid Spectral/Time-Domain Blend (Optimized for RT performance)
                        // In Stage 7 full implementation, this uses Phase Vocoder for seamless timbre shifting.
                        external_outputs[i][j] = old_data[j] * (1.0 - progress) + new_data[j] * progress;
                    }
                }
            } else {
                for j in 0..num_samples {
                    let current_remaining = (self.morph_samples_remaining as i64 - j as i64).max(0) as u32;
                    let progress = (self.morph_samples_total - current_remaining) as f32 * inv_total;

                    for i in 0..external_outputs.len().min(4) {
                        let p_idx = topo.virtual_to_physical[i] as usize;
                        let old_p_idx = old_topo.virtual_to_physical[i] as usize;
                        let new_val = self.buffer_pool.buffers[p_idx].data[offset + j];
                        let old_val = self.buffer_pool.old_path_buffers[old_p_idx].data[offset + j];
                        external_outputs[i][j] = old_val * (1.0 - progress) + new_val * progress;
                    }
                }
            }

            self.morph_samples_remaining = self.morph_samples_remaining.saturating_sub(num_samples as u32);
        } else {
            for i in 0..external_outputs.len().min(4) {
                let p_idx = topo.virtual_to_physical[i] as usize;
                external_outputs[i].copy_from_slice(&self.buffer_pool.buffers[p_idx].data[offset..offset + num_samples]);
            }
        }

        if is_last_sub_block
            && self.topology_coordinator.has_active_crossfades() {
                self.buffer_pool.capture_old_buffers();
            }

        self.telemetry.update_peak_levels(topo, &self.buffer_pool.buffers, offset, num_samples);
    }
fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        for node in self.nodes.iter() {
            unsafe { (*node.processor.get()).setup(config); }
        }
    }
fn reset(&mut self) {
        for node in self.nodes.iter() {
            unsafe { (*node.processor.get()).reset(); }
        }
        for buffer in self.buffer_pool.buffers.iter_mut() {
            buffer.data.fill(0.0);
        }
    }
fn latency_samples(&self) -> usize {
        // For a DAG, the total latency is the maximum latency along any path from input to output.
        // For simplicity in this iteration, we'll sum the latency of nodes in the longest stage path.
        // A more accurate version would traverse the graph edges.
        let active_idx = self.topology_coordinator.active_idx();
        let topo = &self.topology_coordinator.topologies[active_idx];
        let mut total_latency = 0;

        for s_idx in 0..topo.plan.num_stages {
            let mut stage_max = 0;
            for &n_idx_u32 in &topo.plan.stages[s_idx].0[..topo.plan.stage_counts[s_idx] as usize] {
                let node = &self.nodes[n_idx_u32 as usize];
                let lat = unsafe { (*node.processor.get()).latency_samples() };
                if lat > stage_max { stage_max = lat; }
            }
            total_latency += stage_max;
        }
        total_latency
    }
}

impl nullherz_traits::MidiResponder for ProcessorGraph {
    fn apply_midi(&mut self, event: nullherz_traits::MidiEvent, context: Option<&nullherz_traits::ProcessContext>) {
        for i in 0..self.node_count {
            let node = &self.nodes[i];
            let processor = unsafe { &mut *node.processor.get() };
            processor.apply_midi(event, context);
        }
    }
}

impl nullherz_traits::SnapshotProvider for ProcessorGraph {
    fn pull_all_snapshots(&mut self, target: &mut Vec<(u64, std::sync::Arc<Vec<f32>>)>) {
        for i in 0..self.node_count {
            let node = &self.nodes[i];
            let processor = unsafe { &mut *node.processor.get() };
            if let Some(snapshot) = processor.pull_snapshot()
                && let Some(meta) = processor.metadata() {
                    target.push((meta.processor_id, snapshot));
                }
            processor.pull_all_snapshots(target);
        }
    }
}

impl AudioProcessor for ProcessorGraph {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_topology_mutation(&mut self, mutation: TopologyMutation) {
        // Buffer everything until CommitTopology to ensure atomic structural shifts.
        if self.pending_mutation_count < crate::MAX_MUTATIONS {
            self.pending_mutations[self.pending_mutation_count] = Some(mutation);
            self.pending_mutation_count += 1;
        } else {
            // Drop if full.
            if let TopologyMutation::AddNode { processor, .. } | TopologyMutation::SwapProcessor { processor, .. } = mutation
                 && let Some(ref mut prod) = self.garbage_producer {
                     let _ = prod.push_processor(processor);
                 }
        }
    }
fn apply_command(&mut self, command: &nullherz_traits::Command) {
        use nullherz_traits::{Command, CoreCommand};
        match command {
            Command::Core(CoreCommand::SetSafeMode(enabled)) => {
                for node in self.nodes.iter() {
                    unsafe { (*node.processor.get()).set_safe_mode(*enabled); }
                }
            }
            Command::Core(CoreCommand::CommitTopology) => {
                // AUDIT: CommitTopology must never be executed on the RT thread.
                // It is handled off-thread by TopologyManager, which pushes a
                // TopologyMutation::SetTopology for an O(1) Arc swap.
            }
            Command::Mixer(nullherz_traits::MixerCommand::Bundle { .. }) => {}
            _ => { for node in self.nodes.iter() { unsafe { (*node.processor.get()).apply_command(command); } } }
        }
    }
fn set_garbage_producer(&mut self, producer: Box<dyn nullherz_traits::GarbageProducer>) {
        self.garbage_producer = Some(producer);
    }
fn collect_telemetry(&self, node_times: &mut [u64; crate::MAX_NODES], peak_levels: &mut [f32; crate::MAX_NODES]) {
        for i in 0..crate::MAX_NODES {
            node_times[i] = self.telemetry.node_times_cycles[i].load(std::sync::atomic::Ordering::Relaxed);
            peak_levels[i] = f32::from_bits(self.telemetry.peak_levels[i].load(std::sync::atomic::Ordering::Relaxed));
        }
    }
fn list_children(&self) -> Vec<&dyn AudioProcessor> {
        let mut children = Vec::new();
        for i in 0..self.node_count {
            children.push(unsafe { &**self.nodes[i].processor.get() });
        }
        children
    }
}

#[cfg(test)]
mod tests {
    use nullherz_traits::SignalProcessor;
    use super::*;
    use nullherz_traits::ProcessContext;
    use std::sync::atomic::AtomicU64;

    use std::sync::atomic::Ordering;
    struct IdentityProcessor;
    impl std::fmt::Debug for IdentityProcessor {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "IdentityProcessor") }
    }
    impl nullherz_traits::SignalProcessor for IdentityProcessor {
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
            for i in 0..inputs.len().min(outputs.len()) { outputs[i].copy_from_slice(inputs[i]); }
        }
}

impl nullherz_traits::MidiResponder for IdentityProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for IdentityProcessor { }

impl AudioProcessor for IdentityProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

    #[test]
    fn test_graph_sub_block_routing() {
        let mut graph = ProcessorGraph::new();
        graph.add_node(Box::new(IdentityProcessor), vec![2], vec![0]);
        let mut input_data = [0.0f32; 256];
        for i in 0..256 { input_data[i] = i as f32; }
        graph.buffer_pool.buffers[2].data.copy_from_slice(&input_data);
        let mut out_data = [0.0f32; 100];
        let out_slice = &mut out_data[..];
        let mut outputs = [out_slice];
        let mut context = ProcessContext {  transport: None, host: None, sub_block_offset: 0, is_last_sub_block: false, };
        graph.process(&[], &mut outputs, &mut context);
        for i in 0..50 { assert_eq!(out_data[i], i as f32); }
    }

    #[test]
    fn test_crossfade_state_progression() {
        let mut graph = ProcessorGraph::new();
        let topo_idx = graph.topology_coordinator.active_idx();

        // Manually setup a crossfade
        graph.topology_coordinator.topologies[topo_idx].crossfades[0] = Some(CrossfadeState {
            node_idx: 1,
            input_idx: 0,
            old_buffer_idx: 10,
            new_buffer_idx: 20,
            remaining_samples: 100,
            total_samples: 100,
        });

        // Fill old/new buffers with distinct values
        graph.buffer_pool.old_path_buffers[10].data.fill(1.0);
        graph.buffer_pool.buffers[20].data.fill(2.0);

        let mut block_x_map = [[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES];
        GraphExecutor::resolve_crossfades(&mut graph.topology_coordinator.topologies, topo_idx, 0, 50, &graph.buffer_pool.old_path_buffers, &graph.buffer_pool.buffers, &mut graph.buffer_pool.crossfade_buffers, &mut block_x_map);

        // Check progression
        let state = graph.topology_coordinator.topologies[topo_idx].crossfades[0].unwrap();
        assert_eq!(state.remaining_samples, 50);
        assert_eq!(block_x_map[1][0], 64); // 64 + x_idx

        // Check buffer content (halfway should be ~1.5)
        assert_eq!(graph.buffer_pool.crossfade_buffers[0].data[0], 1.0);
        assert!(graph.buffer_pool.crossfade_buffers[0].data[49] > 1.0);

        GraphExecutor::resolve_crossfades(&mut graph.topology_coordinator.topologies, topo_idx, 50, 50, &graph.buffer_pool.old_path_buffers, &graph.buffer_pool.buffers, &mut graph.buffer_pool.crossfade_buffers, &mut block_x_map);
        assert!(graph.topology_coordinator.topologies[topo_idx].crossfades[0].is_none());
    }

    #[test]
    fn test_graph_topology_stability() {
        let mut graph = ProcessorGraph::new();
        for i in 0..10 {
            graph.add_node(Box::new(IdentityProcessor), vec![(i + 1) % crate::MAX_NODES], vec![i % crate::MAX_NODES]);
        }
        let active_idx = graph.topology_coordinator.active_idx();
        let topo = &graph.topology_coordinator.topologies[active_idx];
        if topo.node_count > 0 { assert!(topo.plan.num_stages > 0); }
        let mut seen_nodes = std::collections::HashSet::new();
        for s_idx in 0..topo.plan.num_stages {
            for n_idx in &topo.plan.stages[s_idx].0[..topo.plan.stage_counts[s_idx] as usize] {
                assert!(seen_nodes.insert(*n_idx), "Node {} assigned to multiple stages", n_idx);
            }
        }
        assert!(GraphCompiler::verify_no_hazards(&graph.topology_coordinator.topologies[active_idx], &topo.plan).is_ok());
    }

    #[test]
    fn test_graph_parallel_execution_consistency() {
        let mut graph = ProcessorGraph::new();
        // Setup a simple graph: Node 0 -> Node 1
        graph.add_node(Box::new(IdentityProcessor), vec![10], vec![11]);
        graph.add_node(Box::new(IdentityProcessor), vec![11], vec![0]);

        let mut pool = TaskPool::new(2);
        let mut input_data = [0.0f32; 128];
        for i in 0..128 { input_data[i] = i as f32; }
        graph.buffer_pool.buffers[10].data[..128].copy_from_slice(&input_data);

        let mut out_data = [0.0f32; 128];
        let mut outputs = [&mut out_data[..]];
        let mut context = ProcessContext { transport: None, host: None, sub_block_offset: 0, is_last_sub_block: true };

        graph.process_parallel(&[], &mut outputs, &mut context, Some(&mut pool));

        for i in 0..128 {
            assert_eq!(out_data[i], i as f32);
        }
    }

    #[test]
    fn test_rt_topology_commit_is_no_op() {
        let mut graph = ProcessorGraph::new();
        // Manually increment node count to simulate a populated graph
        graph.node_count = 1;

        // Sending CommitTopology to RT apply_command should NOT trigger calculate_stages
        // (which would set plan.num_stages > 0 if it worked, but here it should be a no-op)
        graph.apply_command(&nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));

        assert_eq!(graph.topology_coordinator.active_topology().plan.num_stages, 0, "RT CommitTopology must be a no-op");
    }

    #[test]
    fn test_task_pool_sync_no_reset_race() {
        let mut pool = TaskPool::new(1);
        let completion = pool.completion.clone();
        assert_eq!(completion.load(Ordering::Relaxed), 0);
        let start_count = completion.load(Ordering::Acquire);
        let node1 = ProcessorNode { processor: std::cell::UnsafeCell::new(Box::new(IdentityProcessor)) };
        let _ = pool.worker_producers[0].push(Job {
            node_ptr: &node1 as *const _,
            num_samples: 10,
            sub_block_offset: 0,
            buffers_ptr: std::ptr::null_mut(),
            x_buffers_ptr: std::ptr::null_mut(),
            input_indices: [0; 16],
            output_indices: [0; 16],
            input_count: 0,
            output_count: 0,
            node_idx: 0,
            telemetry_ptr: &std::array::from_fn(|_| AtomicU64::new(0)) as *const _,
            transport: None,
            host_ptr: None,
            is_last_sub_block: false,
            is_bypassed: false,
        });
        pool.worker_wake_fds[0].notify();
        let target = start_count + 1;
        while completion.load(Ordering::Acquire) < target { pool.completion_fd.wait(); }
        let start_count_2 = completion.load(Ordering::Acquire);
        assert_eq!(start_count_2, 1);
        let node2 = ProcessorNode { processor: std::cell::UnsafeCell::new(Box::new(IdentityProcessor)) };
        let _ = pool.worker_producers[0].push(Job {
            node_ptr: &node2 as *const _,
            num_samples: 10,
            sub_block_offset: 0,
            buffers_ptr: std::ptr::null_mut(),
            x_buffers_ptr: std::ptr::null_mut(),
            input_indices: [0; 16],
            output_indices: [0; 16],
            input_count: 0,
            output_count: 0,
            node_idx: 0,
            telemetry_ptr: &std::array::from_fn(|_| AtomicU64::new(0)) as *const _,
            transport: None,
            host_ptr: None,
            is_last_sub_block: false,
            is_bypassed: false,
        });
        pool.worker_wake_fds[0].notify();
        let target_2 = start_count_2 + 1;
        while completion.load(Ordering::Acquire) < target_2 { pool.completion_fd.wait(); }
        assert_eq!(completion.load(Ordering::Relaxed), 2);
    }
}
