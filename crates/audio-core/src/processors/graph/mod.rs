mod node;
mod pool;
mod telemetry;
mod topology_types;
mod compiler;
mod executor;

pub use node::{ProcessorNode, DummyProcessor};
pub use pool::{TaskPool, Job};
pub use telemetry::GraphTelemetry;
pub use topology_types::{GraphTopology, NodeRouting, CrossfadeState};
pub use compiler::GraphCompiler;
pub use executor::GraphExecutor;

use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicUsize};
use ipc_layer::{AudioBlock, Producer};
use crate::processors::{AudioProcessor, TopologyMutation};

pub struct ProcessorGraph {
    pub(crate) nodes: Box<[ProcessorNode; crate::MAX_NODES]>,
    pub(crate) node_count: usize,
    pub(crate) buffers: Box<[AudioBlock; crate::MAX_NODES]>,
    pub(crate) _crossfade_buffers: [AudioBlock; 8],
    pub(crate) _old_path_buffers: Box<[AudioBlock; crate::MAX_NODES]>,
    pub(crate) topologies: Box<[GraphTopology; 2]>,
    pub(crate) active_topo_idx: Arc<AtomicUsize>,
    pub(crate) needs_commit: bool,

    pub(crate) telemetry: Arc<GraphTelemetry>,
    pub(crate) garbage_producer: Option<Producer<Box<dyn AudioProcessor>>>,
}

impl ProcessorGraph {
    pub fn new() -> Self {
        let buffers = Box::new([AudioBlock { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; crate::MAX_NODES]);
        let mut v2p = [0usize; crate::MAX_NODES];
        for (i, val) in v2p.iter_mut().enumerate() { *val = i; }
        let topo = GraphTopology {
            routing: [NodeRouting { input_indices: [0; crate::MAX_CHANNELS], output_indices: [0; crate::MAX_CHANNELS], input_count: 0, output_count: 0 }; crate::MAX_NODES],
            virtual_to_physical: v2p,
            stages: [[0; crate::MAX_NODES]; crate::MAX_NODES],
            stage_counts: [0; crate::MAX_NODES],
            num_stages: 0,
            crossfades: [None; 8],
            node_count: 0,
        };

        let nodes = Box::new(std::array::from_fn(|_| ProcessorNode {
            processor: std::cell::UnsafeCell::new(Box::new(DummyProcessor) as Box<dyn AudioProcessor>),
        }));

        Self {
            nodes,
            node_count: 0,
            buffers,
            _crossfade_buffers: [AudioBlock { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; 8],
            _old_path_buffers: Box::new([AudioBlock { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; crate::MAX_NODES]),
            topologies: Box::new([topo; 2]),
            active_topo_idx: Arc::new(AtomicUsize::new(0)),
            needs_commit: false,
            telemetry: Arc::new(GraphTelemetry::default()),
            garbage_producer: None,
        }
    }

    fn inactive_topology_mut(&mut self) -> &mut GraphTopology {
        let active = self.active_topo_idx.load(Ordering::Acquire);
        let inactive = (active + 1) % 2;
        if !self.needs_commit {
            self.topologies[inactive] = self.topologies[active];
            self.needs_commit = true;
        }
        &mut self.topologies[inactive]
    }

    pub fn calculate_stages(&mut self) {
        let active_idx = self.active_topo_idx.load(Ordering::Acquire);
        let inactive_idx = (active_idx + 1) % 2;
        GraphCompiler::calculate_stages(&mut self.topologies[inactive_idx]);
    }

    pub fn commit_graph(&mut self) {
        let active = self.active_topo_idx.load(Ordering::Acquire);
        let inactive = (active + 1) % 2;

        if self.topologies[inactive].num_stages == 0 && self.topologies[inactive].node_count > 0 {
            return;
        }

        if let Err(msg) = GraphCompiler::verify_no_hazards(&self.topologies[inactive]) {
            eprintln!("CRITICAL: Refusing to commit hazardous topology: {}", msg);
            return;
        }

        self.active_topo_idx.store(inactive, Ordering::Release);
        self.needs_commit = false;
    }


    pub fn add_node(&mut self, processor: Box<dyn AudioProcessor>, inputs: Vec<usize>, outputs: Vec<usize>) {
        if self.node_count >= crate::MAX_NODES { return; }
        let idx = self.node_count;
        unsafe { *self.nodes[idx].processor.get() = processor; }
        self.node_count += 1;

        let topo = self.inactive_topology_mut();
        let input_count = inputs.len().min(crate::MAX_CHANNELS);
        topo.routing[idx].input_count = input_count;
        topo.routing[idx].input_indices[..input_count].copy_from_slice(&inputs[..input_count]);

        let output_count = outputs.len().min(crate::MAX_CHANNELS);
        topo.routing[idx].output_count = output_count;
        topo.routing[idx].output_indices[..output_count].copy_from_slice(&outputs[..output_count]);
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

impl AudioProcessor for ProcessorGraph {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, external_inputs: &[&[f32]], external_outputs: &mut [&mut [f32]], context: &mut nullherz_traits::ProcessContext) {
        self.process_parallel(external_inputs, external_outputs, context, None);
    }

    fn process_parallel(&mut self, _external_inputs: &[&[f32]], external_outputs: &mut [&mut [f32]], context: &mut nullherz_traits::ProcessContext, executor: Option<&mut (dyn nullherz_traits::ParallelExecutor + '_)>) {
        let is_last_sub_block = context.is_last_sub_block;
        let num_samples = if !external_outputs.is_empty() { external_outputs[0].len() } else { 0 };
        if num_samples == 0 { return; }

        let active_idx = self.active_topo_idx.load(Ordering::Acquire);
        let offset = context.sub_block_offset;

        let mut block_x_map = [[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES];
        GraphExecutor::resolve_crossfades(
            &mut self.topologies,
            active_idx,
            offset,
            num_samples,
            &self._old_path_buffers,
            &self.buffers,
            &mut self._crossfade_buffers,
            &mut block_x_map
        );

        let mut pool = executor.and_then(|e| e.as_any().downcast_mut::<TaskPool>());

        let num_stages = self.topologies[active_idx].num_stages;
        let transport = context.transport;
        for s_idx in 0..num_stages {
            GraphExecutor::execute_stage(
                &self.nodes,
                &mut self.buffers,
                &mut self._crossfade_buffers,
                &self.topologies[active_idx],
                s_idx,
                num_samples,
                offset,
                &block_x_map,
                &mut pool,
                transport,
                is_last_sub_block,
                &self.telemetry.node_times_cycles
            );
        }

        let topo = &self.topologies[active_idx];
        if !external_outputs.is_empty() {
            let p0 = topo.virtual_to_physical[0];
            external_outputs[0].copy_from_slice(&self.buffers[p0].data[offset..offset + num_samples]);
        }
        if external_outputs.len() >= 2 {
            let p1 = topo.virtual_to_physical[1];
            external_outputs[1].copy_from_slice(&self.buffers[p1].data[offset..offset + num_samples]);
        }

        if is_last_sub_block {
            let active_idx = self.active_topo_idx.load(Ordering::Acquire);
            let has_active_crossfades = self.topologies[active_idx].crossfades.iter().any(|x| x.is_some());
            if has_active_crossfades {
                for i in 0..crate::MAX_NODES {
                    self._old_path_buffers[i].data.copy_from_slice(&self.buffers[i].data);
                }
            }
        }

        self.telemetry.update_peak_levels(topo, &self.buffers, offset, num_samples);
    }

    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        for node in self.nodes.iter() {
            unsafe { (*node.processor.get()).setup(config); }
        }
    }

    fn apply_topology_mutation(&mut self, mutation: TopologyMutation) {
        match mutation {
            TopologyMutation::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let n_idx = node_idx as usize;
                let i_idx = input_idx as usize;
                if n_idx < self.node_count && i_idx < 16 {
                    let topo = self.inactive_topology_mut();
                    if i_idx < topo.routing[n_idx].input_count {
                        topo.routing[n_idx].input_indices[i_idx] = (new_buffer_idx as usize).min(63);
                    }
                }
            }
            TopologyMutation::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let n_idx = node_idx as usize;
                let o_idx = output_idx as usize;
                if n_idx < self.node_count && o_idx < 16 {
                    let topo = self.inactive_topology_mut();
                    if o_idx < topo.routing[n_idx].output_count {
                        topo.routing[n_idx].output_indices[o_idx] = (new_buffer_idx as usize).min(63);
                    }
                }
            }
            TopologyMutation::SwapProcessor { node_idx, mut processor } => {
                let n_idx = node_idx as usize;
                if n_idx < self.node_count {
                    let node = &self.nodes[n_idx];
                    if let Some(ref prod) = self.garbage_producer { processor.set_garbage_producer(prod.clone()); }
                    let old_proc = unsafe { std::ptr::replace(node.processor.get(), processor) };
                    if let Some(ref mut prod) = self.garbage_producer {
                        if let Err(leaked) = prod.push(old_proc) { std::mem::forget(leaked); }
                    } else { std::mem::forget(old_proc); }
                }
            }
            TopologyMutation::AddNode { node_idx: _, mut processor } => {
                if self.node_count < 64 {
                    let idx = self.node_count;
                    if let Some(ref prod) = self.garbage_producer { processor.set_garbage_producer(prod.clone()); }
                    unsafe { *self.nodes[idx].processor.get() = processor; }
                    self.node_count += 1;
                    let topo = self.inactive_topology_mut();
                    topo.routing[idx].input_count = 0;
                    topo.routing[idx].output_count = 0;
                    topo.node_count += 1;
                }
            }
        }
    }

    fn apply_command(&mut self, command: &control_plane::Command) {
        match command {
            control_plane::Command::CommitTopology => { self.calculate_stages(); self.commit_graph(); }
            _ => { for node in self.nodes.iter() { unsafe { (*node.processor.get()).apply_command(command); } } }
        }
    }

    fn apply_midi(&mut self, event: ipc_layer::MidiEvent) {
        for node in self.nodes.iter() { unsafe { (*node.processor.get()).apply_midi(event); } }
    }

    fn set_garbage_producer(&mut self, producer: Producer<Box<dyn AudioProcessor>>) {
        self.garbage_producer = Some(producer);
    }
    fn collect_telemetry(&self, node_times: &mut [u64; crate::MAX_NODES], peak_levels: &mut [f32; crate::MAX_NODES]) {
        for i in 0..crate::MAX_NODES {
            node_times[i] = self.telemetry.node_times_cycles[i].load(Ordering::Relaxed);
            peak_levels[i] = f32::from_bits(self.telemetry.peak_levels[i].load(Ordering::Relaxed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::ProcessContext;
    use std::sync::atomic::AtomicU64;

    struct IdentityProcessor;
    impl std::fmt::Debug for IdentityProcessor {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "IdentityProcessor") }
    }
    impl AudioProcessor for IdentityProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

        fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
            for i in 0..inputs.len().min(outputs.len()) { outputs[i].copy_from_slice(inputs[i]); }
        }
    }

    #[test]
    fn test_graph_sub_block_routing() {
        let mut graph = ProcessorGraph::new();
        graph.add_node(Box::new(IdentityProcessor), vec![2], vec![0]);
        let mut input_data = [0.0f32; 256];
        for i in 0..256 { input_data[i] = i as f32; }
        graph.buffers[2].data.copy_from_slice(&input_data);
        let mut out_data = [0.0f32; 100];
        let out_slice = &mut out_data[..];
        let mut outputs = [out_slice];
        let mut context = ProcessContext {  transport: None, sub_block_offset: 10, is_last_sub_block: false, };
        graph.process(&[], &mut outputs, &mut context);
        for i in 0..50 { assert_eq!(out_data[i], (i + 10) as f32); }
    }

    #[test]
    fn test_crossfade_state_progression() {
        let mut graph = ProcessorGraph::new();
        let topo_idx = graph.active_topo_idx.load(Ordering::Relaxed);

        // Manually setup a crossfade
        graph.topologies[topo_idx].crossfades[0] = Some(CrossfadeState {
            node_idx: 1,
            input_idx: 0,
            old_buffer_idx: 10,
            new_buffer_idx: 20,
            remaining_samples: 100,
            total_samples: 100,
        });

        // Fill old/new buffers with distinct values
        graph._old_path_buffers[10].data.fill(1.0);
        graph.buffers[20].data.fill(2.0);

        let mut block_x_map = [[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES];
        GraphExecutor::resolve_crossfades(&mut graph.topologies, topo_idx, 0, 50, &graph._old_path_buffers, &graph.buffers, &mut graph._crossfade_buffers, &mut block_x_map);

        // Check progression
        let state = graph.topologies[topo_idx].crossfades[0].unwrap();
        assert_eq!(state.remaining_samples, 50);
        assert_eq!(block_x_map[1][0], 64); // 64 + x_idx

        // Check buffer content (halfway should be ~1.5)
        assert_eq!(graph._crossfade_buffers[0].data[0], 1.0);
        assert!(graph._crossfade_buffers[0].data[49] > 1.0);

        GraphExecutor::resolve_crossfades(&mut graph.topologies, topo_idx, 50, 50, &graph._old_path_buffers, &graph.buffers, &mut graph._crossfade_buffers, &mut block_x_map);
        assert!(graph.topologies[topo_idx].crossfades[0].is_none());
    }

    #[test]
    fn test_graph_topology_stability() {
        let mut graph = ProcessorGraph::new();
        for i in 0..10 {
            graph.add_node(Box::new(IdentityProcessor), vec![(i + 1) % 64], vec![i % 64]);
        }
        let active_idx = graph.active_topo_idx.load(Ordering::Acquire);
        let topo = &graph.topologies[active_idx];
        if topo.node_count > 0 { assert!(topo.num_stages > 0); }
        let mut seen_nodes = std::collections::HashSet::new();
        for s_idx in 0..topo.num_stages {
            for n_idx in &topo.stages[s_idx][..topo.stage_counts[s_idx]] {
                assert!(seen_nodes.insert(*n_idx), "Node {} assigned to multiple stages", n_idx);
            }
        }
        assert!(GraphCompiler::verify_no_hazards(&graph.topologies[active_idx]).is_ok());
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
        graph.buffers[10].data[..128].copy_from_slice(&input_data);

        let mut out_data = [0.0f32; 128];
        let mut outputs = [&mut out_data[..]];
        let mut context = ProcessContext { transport: None, sub_block_offset: 0, is_last_sub_block: true };

        graph.process_parallel(&[], &mut outputs, &mut context, Some(&mut pool));

        for i in 0..128 {
            assert_eq!(out_data[i], i as f32);
        }
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
            is_last_sub_block: false,
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
            is_last_sub_block: false,
        });
        pool.worker_wake_fds[0].notify();
        let target_2 = start_count_2 + 1;
        while completion.load(Ordering::Acquire) < target_2 { pool.completion_fd.wait(); }
        assert_eq!(completion.load(Ordering::Relaxed), 2);
    }
}
