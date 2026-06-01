use crate::traits::{AudioProcessor, ProcessorNode};
use crate::processors::{BiquadProcessor, GainProcessor, SimdBiquadProcessor, CrossfaderProcessor};
use crate::backends::SpaData;
use ipc_layer::{AudioBlock, Producer, RingBuffer};
use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

#[derive(Clone, Copy)]
pub struct CrossfadeState {
    pub node_idx: usize,
    pub input_idx: usize,
    pub old_buffer_idx: usize,
    pub new_buffer_idx: usize,
    pub remaining_samples: u32,
    pub total_samples: u32,
}

#[derive(Clone, Copy)]
pub struct NodeRouting {
    pub input_indices: [usize; 16],
    pub output_indices: [usize; 16],
    pub input_count: usize,
    pub output_count: usize,
}

#[derive(Clone, Copy)]
pub struct GraphTopology {
    pub routing: [NodeRouting; 64],
    pub virtual_to_physical: [usize; 64],
    pub stages: [[usize; 64]; 64],
    pub stage_counts: [usize; 64],
    pub num_stages: usize,
    pub crossfades: [Option<CrossfadeState>; 8],
    pub node_count: usize,
}

pub struct TopologyStats {
    pub average_load_ns: [u64; 64],
    pub optimization_suggestions: [u8; 64], // 0: None, 1: Parallelize, 2: Merge
    history: [[u64; 100]; 64],
    history_idx: usize,
}

impl TopologyStats {
    pub fn new() -> Self {
        Self {
            average_load_ns: [0; 64],
            optimization_suggestions: [0; 64],
            history: [[0; 100]; 64],
            history_idx: 0,
        }
    }
    pub fn record(&mut self, loads: &[u64; 64], topo: &GraphTopology) {
        for i in 0..64 {
            self.history[i][self.history_idx] = loads[i];
            let mut sum = 0u64;
            for j in 0..100 { sum += self.history[i][j]; }
            self.average_load_ns[i] = sum / 100;
        }
        self.history_idx = (self.history_idx + 1) % 100;

        for i in 0..topo.node_count {
            if self.average_load_ns[i] > 50000 {
                self.optimization_suggestions[i] = 1;
            } else if self.average_load_ns[i] < 500 && self.average_load_ns[i] > 0 {
                self.optimization_suggestions[i] = 2;
            } else {
                self.optimization_suggestions[i] = 0;
            }
        }
    }
}

#[repr(C)]
pub struct SpaBuffer {
    pub n_datas: u32,
    pub datas: *mut SpaData,
}

/// A Directed Acyclic Graph (DAG) of audio processors.
///
/// `ProcessorGraph` manages a collection of `ProcessorNode`s and their interconnections.
/// It uses a topological stage grouping strategy to identify independent branches
/// of the graph that can be safely executed in parallel by the `TaskPool`.
///
/// Signal routing is handled via a virtual-to-physical index mapping into a
/// pre-allocated pool of `AudioBlock` scratchpad buffers. This design allows for
/// zero-allocation, sample-accurate re-wiring during the real-time processing loop.
pub struct ProcessorGraph {
    pub(crate) nodes: Arc<Vec<ProcessorNode>>,
    pub(crate) buffers: Box<[AudioBlock; 64]>,
    pub last_node_load_ns: [u64; 64],
    pub stats: TopologyStats,
    pub(crate) crossfade_buffers: [AudioBlock; 8],
    pub topologies: Box<[GraphTopology; 2]>,
    pub(crate) active_topo_idx: Arc<AtomicUsize>,
    pub pool: Option<TaskPool>,
    pub(crate) needs_commit: bool,

    _stage_scratch_assigned: [bool; 64],
    _stage_scratch_in_degree: [usize; 64],
}

pub struct TaskPool {
    pub(crate) _workers: Vec<thread::JoinHandle<()>>,
    pub(crate) worker_producers: Vec<Producer<usize>>,
    pub(crate) completion: Arc<AtomicUsize>,
    pub(crate) running: Arc<AtomicBool>,
}

impl TaskPool {
    pub fn new(num_workers: usize) -> Self {
        let mut workers = Vec::new();
        let mut worker_producers = Vec::new();
        let completion = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicBool::new(true));

        for _ in 0..num_workers {
            let (_prod, mut cons) = RingBuffer::new(128).split();
            let running_worker = running.clone();
            let completion_worker = completion.clone();

            let handle = thread::spawn(move || {
                while running_worker.load(Ordering::Relaxed) {
                    if let Some(_node_idx) = cons.pop() {
                        completion_worker.fetch_add(1, Ordering::SeqCst);
                    } else {
                        thread::yield_now();
                    }
                }
            });

            workers.push(handle);
            worker_producers.push(_prod);
        }

        Self { _workers: workers, worker_producers, completion, running }
    }
}

impl Drop for TaskPool {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        for handle in self._workers.drain(..) {
            let _ = handle.join();
        }
    }
}

impl ProcessorGraph {
    pub fn new() -> Self {
        let buffers = Box::new([AudioBlock { data: [0.0f32; 128] }; 64]);
        let mut v2p = [0usize; 64];
        for i in 0..64 { v2p[i] = i; }
        let topo = GraphTopology {
            routing: [NodeRouting { input_indices: [0; 16], output_indices: [0; 16], input_count: 0, output_count: 0 }; 64],
            virtual_to_physical: v2p,
            stages: [[0; 64]; 64],
            stage_counts: [0; 64],
            num_stages: 0,
            crossfades: [None; 8],
            node_count: 0,
        };
        Self {
            nodes: Arc::new(Vec::with_capacity(64)),
            buffers,
            last_node_load_ns: [0u64; 64],
            stats: TopologyStats::new(),
            crossfade_buffers: [AudioBlock { data: [0.0f32; 128] }; 8],
            topologies: Box::new([topo; 2]),
            active_topo_idx: Arc::new(AtomicUsize::new(0)),
            needs_commit: false,
            _stage_scratch_assigned: [false; 64],
            _stage_scratch_in_degree: [0; 64],
            pool: Some(TaskPool::new(4)),
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

    pub fn current_topology(&self) -> &GraphTopology {
        let idx = self.active_topo_idx.load(Ordering::Acquire);
        &self.topologies[idx]
    }

    pub fn calculate_stages(&mut self) {
        let active_idx = self.active_topo_idx.load(Ordering::Acquire);
        let inactive_idx = (active_idx + 1) % 2;

        let n = self.topologies[inactive_idx].node_count;
        if n == 0 { return; }

        let mut in_degree = [0usize; 64];
        let mut assigned = [false; 64];

        for i in 0..n {
            let routing_i = &self.topologies[inactive_idx].routing[i];
            for j in 0..n {
                if i == j { continue; }
                let routing_j = &self.topologies[inactive_idx].routing[j];
                for k in 0..routing_j.output_count {
                    let out = routing_j.output_indices[k];
                    for l in 0..routing_i.input_count {
                        if routing_i.input_indices[l] == out {
                            in_degree[i] += 1;
                            break;
                        }
                    }
                }
            }
        }

        let topo = &mut self.topologies[inactive_idx];
        topo.num_stages = 0;
        while assigned[..n].iter().any(|&a| !a) {
            let mut count = 0;
            for i in 0..n {
                if !assigned[i] && in_degree[i] == 0 {
                    topo.stages[topo.num_stages][count] = i;
                    count += 1;
                }
            }
            if count == 0 { break; }
            for &i in &topo.stages[topo.num_stages][..count] {
                assigned[i] = true;
                let routing_i = &topo.routing[i];
                for j in 0..n {
                    if assigned[j] { continue; }
                    let routing_j = &topo.routing[j];
                    for k in 0..routing_i.output_count {
                        let out = routing_i.output_indices[k];
                        for l in 0..routing_j.input_count {
                            if routing_j.input_indices[l] == out {
                                in_degree[j] -= 1;
                                break;
                            }
                        }
                    }
                }
            }
            topo.stage_counts[topo.num_stages] = count;
            topo.num_stages += 1;
            if topo.num_stages >= 64 { break; }
        }
    }

    pub fn commit_graph(&mut self) {
        let active = self.active_topo_idx.load(Ordering::Acquire);
        let inactive = (active + 1) % 2;
        self.active_topo_idx.store(inactive, Ordering::Release);
        self.needs_commit = false;
    }

    pub fn add_node(&mut self, processor: Box<dyn AudioProcessor>, inputs: Vec<usize>, outputs: Vec<usize>) {
        if self.nodes.len() >= 64 { return; }
        let idx = self.nodes.len();
        Arc::get_mut(&mut self.nodes).unwrap().push(ProcessorNode { processor: Arc::new(std::cell::UnsafeCell::new(processor)) });

        let topo = self.inactive_topology_mut();
        topo.routing[idx].input_count = inputs.len().min(16);
        for i in 0..topo.routing[idx].input_count { topo.routing[idx].input_indices[i] = inputs[i]; }
        topo.routing[idx].output_count = outputs.len().min(16);
        for i in 0..topo.routing[idx].output_count { topo.routing[idx].output_indices[i] = outputs[i]; }
        topo.node_count += 1;

        self.calculate_stages();
    }

    pub fn get_buffer_levels(&self) -> [f32; 64] {
        let mut levels = [0.0f32; 64];
        for i in 0..64 {
            let mut peak = 0.0f32;
            for &s in self.buffers[i].data.iter() {
                peak = peak.max(s.abs());
            }
            levels[i] = peak;
        }
        levels
    }
    pub fn get_node_load_ns(&self) -> [u64; 64] {
        self.last_node_load_ns
    }
}

pub struct ConstantProcessor { pub val: f32 }
impl AudioProcessor for ConstantProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for out in outputs { for s in out.iter_mut() { *s = self.val; } }
    }
}

impl AudioProcessor for ProcessorGraph {
    fn apply_command(&mut self, command: &control_plane::Command) {
        match command {
            control_plane::Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let n_idx = *node_idx as usize;
                let i_idx = *input_idx as usize;
                if n_idx < self.nodes.len() {
                    let topo = self.inactive_topology_mut();
                    if i_idx < topo.routing[n_idx].input_count {
                        topo.routing[n_idx].input_indices[i_idx] = *new_buffer_idx as usize;
                        self.calculate_stages();
                        self.needs_commit = true;
                    }
                }
            }
            control_plane::Command::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let n_idx = *node_idx as usize;
                let o_idx = *output_idx as usize;
                if n_idx < self.nodes.len() {
                    let topo = self.inactive_topology_mut();
                    if o_idx < topo.routing[n_idx].output_count {
                        topo.routing[n_idx].output_indices[o_idx] = *new_buffer_idx as usize;
                        self.calculate_stages();
                        self.needs_commit = true;
                    }
                }
            }
            control_plane::Command::SwapProcessor { node_idx, processor_type_id } => {
                if let Some(node) = Arc::get_mut(&mut self.nodes).and_then(|n| n.get_mut(*node_idx as usize)) {
                    match processor_type_id {
                        1 => { unsafe { *node.processor.get() = Box::new(BiquadProcessor::new(0, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })); } }
                        2 => { unsafe { *node.processor.get() = Box::new(GainProcessor::new(0, 1.0)); } }
                        20 => { unsafe { *node.processor.get() = Box::new(CrossfaderProcessor::new()); } }
                        _ => {}
                    }
                }
            }
            control_plane::Command::AddNode { processor_type_id, .. } => {
                let processor: Box<dyn AudioProcessor> = match processor_type_id {
                    1 => Box::new(BiquadProcessor::new(0, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })),
                    2 => Box::new(GainProcessor::new(0, 1.0)),
                    3 => Box::new(SimdBiquadProcessor::new(audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })),
                    20 => Box::new(CrossfaderProcessor::new()),
                    _ => Box::new(GainProcessor::new(0, 0.0)), // Silence
                };
                self.add_node(processor, vec![], vec![]);
            }
            _ => {
                for node in self.nodes.iter() { unsafe { (*node.processor.get()).apply_command(command); } }
            }
        }
    }
    fn get_telemetry(&self, node_load: &mut [u64; 64], node_avg_load: &mut [u64; 64], suggestions: &mut [u8; 64], buffer_levels: &mut [f32; 64]) {
        *node_load = self.get_node_load_ns();
        *node_avg_load = self.stats.average_load_ns;
        *suggestions = self.stats.optimization_suggestions;
        *buffer_levels = self.get_buffer_levels();
    }
    fn process(&mut self, external_inputs: &[&[f32]], external_outputs: &mut [&mut [f32]]) {
        let num_samples = if !external_outputs.is_empty() { external_outputs[0].len() } else if !external_inputs.is_empty() { external_inputs[0].len() } else { 0 };
        if num_samples == 0 { return; }

        if self.needs_commit {
            self.commit_graph();
            self.needs_commit = false;
        }

        let topo = *self.current_topology();

        for (i, &input) in external_inputs.iter().enumerate().take(16) {
            let p_idx = topo.virtual_to_physical[i];
            self.buffers[p_idx].data[..num_samples].copy_from_slice(input);
        }

        let buffers_ptr = self.buffers.as_mut_ptr();
        let mut node_loads = [0u64; 64];

        for s_idx in 0..topo.num_stages {
            let stage = &topo.stages[s_idx][..topo.stage_counts[s_idx]];

            if let Some(pool) = &mut self.pool {
                let stage_start = Instant::now();
                pool.completion.store(0, Ordering::Release);
                let num_nodes = stage.len();
                for (i, &n_idx) in stage.iter().enumerate() {
                    let worker_idx = i % pool.worker_producers.len();
                    let _ = pool.worker_producers[worker_idx].push(n_idx);
                }

                while pool.completion.load(Ordering::Acquire) < num_nodes {
                    std::thread::yield_now();
                }
                let stage_duration = stage_start.elapsed().as_nanos() as u64;
                for &n_idx in stage {
                    let load = stage_duration / num_nodes as u64;
                    self.last_node_load_ns[n_idx] = load;
                    node_loads[n_idx] = load;
                }
            } else {
                for &n_idx in stage {
                    let node_start = Instant::now();
                    let node = &self.nodes[n_idx];
                    let routing = &topo.routing[n_idx];
                    let mut node_inputs_storage = [ &[][..]; 16 ];
                    for i in 0..routing.input_count {
                        let p_idx = topo.virtual_to_physical[routing.input_indices[i].min(63)];
                        unsafe { node_inputs_storage[i] = &(&(*buffers_ptr.add(p_idx)).data)[..num_samples]; }
                    }
                    let mut node_outputs_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|i| {
                        if i < routing.output_count {
                            let p_idx = topo.virtual_to_physical[routing.output_indices[i].min(63)];
                            unsafe { std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr(), num_samples) }
                        } else { &mut [] }
                    });
                    unsafe { (*node.processor.get()).process(&node_inputs_storage[..routing.input_count], &mut node_outputs_reconstructed[..routing.output_count]); }
                    let load = node_start.elapsed().as_nanos() as u64;
                    self.last_node_load_ns[n_idx] = load;
                    node_loads[n_idx] = load;
                }
            }
        }

        if external_outputs.len() >= 1 {
            let p0 = topo.virtual_to_physical[0];
            external_outputs[0].copy_from_slice(&self.buffers[p0].data[..num_samples]);
        }
        if external_outputs.len() >= 2 {
            let p1 = topo.virtual_to_physical[1];
            external_outputs[1].copy_from_slice(&self.buffers[p1].data[..num_samples]);
        }

        self.stats.record(&node_loads, &topo);
    }
}
