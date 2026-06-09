use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool, AtomicU32, AtomicU64};
use std::thread;
use ipc_layer::{AudioBlock, RingBuffer, Producer};
use crate::processors::AudioProcessor;

#[derive(Clone, Copy)]
pub struct CrossfadeState {
    pub node_idx: usize,
    pub input_idx: usize,
    pub old_buffer_idx: usize,
    pub new_buffer_idx: usize,
    pub remaining_samples: u32,
    pub total_samples: u32,
}

pub struct ProcessorNode {
    pub processor: std::cell::UnsafeCell<Box<dyn AudioProcessor>>,
}

unsafe impl Send for ProcessorNode {}
unsafe impl Sync for ProcessorNode {}

struct DummyProcessor;
impl AudioProcessor for DummyProcessor {
    fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]], _context: &mut crate::processors::ProcessContext) {}
}

#[derive(Clone, Copy)]
pub struct NodeRouting {
    pub input_indices: [usize; crate::MAX_CHANNELS],
    pub output_indices: [usize; crate::MAX_CHANNELS],
    pub input_count: usize,
    pub output_count: usize,
}

#[derive(Clone, Copy)]
pub struct GraphTopology {
    pub routing: [NodeRouting; crate::MAX_NODES],
    pub virtual_to_physical: [usize; crate::MAX_NODES],
    pub stages: [[usize; crate::MAX_NODES]; crate::MAX_NODES],
    pub stage_counts: [usize; crate::MAX_NODES],
    pub num_stages: usize,
    pub crossfades: [Option<CrossfadeState>; 8],
    pub node_count: usize,
}

#[derive(Clone, Copy)]
pub struct Job {
    pub node_ptr: *const ProcessorNode,
    pub num_samples: usize,
    pub sub_block_offset: usize,
    pub buffers_ptr: *mut AudioBlock,
    pub x_buffers_ptr: *mut AudioBlock,
    pub input_indices: [usize; crate::MAX_CHANNELS],
    pub output_indices: [usize; crate::MAX_CHANNELS],
    pub input_count: usize,
    pub output_count: usize,
    pub node_idx: usize, // for telemetry
    pub telemetry_ptr: *const [AtomicU64; crate::MAX_NODES],
    pub transport: Option<crate::Transport>,
    pub is_last_sub_block: bool,
}

unsafe impl Send for Job {}

pub struct TaskPool {
    workers: Vec<thread::JoinHandle<()>>,
    pub(crate) worker_producers: Vec<Producer<Job>>,
    pub(crate) completion: Arc<AtomicUsize>,
    pub(crate) running: Arc<AtomicBool>,
}

impl TaskPool {
    pub fn new(num_workers: usize) -> Self {
        let mut workers = Vec::new();
        let mut worker_producers = Vec::new();
        let completion = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicBool::new(true));

        for i in 0..num_workers {
            let (prod, mut cons) = RingBuffer::<Job>::new(128).split();
            let running_worker = running.clone();
            let completion_worker = completion.clone();

            let handle = thread::spawn(move || {
                crate::setup_rt_thread(85, Some(i + 1)); // Pin workers to cores 1..N
                let mut spins = 0;
                while running_worker.load(Ordering::Relaxed) {
                    if let Some(job) = cons.pop() {
                        spins = 0;
                        let node = unsafe { &*job.node_ptr };
                        let num_samples = job.num_samples;
                        let buffers_ptr = job.buffers_ptr;

                        let mut node_inputs_storage = [ &[][..]; 16 ];
                        let input_count = job.input_count.min(16);
                        let offset = job.sub_block_offset;

                        for (i, input_storage) in node_inputs_storage.iter_mut().enumerate().take(input_count) {
                            let p_idx = *job.input_indices.get(i).unwrap_or(&0);
                            if p_idx >= 64 {
                                let x_idx = p_idx - 64;
                                if x_idx < 8 {
                                    // SAFETY: x_buffers_ptr is valid for 8 AudioBlocks.
                                    unsafe { *input_storage = &(&(*job.x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                                }
                            } else if p_idx < 64 {
                                // SAFETY: buffers_ptr is valid for MAX_NODES AudioBlocks.
                                unsafe { *input_storage = &(&(*buffers_ptr.add(p_idx)).data)[offset..offset + num_samples]; }
                            }
                        }

                        let mut node_outputs_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|_| &mut [][..]);
                        let output_count = job.output_count.min(16);
                        for (i, output_storage) in node_outputs_reconstructed.iter_mut().enumerate().take(output_count) {
                            let p_idx = *job.output_indices.get(i).unwrap_or(&0);
                            if p_idx < 64 {
                                // SAFETY: buffers_ptr is valid and unique for each index in the current stage.
                                unsafe {
                                    *output_storage = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr().add(offset), num_samples);
                                }
                            }
                        }

                        #[cfg(target_arch = "x86_64")]
                        let start = unsafe { std::arch::x86_64::_rdtsc() };

                        let mut inner_context = crate::processors::ProcessContext {
                            pool: None,
                            transport: job.transport.as_ref(),
                            sub_block_offset: offset,
                            is_last_sub_block: job.is_last_sub_block
                        };
                        unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count], &mut inner_context); }

                        #[cfg(target_arch = "x86_64")]
                        {
                            let elapsed = unsafe { std::arch::x86_64::_rdtsc() } - start;
                            unsafe { (*job.telemetry_ptr)[job.node_idx].store(elapsed, Ordering::Relaxed); }
                        }

                        completion_worker.fetch_add(1, Ordering::Release);
                    } else {
                        if spins < 10000 {
                            std::hint::spin_loop();
                        } else {
                            std::thread::yield_now();
                        }
                        spins += 1;
                    }
                }
            });

            workers.push(handle);
            worker_producers.push(prod);
        }

        Self { workers, worker_producers, completion, running }
    }
}

impl Drop for TaskPool {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Default for ProcessorGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use super::*;
    use crate::processors::ProcessContext;

    struct IdentityProcessor;
    impl AudioProcessor for IdentityProcessor {
        fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
            for i in 0..inputs.len().min(outputs.len()) {
                outputs[i].copy_from_slice(inputs[i]);
            }
        }
    }

    #[test]
    fn test_task_pool_sync_no_reset_race() {
        let mut pool = TaskPool::new(1);
        let completion = pool.completion.clone();

        // Initial state
        assert_eq!(completion.load(Ordering::Relaxed), 0);

        // Stage 1
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

        let target = start_count + 1;
        while completion.load(Ordering::Acquire) < target { std::hint::spin_loop(); }

        // Stage 2 - Must not reset to 0
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

        let target_2 = start_count_2 + 1;
        while completion.load(Ordering::Acquire) < target_2 { std::hint::spin_loop(); }
        assert_eq!(completion.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_graph_sub_block_routing() {
        let mut graph = ProcessorGraph::new();
        // Node 0: Identity, Input Buffer 2 -> Output Buffer 0
        graph.add_node(Box::new(IdentityProcessor), vec![2], vec![0]);

        let mut input_data = [0.0f32; 256];
        for i in 0..256 { input_data[i] = i as f32; }

        // Mock physical buffer 2
        graph.buffers[2].data.copy_from_slice(&input_data);

        let mut out_data = [0.0f32; 100];
        let out_slice = &mut out_data[..];
        let mut outputs = [out_slice];

        let mut context = ProcessContext {
            pool: None,
            transport: None,
            sub_block_offset: 10,
            is_last_sub_block: false,
        };

        // Process a sub-block of 50 samples at offset 10
        graph.process(&[], &mut outputs, &mut context);

        // Check if output buffer 0 (which is mapped to external_outputs[0])
        // contains the correct data at the correct offset
        for i in 0..50 {
            assert_eq!(out_data[i], (i + 10) as f32);
        }
    }

    proptest! {
        #[test]
        fn test_graph_topology_stability(
            node_counts in 1..crate::MAX_NODES,
            edge_counts in 1..100
        ) {
            let mut graph = ProcessorGraph::new();
            for i in 0..node_counts {
                // Add nodes with randomized but valid physical buffer indices
                graph.add_node(Box::new(IdentityProcessor), vec![(i + 1) % 64], vec![i % 64]);
            }

            // The scheduler should have produced a valid execution plan
            let active_idx = graph.active_topo_idx.load(Ordering::Acquire);
            let topo = &graph.topologies[active_idx];

            if topo.node_count > 0 {
                assert!(topo.num_stages > 0);
            }

            // Verify that all assigned nodes are unique across stages
            let mut seen_nodes = std::collections::HashSet::new();
            for s_idx in 0..topo.num_stages {
                for n_idx in &topo.stages[s_idx][..topo.stage_counts[s_idx]] {
                    assert!(seen_nodes.insert(*n_idx), "Node {} assigned to multiple stages", n_idx);
                }
            }

            // Verify that hazard check passes for the committed graph
            assert!(graph.verify_no_hazards_prod(active_idx).is_ok());
        }
    }
}

/// Encapsulates all real-time telemetry gathered during graph execution.
pub struct GraphTelemetry {
    /// Atomic cycle counts per node for performance profiling.
    pub node_times_cycles: [AtomicU64; crate::MAX_NODES],
    /// Atomic peak signal levels (f32 bits) per node for metering.
    pub peak_levels: [AtomicU32; crate::MAX_NODES],
}

impl Default for GraphTelemetry {
    fn default() -> Self {
        Self {
            node_times_cycles: std::array::from_fn(|_| AtomicU64::new(0)),
            peak_levels: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}

pub struct ProcessorGraph {
    pub(crate) nodes: Box<[ProcessorNode; crate::MAX_NODES]>,
    pub(crate) node_count: usize,
    pub(crate) buffers: Box<[AudioBlock; crate::MAX_NODES]>,
    pub(crate) _crossfade_buffers: [AudioBlock; 8],
    pub(crate) _old_path_buffers: Box<[AudioBlock; crate::MAX_NODES]>,
    pub(crate) topologies: Box<[GraphTopology; 2]>,
    pub(crate) active_topo_idx: Arc<AtomicUsize>,
    pub(crate) needs_commit: bool,

    pub(crate) _stage_scratch_assigned: [bool; crate::MAX_NODES],
    pub(crate) _stage_scratch_in_degree: [usize; crate::MAX_NODES],

    pub(crate) telemetry: Arc<GraphTelemetry>,
    pub(crate) _telemetry_offset: AtomicUsize,
    pub(crate) garbage_producer: Option<ipc_layer::Producer<Box<dyn AudioProcessor>>>,
}

use crate::processors::TopologyMutation;

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
            _stage_scratch_assigned: [false; 64],
            _stage_scratch_in_degree: [0; 64],
            telemetry: Arc::new(GraphTelemetry::default()),
            _telemetry_offset: AtomicUsize::new(0),
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
        let topo = &mut self.topologies[inactive_idx];
        let n = topo.node_count;
        if n == 0 { return; }

        let mut in_degree = [0usize; crate::MAX_NODES];
        let mut adj = [[0usize; crate::MAX_NODES]; crate::MAX_NODES];
        let mut adj_count = [0usize; crate::MAX_NODES];

        // 1. Build adjacency list and in-degrees efficiently
        let mut v_to_producers = [[0usize; crate::MAX_NODES]; crate::MAX_NODES];
        let mut v_producer_counts = [0usize; crate::MAX_NODES];
        for j in 0..n {
            let routing_j = &topo.routing[j];
            for k in 0..routing_j.output_count {
                let v_out = routing_j.output_indices[k];
                if v_out < crate::MAX_NODES {
                    v_to_producers[v_out][v_producer_counts[v_out]] = j;
                    v_producer_counts[v_out] += 1;
                }
            }
        }

        for (i, in_degree_val) in in_degree.iter_mut().enumerate().take(n) {
            let routing_i = &topo.routing[i];
            for l in 0..routing_i.input_count {
                let v_in = routing_i.input_indices[l];
                if v_in < crate::MAX_NODES {
                    for &j in v_to_producers[v_in].iter().take(v_producer_counts[v_in]) {
                        if i == j { continue; }
                        let mut exists = false;
                        for &adj_val in adj[j].iter().take(adj_count[j]) {
                            if adj_val == i {
                                exists = true;
                                break;
                            }
                        }
                        if !exists {
                            adj[j][adj_count[j]] = i;
                            adj_count[j] += 1;
                            *in_degree_val += 1;
                        }
                    }
                }
            }
        }

        // 2. Kahn's algorithm with Write-After-Write (WAW) tracking
        let mut processed_count = 0;
        let mut is_processed = [false; crate::MAX_NODES];
        topo.num_stages = 0;

        while processed_count < n {
            let mut stage_nodes = [0usize; crate::MAX_NODES];
            let mut stage_count = 0;
            let mut physical_buffers_in_stage = [false; crate::MAX_NODES];

            for i in 0..n {
                if !is_processed[i] && in_degree[i] == 0 {
                    // Check for WAW collision on physical buffers
                    let mut collision = false;
                    let routing = &topo.routing[i];
                    for k in 0..routing.output_count {
                        let v_out = routing.output_indices[k].min(63);
                        let p_out = topo.virtual_to_physical[v_out].min(63);
                        if physical_buffers_in_stage[p_out] {
                            collision = true;
                            break;
                        }
                    }

                    if !collision {
                        stage_nodes[stage_count] = i;
                        stage_count += 1;
                        for k in 0..routing.output_count {
                            let v_out = routing.output_indices[k].min(63);
                            let p_out = topo.virtual_to_physical[v_out].min(63);
                            physical_buffers_in_stage[p_out] = true;
                        }
                    }
                }
            }

            if stage_count == 0 { break; } // Cycle detected or no more progress

            for (i, &node_idx) in stage_nodes.iter().enumerate().take(stage_count) {
                topo.stages[topo.num_stages][i] = node_idx;
                is_processed[node_idx] = true;
                processed_count += 1;
            }
            topo.stage_counts[topo.num_stages] = stage_count;
            topo.num_stages += 1;

            for &node_idx in stage_nodes.iter().take(stage_count) {
                for &dependent in adj[node_idx].iter().take(adj_count[node_idx]) {
                    in_degree[dependent] -= 1;
                }
            }
        }
    }

    pub fn commit_graph(&mut self) {
        let active = self.active_topo_idx.load(Ordering::Acquire);
        let inactive = (active + 1) % 2;

        // Safety: Ensure topological stages were actually calculated for the new topology
        if self.topologies[inactive].num_stages == 0 && self.topologies[inactive].node_count > 0 {
            return;
        }

        // Production Hardening: Verify topology for hazards before commitment
        if let Err(msg) = self.verify_no_hazards_prod(inactive) {
            eprintln!("CRITICAL: Refusing to commit hazardous topology: {}", msg);
            return;
        }

        self.active_topo_idx.store(inactive, Ordering::Release);
        self.needs_commit = false;
    }

    fn verify_no_hazards_prod(&self, topo_idx: usize) -> Result<(), crate::error::AudioError> {
        let topo = &self.topologies[topo_idx];
        for s_idx in 0..topo.num_stages {
            let stage = &topo.stages[s_idx][..topo.stage_counts[s_idx]];
            let mut physical_writes = [false; crate::MAX_NODES];
            let mut physical_reads = [false; crate::MAX_NODES];

            for &n_idx in stage {
                let routing = &topo.routing[n_idx];

                // 1. Check for RAW (Read-After-Write) and WAR (Write-After-Read) Hazards
                // A node in the same stage cannot write to a buffer being read by another node in that stage.
                for k in 0..routing.output_count {
                    let v_out = *routing.output_indices.get(k).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
                    let p_out = *topo.virtual_to_physical.get(v_out).unwrap_or(&0).min(&(crate::MAX_NODES - 1));

                    if physical_writes[p_out] {
                        return Err(crate::error::AudioError::IpcError(format!("WAW Hazard at stage {}. Node {} attempts to write to physical buffer {} which is already used for writing in this stage.", s_idx, n_idx, p_out)));
                    }
                    if physical_reads[p_out] {
                        return Err(crate::error::AudioError::IpcError(format!("WAR Hazard at stage {}. Node {} attempts to write to physical buffer {} which is already being read in this stage.", s_idx, n_idx, p_out)));
                    }
                    physical_writes[p_out] = true;
                }

                for k in 0..routing.input_count {
                    let v_in = *routing.input_indices.get(k).unwrap_or(&0).min(&(crate::MAX_NODES - 1));
                    let p_in = *topo.virtual_to_physical.get(v_in).unwrap_or(&0).min(&(crate::MAX_NODES - 1));

                    if physical_writes[p_in] {
                        return Err(crate::error::AudioError::IpcError(format!("RAW Hazard at stage {}. Node {} attempts to read from physical buffer {} which is being written to in this stage.", s_idx, n_idx, p_in)));
                    }
                    physical_reads[p_in] = true;
                }
            }
        }
        Ok(())
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

impl AudioProcessor for ProcessorGraph {
    fn process(&mut self, _external_inputs: &[&[f32]], external_outputs: &mut [&mut [f32]], context: &mut crate::processors::ProcessContext) {
        let is_last_sub_block = context.is_last_sub_block;
        let num_samples = if !external_outputs.is_empty() { external_outputs[0].len() } else { 0 };
        if num_samples == 0 { return; }

        let active_idx = self.active_topo_idx.load(Ordering::Acquire);
        let topo_ptr = &self.topologies[active_idx] as *const GraphTopology;
        let topo = unsafe { &*topo_ptr };
        let buffers_ptr = self.buffers.as_mut_ptr();
        let x_buffers_ptr = self._crossfade_buffers.as_mut_ptr();

        // 1. Resolve Crossfades for this block
        // Store crossfade overrides: [node_idx][input_idx] -> buffer_idx (0 means no override)
        let mut block_x_map = [[0u8; crate::MAX_CHANNELS]; crate::MAX_NODES];

        let crossfades_mut_ptr = &self.topologies[active_idx].crossfades as *const [Option<CrossfadeState>; 8] as *mut [Option<CrossfadeState>; 8];

        let offset = context.sub_block_offset;
        for i in 0..8 {
            let x_state_opt = unsafe { &mut (*crossfades_mut_ptr)[i] };
            if let Some(state) = x_state_opt {
                let x_buf_idx = i;
                let old_data = &self._old_path_buffers[state.old_buffer_idx].data[offset..offset + num_samples];
                let new_data = &self.buffers[state.new_buffer_idx].data[offset..offset + num_samples];
                let x_data = &mut self._crossfade_buffers[x_buf_idx].data[..num_samples];

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

        for s_idx in 0..topo.num_stages {
            let stage = &topo.stages[s_idx][..topo.stage_counts[s_idx]];

            if let Some(pool) = &mut context.pool {
                let start_count = pool.completion.load(Ordering::Acquire);
                let num_nodes = stage.len();
                for (i, &n_idx) in stage.iter().enumerate() {
                    let worker_idx = i % pool.worker_producers.len();
                    let routing = &topo.routing[n_idx];
                    let mut resolved_inputs = [0usize; 16];
                    let mut resolved_outputs = [0usize; 16];

                    for j in 0..routing.input_count.min(crate::MAX_CHANNELS) {
                        let v_idx = routing.input_indices[j].min(crate::MAX_NODES - 1);
                        let mut p_idx = topo.virtual_to_physical[v_idx];

                        // Apply crossfade override
                        let p_override = block_x_map[n_idx][j];
                        if p_override != 0 {
                            p_idx = p_override as usize;
                        }
                        resolved_inputs[j] = p_idx;
                    }

                    for (j, resolved_out) in resolved_outputs.iter_mut().enumerate().take(routing.output_count.min(crate::MAX_CHANNELS)) {
                        let v_idx = routing.output_indices[j].min(crate::MAX_NODES - 1);
                        *resolved_out = topo.virtual_to_physical[v_idx];
                    }

                    let _ = pool.worker_producers[worker_idx].push(Job {
                        node_ptr: &self.nodes[n_idx] as *const _,
                        num_samples,
                        sub_block_offset: context.sub_block_offset,
                        buffers_ptr,
                        x_buffers_ptr,
                        input_indices: resolved_inputs,
                        output_indices: resolved_outputs,
                        input_count: routing.input_count,
                        output_count: routing.output_count,
                        node_idx: n_idx,
                        telemetry_ptr: &self.telemetry.node_times_cycles as *const _,
                        transport: context.transport.copied(),
                        is_last_sub_block: context.is_last_sub_block,
                    });
                }

                let mut spins = 0;
                let target = start_count + num_nodes;
                while pool.completion.load(Ordering::Acquire) < target {
                    if spins < 10000 {
                        std::hint::spin_loop();
                    } else {
                        std::thread::yield_now();
                    }
                    spins += 1;
                }
            } else {
                for &n_idx in stage {
                    let node = &self.nodes[n_idx];
                    let routing = &topo.routing[n_idx];
                    let mut node_inputs_storage = [ &[][..]; crate::MAX_CHANNELS ];
                    let input_count = routing.input_count.min(crate::MAX_CHANNELS);
                    for i in 0..input_count {
                        let v_idx = routing.input_indices.get(i).copied().unwrap_or(0).min(crate::MAX_NODES - 1);
                        let mut p_idx = topo.virtual_to_physical[v_idx];
                        let p_override = block_x_map[n_idx][i];
                        if p_override != 0 {
                            p_idx = p_override as usize;
                        }

                        if p_idx >= 64 {
                            let x_idx = p_idx - 64;
                            if x_idx < 8 {
                                unsafe { node_inputs_storage[i] = &(&(*x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                            }
                        } else if p_idx < 64 {
                            let offset = context.sub_block_offset;
                            unsafe { node_inputs_storage[i] = &(&(*buffers_ptr.add(p_idx)).data)[offset..offset + num_samples]; }
                        }
                    }
                    let mut node_outputs_reconstructed: [&mut [f32]; crate::MAX_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
                    let output_count = routing.output_count.min(crate::MAX_CHANNELS);
                    let offset = context.sub_block_offset;
                    for (i, node_out) in node_outputs_reconstructed.iter_mut().enumerate().take(output_count) {
                        let v_idx = routing.output_indices.get(i).copied().unwrap_or(0).min(crate::MAX_NODES - 1);
                        let p_idx = topo.virtual_to_physical.get(v_idx).copied().unwrap_or(0).min(crate::MAX_NODES - 1);
                        unsafe {
                            *node_out = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr().add(offset), num_samples);
                        }
                    }

                    #[cfg(target_arch = "x86_64")]
                    let start = unsafe { std::arch::x86_64::_rdtsc() };

                    let mut inner_context = crate::processors::ProcessContext { pool: None, transport: context.transport, sub_block_offset: context.sub_block_offset, is_last_sub_block: context.is_last_sub_block };
                    unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count], &mut inner_context); }

                    #[cfg(target_arch = "x86_64")]
                    {
                        let elapsed = unsafe { std::arch::x86_64::_rdtsc() } - start;
                        self.telemetry.node_times_cycles[n_idx].store(elapsed, Ordering::Relaxed);
                    }
                }
            }
        }

        if !external_outputs.is_empty() {
            let p0 = topo.virtual_to_physical[0];
            let offset = context.sub_block_offset;
            external_outputs[0].copy_from_slice(&self.buffers[p0].data[offset..offset + num_samples]);
        }
        if external_outputs.len() >= 2 {
            let p1 = topo.virtual_to_physical[1];
            let offset = context.sub_block_offset;
            external_outputs[1].copy_from_slice(&self.buffers[p1].data[offset..offset + num_samples]);
        }

        // Before finishing process, copy current buffers to old_path_buffers for crossfading in next block
        // ONLY if this is the last sub-block of the engine cycle to preserve the entire "previous block" state
        if is_last_sub_block {
            for i in 0..crate::MAX_NODES {
                self._old_path_buffers[i].data.copy_from_slice(&self.buffers[i].data);
            }
        }

        #[cfg(target_arch = "x86_64")]
        let has_avx2 = is_x86_feature_detected!("avx2");

        for n_idx in 0..topo.node_count.min(crate::MAX_NODES) {
            let routing = &topo.routing[n_idx];
            let mut node_peak = if context.sub_block_offset == 0 { 0.0f32 } else { f32::from_bits(self.telemetry.peak_levels[n_idx].load(Ordering::Relaxed)) };

            for o_idx in 0..routing.output_count {
                let v_out = routing.output_indices.get(o_idx).copied().unwrap_or(0).min(crate::MAX_NODES - 1);
                let p_idx = topo.virtual_to_physical.get(v_out).copied().unwrap_or(0).min(crate::MAX_NODES - 1);
                let offset = context.sub_block_offset;
                let data = &self.buffers[p_idx].data[offset..offset + num_samples];

                let mut channel_peak = 0.0f32;
                #[cfg(target_arch = "x86_64")]
                {
                    if has_avx2 {
                        unsafe {
                            use std::arch::x86_64::*;
                            let mut v_peak = _mm256_setzero_ps();
                            let abs_mask = _mm256_castsi256_ps(_mm256_set1_epi32(0x7FFFFFFF));
                            let mut j = 0;
                            while j + 8 <= num_samples {
                                let v_data = _mm256_loadu_ps(data.as_ptr().add(j));
                                let v_abs = _mm256_and_ps(v_data, abs_mask);
                                v_peak = _mm256_max_ps(v_peak, v_abs);
                                j += 8;
                            }
                            let mut res = [0.0f32; 8];
                            _mm256_storeu_ps(res.as_mut_ptr(), v_peak);
                            for &val in &res { if val > channel_peak { channel_peak = val; } }
                            while j < num_samples {
                                let abs = data[j].abs();
                                if abs > channel_peak { channel_peak = abs; }
                                j += 1;
                            }
                        }
                    } else {
                        for &sample in data {
                            let abs = sample.abs();
                            if abs > channel_peak { channel_peak = abs; }
                        }
                    }
                }
                #[cfg(not(target_arch = "x86_64"))]
                {
                    for &sample in data {
                        let abs = sample.abs();
                        if abs > channel_peak { channel_peak = abs; }
                    }
                }
                if channel_peak > node_peak { node_peak = channel_peak; }
            }
            self.telemetry.peak_levels[n_idx].store(node_peak.to_bits(), Ordering::Relaxed);
        }
    }
    fn setup(&mut self, config: crate::AudioConfig) {
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

                    if let Some(ref prod) = self.garbage_producer {
                        processor.set_garbage_producer(prod.clone());
                    }

                    // SAFETY: We replace the processor inside UnsafeCell.
                    let old_proc = unsafe { std::ptr::replace(node.processor.get(), processor) };
                    if let Some(ref mut prod) = self.garbage_producer {
                        if let Err(leaked) = prod.push(old_proc) {
                            std::mem::forget(leaked);
                        }
                    } else {
                        std::mem::forget(old_proc);
                    }
                }
            }
            TopologyMutation::AddNode { node_idx: _, mut processor } => {
                if self.node_count < 64 {
                    let idx = self.node_count;
                    if let Some(ref prod) = self.garbage_producer {
                        processor.set_garbage_producer(prod.clone());
                    }
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
            control_plane::Command::CommitTopology => {
                self.calculate_stages();
                self.commit_graph();
            }
            _ => {
                for node in self.nodes.iter() { unsafe { (*node.processor.get()).apply_command(command); } }
            }
        }
    }
    fn set_garbage_producer(&mut self, producer: ipc_layer::Producer<Box<dyn AudioProcessor>>) {
        self.garbage_producer = Some(producer);
    }
    fn collect_telemetry(&self, node_times: &mut [u64; crate::MAX_NODES], peak_levels: &mut [f32; crate::MAX_NODES]) {
        for i in 0..crate::MAX_NODES {
            node_times[i] = self.telemetry.node_times_cycles[i].load(Ordering::Relaxed);
            peak_levels[i] = f32::from_bits(self.telemetry.peak_levels[i].load(Ordering::Relaxed));
        }
    }
}
