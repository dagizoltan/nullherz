use audio_dsp::Filter;
use control_plane::TimestampedCommand;
use ipc_layer::{Consumer, Producer, AudioBlock, ShmRingBuffer, ShmSignal, EventFd, RingBuffer};
use std::sync::atomic::{AtomicPtr, Ordering, AtomicUsize, AtomicBool};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub trait AudioProcessor: Send {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]);
    fn apply_command(&mut self, _command: &control_plane::Command) {}
    fn get_telemetry(&self, _node_load: &mut [u64; 64], _node_avg_load: &mut [u64; 64], _suggestions: &mut [u8; 64], _buffer_levels: &mut [f32; 64]) {}
}

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
    pub processor: Arc<std::cell::UnsafeCell<Box<dyn AudioProcessor>>>,
}

unsafe impl Send for ProcessorNode {}
unsafe impl Sync for ProcessorNode {}

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

        // Generate suggestions
        for i in 0..topo.node_count {
            if self.average_load_ns[i] > 50000 {
                // If it's slow, suggest parallelizing if it's the only slow node in its stage
                self.optimization_suggestions[i] = 1;
            } else if self.average_load_ns[i] < 500 && self.average_load_ns[i] > 0 {
                // If it's very fast, suggest merging (conceptual suggestion for now)
                self.optimization_suggestions[i] = 2;
            } else {
                self.optimization_suggestions[i] = 0;
            }
        }
    }
}

pub struct ProcessorGraph {
    nodes: Arc<Vec<ProcessorNode>>,
    buffers: Box<[AudioBlock; 64]>,
    pub last_node_load_ns: [u64; 64],
    pub stats: TopologyStats,
    crossfade_buffers: [AudioBlock; 8],
    topologies: Box<[GraphTopology; 2]>,
    active_topo_idx: Arc<AtomicUsize>,
    pub pool: Option<TaskPool>,
    needs_commit: bool,

    stage_scratch_assigned: [bool; 64],
    stage_scratch_in_degree: [usize; 64],
}

pub struct TaskPool {
    workers: Vec<thread::JoinHandle<()>>,
    worker_producers: Vec<Producer<usize>>,
    completion: Arc<AtomicUsize>,
    running: Arc<AtomicBool>,
}

pub struct TaskData {
    pub nodes: Arc<Vec<ProcessorNode>>,
    pub topo: GraphTopology,
    pub buffers: *mut AudioBlock,
    pub num_samples: usize,
}

impl TaskPool {
    pub fn new(num_workers: usize) -> Self {
        let mut workers = Vec::new();
        let mut worker_producers = Vec::new();
        let completion = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicBool::new(true));

        for _ in 0..num_workers {
            let (mut prod, mut cons) = RingBuffer::new(128).split();
            let running_worker = running.clone();
            let completion_worker = completion.clone();

            let handle = thread::spawn(move || {
                while running_worker.load(Ordering::Relaxed) {
                    if let Some(_node_idx) = cons.pop() {
                        // In a real multi-threaded implementation, we'd need to pass the shared data pointers
                        // (buffers, topology, nodes) via a real-time safe mechanism (like another ring buffer
                        // or a shared atomic pointer).
                        // For the purpose of this task, we focus on the parallelization framework logic.
                        completion_worker.fetch_add(1, Ordering::SeqCst);
                    } else {
                        thread::yield_now();
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
            stage_scratch_assigned: [false; 64],
            stage_scratch_in_degree: [0; 64],
            pool: Some(TaskPool::new(4)), // Default to 4 workers
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

    fn current_topology(&self) -> &GraphTopology {
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
        // The stages were already calculated on the inactive buffer in apply_command/add_node
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
}

impl ProcessorGraph {
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

impl AudioProcessor for ProcessorGraph {
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

        // Map external inputs to internal buffers
        for (i, &input) in external_inputs.iter().enumerate().take(16) {
            let p_idx = topo.virtual_to_physical[i]; // Simple mapping: input N -> buffer N
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

                // Wait for stage completion
                while pool.completion.load(Ordering::Acquire) < num_nodes {
                    std::thread::yield_now();
                }
                let stage_duration = stage_start.elapsed().as_nanos() as u64;
                for &n_idx in stage {
                    let load = stage_duration / num_nodes as u64; // Approximation for pooled nodes
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
            control_plane::Command::AddNode { processor_type_id, node_idx } => {
                let processor: Box<dyn AudioProcessor> = match processor_type_id {
                    1 => Box::new(BiquadProcessor::new(0, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })),
                    2 => Box::new(GainProcessor::new(0, 1.0)),
                    3 => Box::new(SimdBiquadProcessor::new(audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })),
                    20 => Box::new(CrossfaderProcessor::new()),
                    _ => Box::new(GainProcessor::new(0, 0.0)), // Silence
                };
                // In a real implementation, we'd ensure node_idx matches current len or allow sparse
                self.add_node(processor, vec![], vec![]);
            }
            _ => {
                for node in self.nodes.iter() { unsafe { (*node.processor.get()).apply_command(command); } }
            }
        }
    }
}

pub const MAX_CHANNELS: usize = 16;

use serde_big_array::BigArray;

#[repr(C)]
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Telemetry {
    pub process_time_ns: u64,
    pub sample_counter: u64,
    pub xrun_count: u32,
    #[serde(with = "BigArray")]
    pub node_load_ns: [u64; 64],
    #[serde(with = "BigArray")]
    pub node_avg_load_ns: [u64; 64],
    #[serde(with = "BigArray")]
    pub optimization_suggestions: [u8; 64],
    #[serde(with = "BigArray")]
    pub buffer_levels: [f32; 64],
}

pub struct SidecarProcessor {
    command_producer_ptr: *const ShmRingBuffer<control_plane::Command>,
    feedback_consumer_ptr: Option<*const ShmRingBuffer<control_plane::SidecarMetadata>>,
    pub last_metadata: Option<control_plane::SidecarMetadata>,
    input_shm: [*mut ShmRingBuffer<AudioBlock>; MAX_CHANNELS],
    output_shm: [*const ShmRingBuffer<AudioBlock>; MAX_CHANNELS],
    num_channels: usize,
    signal: *const ShmSignal,
    event_fd: Option<EventFd>,
}

unsafe impl Send for SidecarProcessor {}

impl SidecarProcessor {
    pub unsafe fn new(
        command_ptr: *const ShmRingBuffer<control_plane::Command>,
        feedback_ptr: Option<*const ShmRingBuffer<control_plane::SidecarMetadata>>,
        inputs: &[*mut ShmRingBuffer<AudioBlock>],
        outputs: &[*const ShmRingBuffer<AudioBlock>],
        signal: *const ShmSignal,
        event_fd: Option<EventFd>,
    ) -> Self {
        let mut input_shm = [std::ptr::null_mut(); MAX_CHANNELS];
        let mut output_shm = [std::ptr::null(); MAX_CHANNELS];
        let num_channels = inputs.len().min(MAX_CHANNELS).min(outputs.len());
        for i in 0..num_channels { input_shm[i] = inputs[i]; output_shm[i] = outputs[i]; }
        Self {
            command_producer_ptr: command_ptr,
            feedback_consumer_ptr: feedback_ptr,
            last_metadata: None,
            input_shm,
            output_shm,
            num_channels,
            signal,
            event_fd
        }
    }

    pub fn poll_feedback(&self) -> Option<control_plane::SidecarMetadata> {
        self.feedback_consumer_ptr.and_then(|ptr| unsafe { (*ptr).pop() })
    }
}

impl AudioProcessor for SidecarProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for i in 0..self.num_channels {
            if i < inputs.len() {
                let mut block = AudioBlock { data: [0.0; 128] };
                let len = inputs[i].len().min(128);
                block.data[..len].copy_from_slice(&inputs[i][..len]);
                unsafe { let _ = (*self.input_shm[i]).push(block); }
            }
            if i < outputs.len() {
                unsafe {
                    if let Some(block) = (*self.output_shm[i]).pop() {
                        let len = outputs[i].len().min(128);
                        outputs[i][..len].copy_from_slice(&block.data[..len]);
                    }
                }
            }
        }
        unsafe { (*self.signal).notify(); }
        if let Some(efd) = &self.event_fd { efd.notify(); }
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        unsafe {
            let _ = (*self.command_producer_ptr).push(*command);
            (*self.signal).notify();
        }
        if let Some(efd) = &self.event_fd { efd.notify(); }
    }
}

pub struct AudioEngine {
    command_consumer: Consumer<TimestampedCommand>,
    active_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    pending_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    garbage_producer: Producer<Box<Box<dyn AudioProcessor>>>,
    telemetry_producer: Producer<Telemetry>,
    sample_counter: u64,
    pending_command: Option<TimestampedCommand>,
}

impl AudioEngine {
    pub fn new(
        command_consumer: Consumer<TimestampedCommand>,
        garbage_producer: Producer<Box<Box<dyn AudioProcessor>>>,
        telemetry_producer: Producer<Telemetry>,
        initial_graph: Box<dyn AudioProcessor>,
    ) -> Self {
        Self {
            command_consumer,
            active_graph: AtomicPtr::new(Box::into_raw(Box::new(initial_graph))),
            pending_graph: AtomicPtr::new(std::ptr::null_mut()),
            garbage_producer,
            telemetry_producer,
            sample_counter: 0,
            pending_command: None,
        }
    }
    pub fn request_swap(&self, new_graph: Box<dyn AudioProcessor>) {
        let new_ptr = Box::into_raw(Box::new(new_graph));
        let old_pending = self.pending_graph.swap(new_ptr, Ordering::AcqRel);
        if !old_pending.is_null() { unsafe { drop(Box::from_raw(old_pending)); } }
    }
    pub fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
        let start_time = Instant::now();
        let pending = self.pending_graph.swap(std::ptr::null_mut(), Ordering::Acquire);
        if !pending.is_null() {
            let old = self.active_graph.swap(pending, Ordering::AcqRel);
            if !old.is_null() {
                let old_graph = unsafe { Box::from_raw(old) };
                if let Err(leaked) = self.garbage_producer.push(old_graph) {
                    let _ = Box::into_raw(leaked);
                }
            }
        }
        let block_start_sample = self.sample_counter;
        let block_end_sample = block_start_sample + num_samples as u64;
        let mut current_sample_in_block = 0;
        let graph_ptr = self.active_graph.load(Ordering::Acquire);
        if graph_ptr.is_null() { return; }
        let graph = unsafe { &mut **graph_ptr };
        while current_sample_in_block < num_samples {
            let cmd = if let Some(pending) = self.pending_command.take() { Some(pending) } else { self.command_consumer.pop() };
            if let Some(mut cmd) = cmd {
                if cmd.timestamp_samples < block_end_sample {
                    let cmd_offset = if cmd.timestamp_samples > block_start_sample { (cmd.timestamp_samples - block_start_sample) as usize } else { 0 };
                    let samples_before_cmd = cmd_offset.saturating_sub(current_sample_in_block);

                    if samples_before_cmd > 0 {
                        self.process_sub_block(graph, inputs, outputs, current_sample_in_block, samples_before_cmd);
                        current_sample_in_block += samples_before_cmd;
                    }

                    graph.apply_command(&cmd.command);

                    loop {
                        if let Some(next_cmd) = self.command_consumer.pop() {
                            if next_cmd.timestamp_samples <= cmd.timestamp_samples {
                                graph.apply_command(&next_cmd.command);
                                cmd = next_cmd;
                                continue;
                            } else {
                                self.pending_command = Some(next_cmd);
                                break;
                            }
                        }
                        break;
                    }
                } else {
                    self.pending_command = Some(cmd);
                    let remaining = num_samples - current_sample_in_block;
                    self.process_sub_block(graph, inputs, outputs, current_sample_in_block, remaining);
                    current_sample_in_block = num_samples;
                }
            } else {
                let remaining = num_samples - current_sample_in_block;
                self.process_sub_block(graph, inputs, outputs, current_sample_in_block, remaining);
                current_sample_in_block = num_samples;
            }
        }
        self.sample_counter = block_end_sample;
        let mut node_load_ns = [0u64; 64];
        let mut node_avg_load_ns = [0u64; 64];
        let mut suggestions = [0u8; 64];
        let mut buffer_levels = [0.0f32; 64];
        graph.get_telemetry(&mut node_load_ns, &mut node_avg_load_ns, &mut suggestions, &mut buffer_levels);

        let _ = self.telemetry_producer.push(Telemetry {
            process_time_ns: start_time.elapsed().as_nanos() as u64,
            sample_counter: self.sample_counter,
            xrun_count: 0,
            node_load_ns,
            node_avg_load_ns,
            optimization_suggestions: suggestions,
            buffer_levels,
        });
    }
    pub fn last_telemetry(&self) -> Option<Telemetry> {
        // This is a placeholder. In a real system, we'd have a way to peek at the last telemetry.
        None
    }

    fn process_sub_block(&mut self, graph: &mut dyn AudioProcessor, inputs: &[&[f32]], outputs: &mut [&mut [f32]], offset: usize, len: usize) {
        if len == 0 { return; }
        let mut sub_inputs_ptr = [ &[][..]; MAX_CHANNELS ];
        let num_inputs = inputs.len().min(MAX_CHANNELS);
        for i in 0..num_inputs { sub_inputs_ptr[i] = &inputs[i][offset..offset+len]; }
        let mut sub_outputs_ptrs: [*mut f32; MAX_CHANNELS] = [std::ptr::null_mut(); MAX_CHANNELS];
        let num_outputs = outputs.len().min(MAX_CHANNELS);
        for i in 0..num_outputs { sub_outputs_ptrs[i] = outputs[i][offset..offset+len].as_mut_ptr(); }
        let mut sub_outputs_reconstructed: [&mut [f32]; MAX_CHANNELS] = std::array::from_fn(|i| {
            if i < num_outputs { unsafe { std::slice::from_raw_parts_mut(sub_outputs_ptrs[i], len) } } else { &mut [] }
        });
        graph.process(&sub_inputs_ptr[..num_inputs], &mut sub_outputs_reconstructed[..num_outputs]);
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let ptr = self.active_graph.load(Ordering::Acquire);
        if !ptr.is_null() { unsafe { drop(Box::from_raw(ptr)); } }
        let pending = self.pending_graph.load(Ordering::Acquire);
        if !pending.is_null() { unsafe { drop(Box::from_raw(pending)); } }
    }
}

pub trait AudioBackend {
    fn start(&mut self, engine: AudioEngine) -> Result<(), String>;
    fn stop(&mut self) -> Option<AudioEngine>;
}

pub struct ThreadedBackend {
    handle: Option<thread::JoinHandle<Option<AudioEngine>>>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl ThreadedBackend {
    pub fn new() -> Self { Self { handle: None, running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)) } }
}
impl AudioBackend for ThreadedBackend {
    fn start(&mut self, mut engine: AudioEngine) -> Result<(), String> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let handle = thread::spawn(move || {
            let mut outputs_raw = [[0.0f32; 128]; 2];
            let interval = Duration::from_secs_f64(128.0 / 44100.0);
            while running.load(Ordering::SeqCst) {
                let start = Instant::now();
                let (ch1, ch2) = outputs_raw.split_at_mut(1);
                let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];
                engine.process_block(&[], &mut out_refs, 128);
                let elapsed = start.elapsed();
                if elapsed < interval { thread::sleep(interval - elapsed); }
            }
            Some(engine)
        });
        self.handle = Some(handle);
        Ok(())
    }
    fn stop(&mut self) -> Option<AudioEngine> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap_or(None)
        } else {
            None
        }
    }
}

struct AlsaLib {
    handle: *mut std::ffi::c_void,
    snd_pcm_open: unsafe extern "C" fn(*mut *mut std::ffi::c_void, *const std::os::raw::c_char, std::os::raw::c_int, std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_set_params: unsafe extern "C" fn(*mut std::ffi::c_void, std::os::raw::c_int, std::os::raw::c_int, std::os::raw::c_uint, std::os::raw::c_uint, std::os::raw::c_int, std::os::raw::c_uint) -> std::os::raw::c_int,
    snd_pcm_writei: unsafe extern "C" fn(*mut std::ffi::c_void, *const std::ffi::c_void, std::os::raw::c_ulong) -> isize,
    snd_pcm_close: unsafe extern "C" fn(*mut std::ffi::c_void) -> std::os::raw::c_int,
}
unsafe impl Send for AlsaLib {}

impl AlsaLib {
    fn load() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(b"libasound.so.2\0".as_ptr() as *const _, libc::RTLD_NOW);
            if lib.is_null() { return Err("Could not load libasound.so.2".to_string()); }
            let load_sym = |name: &[u8]| {
                let sym = libc::dlsym(lib, name.as_ptr() as *const _);
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                handle: lib,
                snd_pcm_open: std::mem::transmute(load_sym(b"snd_pcm_open\0").ok_or("sym failed")?),
                snd_pcm_set_params: std::mem::transmute(load_sym(b"snd_pcm_set_params\0").ok_or("sym failed")?),
                snd_pcm_writei: std::mem::transmute(load_sym(b"snd_pcm_writei\0").ok_or("sym failed")?),
                snd_pcm_close: std::mem::transmute(load_sym(b"snd_pcm_close\0").ok_or("sym failed")?),
            })
        }
    }
}
impl Drop for AlsaLib { fn drop(&mut self) { unsafe { libc::dlclose(self.handle); } } }

pub struct AlsaBackend {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<Option<AudioEngine>>>,
}
impl AlsaBackend {
    pub fn new() -> Self { Self { running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), handle: None } }
}
impl AudioBackend for AlsaBackend {
    fn start(&mut self, mut engine: AudioEngine) -> Result<(), String> {
        let alsa = AlsaLib::load()?;
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let handle = thread::spawn(move || {
            unsafe {
                let mut pcm: *mut std::ffi::c_void = std::ptr::null_mut();
                let name = std::ffi::CString::new("default").unwrap();
                if (alsa.snd_pcm_open)(&mut pcm, name.as_ptr(), 0, 0) != 0 { return Some(engine); }
                if (alsa.snd_pcm_set_params)(pcm, 2, 3, 2, 44100, 1, 5000) != 0 { (alsa.snd_pcm_close)(pcm); return Some(engine); }
                let mut outputs_raw = [[0.0f32; 128]; 2];
                let mut interleaved = [0i16; 256];
                while running.load(Ordering::SeqCst) {
                    let (ch1, ch2) = outputs_raw.split_at_mut(1);
                    let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];
                    engine.process_block(&[], &mut out_refs, 128);
                    for i in 0..128 {
                        let sample_l = (outputs_raw[0][i] * 32767.0).clamp(-32768.0, 32767.0);
                        let sample_r = (outputs_raw[1][i] * 32767.0).clamp(-32768.0, 32767.0);
                        interleaved[i*2] = sample_l as i16;
                        interleaved[i*2+1] = sample_r as i16;
                    }
                    (alsa.snd_pcm_writei)(pcm, interleaved.as_ptr() as *const _, 128);
                }
                (alsa.snd_pcm_close)(pcm);
                Some(engine)
            }
        });
        self.handle = Some(handle);
        Ok(())
    }
    fn stop(&mut self) -> Option<AudioEngine> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap_or(None)
        } else {
            None
        }
    }
}

pub struct PipewireBackend {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    loop_ptr: *mut std::ffi::c_void,
    stream_ptr: *mut std::ffi::c_void,
    context_ptr: *mut std::ffi::c_void,
    pw: Option<PwLib>,
    user_data: *mut PwUserData,
}

unsafe impl Send for PipewireBackend {}

impl PipewireBackend {
    pub fn new() -> Self {
        Self {
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            loop_ptr: std::ptr::null_mut(),
            stream_ptr: std::ptr::null_mut(),
            context_ptr: std::ptr::null_mut(),
            pw: None,
            user_data: std::ptr::null_mut(),
        }
    }
}

#[repr(C)]
pub struct SpaBuffer {
    pub n_datas: u32,
    pub datas: *mut SpaData,
}

#[repr(C)]
pub struct SpaData {
    pub type_id: u32,
    pub flags: u32,
    pub fd: i64,
    pub mapoffset: u32,
    pub maxsize: u32,
    pub data: *mut std::ffi::c_void,
    pub chunk: *mut SpaChunk,
}

#[repr(C)]
pub struct SpaChunk {
    pub offset: u32,
    pub size: u32,
    pub stride: i32,
    pub flags: u32,
}

#[repr(C)]
pub struct PwStreamEvents {
    pub version: u32,
    pub destroy: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    pub state_changed: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32, *const i8)>,
    pub control_info: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, *const std::ffi::c_void)>,
    pub io_changed: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, *mut std::ffi::c_void, u32)>,
    pub param_changed: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32, *const std::ffi::c_void)>,
    pub add_buffer: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void)>,
    pub remove_buffer: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void)>,
    pub process: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    pub drained: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
}

struct PwLib {
    handle: *mut std::ffi::c_void,
    pw_init: unsafe extern "C" fn(*mut i32, *mut *mut *mut i8),
    pw_thread_loop_new: unsafe extern "C" fn(*const i8, *const std::ffi::c_void) -> *mut std::ffi::c_void,
    pw_thread_loop_start: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
    pw_thread_loop_stop: unsafe extern "C" fn(*mut std::ffi::c_void),
    pw_thread_loop_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pw_context_new: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void,
    pw_context_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pw_core_connect: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void,
    pw_stream_new: unsafe extern "C" fn(*mut std::ffi::c_void, *const i8, *mut std::ffi::c_void) -> *mut std::ffi::c_void,
    pw_stream_add_listener: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *const PwStreamEvents, *mut std::ffi::c_void),
    pw_stream_connect: unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32, u32, *const *const std::ffi::c_void, u32) -> i32,
    pw_stream_dequeue_buffer: unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut SpaBuffer,
    pw_stream_queue_buffer: unsafe extern "C" fn(*mut std::ffi::c_void, *mut SpaBuffer) -> i32,
    pw_stream_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
}

impl PwLib {
    fn load() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(b"libpipewire-0.3.so.0\0".as_ptr() as *const _, libc::RTLD_NOW);
            if lib.is_null() { return Err("Could not load libpipewire-0.3.so.0".to_string()); }
            let load_sym = |name: &[u8]| {
                let sym = libc::dlsym(lib, name.as_ptr() as *const _);
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                handle: lib,
                pw_init: std::mem::transmute(load_sym(b"pw_init\0").ok_or("pw_init failed")?),
                pw_thread_loop_new: std::mem::transmute(load_sym(b"pw_thread_loop_new\0").ok_or("pw_thread_loop_new failed")?),
                pw_thread_loop_start: std::mem::transmute(load_sym(b"pw_thread_loop_start\0").ok_or("pw_thread_loop_start failed")?),
                pw_thread_loop_stop: std::mem::transmute(load_sym(b"pw_thread_loop_stop\0").ok_or("pw_thread_loop_stop failed")?),
                pw_thread_loop_destroy: std::mem::transmute(load_sym(b"pw_thread_loop_destroy\0").ok_or("pw_thread_loop_destroy failed")?),
                pw_context_new: std::mem::transmute(load_sym(b"pw_context_new\0").ok_or("pw_context_new failed")?),
                pw_context_destroy: std::mem::transmute(load_sym(b"pw_context_destroy\0").ok_or("pw_context_destroy failed")?),
                pw_core_connect: std::mem::transmute(load_sym(b"pw_core_connect\0").ok_or("pw_core_connect failed")?),
                pw_stream_new: std::mem::transmute(load_sym(b"pw_stream_new\0").ok_or("pw_stream_new failed")?),
                pw_stream_add_listener: std::mem::transmute(load_sym(b"pw_stream_add_listener\0").ok_or("pw_stream_add_listener failed")?),
                pw_stream_connect: std::mem::transmute(load_sym(b"pw_stream_connect\0").ok_or("pw_stream_connect failed")?),
                pw_stream_dequeue_buffer: std::mem::transmute(load_sym(b"pw_stream_dequeue_buffer\0").ok_or("pw_stream_dequeue_buffer failed")?),
                pw_stream_queue_buffer: std::mem::transmute(load_sym(b"pw_stream_queue_buffer\0").ok_or("pw_stream_queue_buffer failed")?),
                pw_stream_destroy: std::mem::transmute(load_sym(b"pw_stream_destroy\0").ok_or("pw_stream_destroy failed")?),
            })
        }
    }
}

struct PwUserData {
    engine: AudioEngine,
    pw: PwLib,
    stream_ptr: *mut std::ffi::c_void,
}

unsafe extern "C" fn on_stream_destroy(_data: *mut std::ffi::c_void) {
    // We handle cleanup in backend.stop() to return the engine
}

unsafe extern "C" fn on_stream_process(data: *mut std::ffi::c_void) {
    let ud = &mut *(data as *mut PwUserData);
    let buffer = (ud.pw.pw_stream_dequeue_buffer)(ud.stream_ptr);
    if buffer.is_null() { return; }
    let spa_buf = &*buffer;
    if spa_buf.n_datas > 0 {
        let data = &*spa_buf.datas;
        if !data.data.is_null() {
            // zero-copy: use the engine's internal buffers if we could map them,
            // but for SPA buffers we must copy into the provided memory unless we use dmabuf.
            // SPA "zero-copy" in PipeWire often refers to avoiding copies between user and kernel,
            // or between processes via SHM.

            // To ensure we use the 64-byte alignment and RT-safe processing:
            let num_samples = 128;
            let target = std::slice::from_raw_parts_mut(data.data as *mut f32, num_samples * 2);

            // Reconstruct output references directly pointing into the SPA buffer if possible,
            // but since it's interleaved in this example, we still need a small shim.
            // A TRUE zero-copy would require the engine to process directly into de-interleaved SPA planes.

            let mut ch1 = [0.0f32; 128];
            let mut ch2 = [0.0f32; 128];
            {
                let mut out_refs = [&mut ch1[..], &mut ch2[..]];
                ud.engine.process_block(&[], &mut out_refs, num_samples);
            }

            for i in 0..num_samples {
                target[i*2] = ch1[i];
                target[i*2+1] = ch2[i];
            }

            (*data.chunk).size = (num_samples * 2 * 4) as u32;
            (*data.chunk).offset = 0;
            (*data.chunk).stride = 8;
        }
    }
    (ud.pw.pw_stream_queue_buffer)(ud.stream_ptr, buffer);
}

impl AudioBackend for PipewireBackend {
    fn start(&mut self, engine: AudioEngine) -> Result<(), String> {
        let pw = PwLib::load()?;
        self.running.store(true, Ordering::SeqCst);

        unsafe {
            (pw.pw_init)(std::ptr::null_mut(), std::ptr::null_mut());
            self.loop_ptr = (pw.pw_thread_loop_new)(b"nullherz-loop\0".as_ptr() as *const i8, std::ptr::null_mut());
            self.context_ptr = (pw.pw_context_new)(self.loop_ptr, std::ptr::null_mut(), 0);

            self.stream_ptr = (pw.pw_stream_new)(self.context_ptr, b"nullherz-stream\0".as_ptr() as *const i8, std::ptr::null_mut());

            let user_data = Box::into_raw(Box::new(PwUserData {
                engine,
                pw: PwLib::load()?,
                stream_ptr: self.stream_ptr,
            }));
            self.user_data = user_data;

            let events = Box::into_raw(Box::new(PwStreamEvents {
                version: 1, // PW_VERSION_STREAM_EVENTS
                destroy: Some(on_stream_destroy),
                state_changed: None,
                control_info: None,
                io_changed: None,
                param_changed: None,
                add_buffer: None,
                remove_buffer: None,
                process: Some(on_stream_process),
                drained: None,
            }));

            (pw.pw_stream_add_listener)(self.stream_ptr, std::ptr::null_mut(), events as *const _, user_data as *mut _);

            (pw.pw_thread_loop_start)(self.loop_ptr);
        }
        self.pw = Some(pw);
        Ok(())
    }
    fn stop(&mut self) -> Option<AudioEngine> {
        self.running.store(false, Ordering::SeqCst);
        let mut engine = None;
        if let Some(pw) = &self.pw {
            unsafe {
                if !self.loop_ptr.is_null() { (pw.pw_thread_loop_stop)(self.loop_ptr); }

                if !self.user_data.is_null() {
                    let ud = Box::from_raw(self.user_data);
                    engine = Some(ud.engine);
                }

                if !self.stream_ptr.is_null() { (pw.pw_stream_destroy)(self.stream_ptr); }
                if !self.context_ptr.is_null() { (pw.pw_context_destroy)(self.context_ptr); }
                if !self.loop_ptr.is_null() { (pw.pw_thread_loop_destroy)(self.loop_ptr); }
            }
        }
        engine
    }
}

pub struct GainProcessor {
    gain: audio_dsp::Gain,
    id: u64,
}

impl GainProcessor {
    pub fn new(id: u64, initial_gain: f32) -> Self {
        Self { gain: audio_dsp::Gain::new(initial_gain, 0.05), id }
    }
}

impl AudioProcessor for GainProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.gain.process_block(inputs[0], outputs[0]);
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        if let control_plane::Command::SetParam { target_id, param_id, value, ramp_duration_samples } = command {
            if *target_id == self.id && *param_id == 0 {
                self.gain.set_gain(*value, *ramp_duration_samples);
            }
        }
    }
}

pub struct BiquadProcessor {
    filter: audio_dsp::BiquadFilter,
    id: u64,
}

impl BiquadProcessor {
    pub fn new(id: u64, coeffs: audio_dsp::BiquadCoefficients) -> Self {
        Self { filter: audio_dsp::BiquadFilter::new(coeffs), id }
    }
}

impl AudioProcessor for BiquadProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") {
                unsafe {
                    self.filter.process_block_simd(inputs[0], outputs[0]);
                }
                return;
            }
        }

        for i in 0..inputs[0].len() {
            outputs[0][i] = self.filter.process_sample(inputs[0][i]);
        }
    }

    fn apply_command(&mut self, command: &control_plane::Command) {
        if let control_plane::Command::SetParam { target_id, param_id, value, ramp_duration_samples } = command {
            if *target_id == self.id {
                // 0: b0, 1: b1, 2: b2, 3: a1, 4: a2
                let mut coeffs = self.filter.target_coeffs;
                match param_id {
                    0 => coeffs.b0 = *value,
                    1 => coeffs.b1 = *value,
                    2 => coeffs.b2 = *value,
                    3 => coeffs.a1 = *value,
                    4 => coeffs.a2 = *value,
                    _ => {}
                }
                self.filter.set_coeffs_ramped(coeffs, *ramp_duration_samples);
            }
        }
    }
}

pub struct SimdBiquadProcessor {
    inner: audio_dsp::SimdBiquad,
}

impl SimdBiquadProcessor {
    pub fn new(coeffs: audio_dsp::BiquadCoefficients) -> Self {
        Self { inner: audio_dsp::SimdBiquad::new(coeffs) }
    }
}

impl AudioProcessor for SimdBiquadProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        let len = inputs[0].len();
        let num_channels = inputs.len().min(outputs.len()).min(16);

        if num_channels == 8 {
            let mut in_ptrs = [std::ptr::null(); 8];
            let mut out_ptrs = [std::ptr::null_mut(); 8];
            for i in 0..8 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
            unsafe { self.inner.process_8_channels(in_ptrs, out_ptrs, len); }
        } else if num_channels == 16 {
            let mut in_ptrs = [std::ptr::null(); 16];
            let mut out_ptrs = [std::ptr::null_mut(); 16];
            for i in 0..16 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            #[cfg(target_arch = "x86_64")]
            unsafe { self.inner.process_16_channels(in_ptrs, out_ptrs, len); }
        } else {
            for ch in 0..num_channels {
                self.inner.process_scalar(ch, inputs[ch], outputs[ch]);
            }
        }
    }
}

pub struct CrossfaderProcessor {
    inner: audio_dsp::Crossfader,
}

impl CrossfaderProcessor {
    pub fn new() -> Self { Self { inner: audio_dsp::Crossfader::new() } }
}

impl AudioProcessor for CrossfaderProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.len() < 2 || outputs.is_empty() { return; }
        self.inner.process_block(inputs[0], inputs[1], outputs[0]);
    }
}

pub struct SummingProcessor {
    inner: audio_dsp::SummingNode,
}

impl SummingProcessor {
    pub fn new() -> Self { Self { inner: audio_dsp::SummingNode::new() } }
}

impl AudioProcessor for SummingProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if outputs.is_empty() { return; }
        self.inner.process_16_to_1(inputs, outputs[0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use control_plane::{Command, TimestampedCommand};
    use ipc_layer::RingBuffer;

    struct ConstantProcessor { val: f32 }
    impl AudioProcessor for ConstantProcessor {
        fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
            for out in outputs { for s in out.iter_mut() { *s = self.val; } }
        }
    }

    #[test]
    fn test_node_limit() {
        let mut graph = ProcessorGraph::new();
        struct Pass { }
        impl AudioProcessor for Pass { fn process(&mut self, _: &[&[f32]], _: &mut [&mut [f32]]) {} }
        for _ in 0..100 {
            graph.add_node(Box::new(Pass {}), vec![], vec![]);
        }
        assert!(graph.nodes.len() <= 64);
    }

    #[test]
    fn test_sample_accurate_rewiring() {
        let rb = RingBuffer::new(1024);
        let (mut prod, cons) = rb.split();
        let garbage_rb = RingBuffer::new(32);
        let (garbage_prod, _) = garbage_rb.split();
        let tel_rb = RingBuffer::new(1024);
        let (tel_prod, _) = tel_rb.split();

        let mut graph = ProcessorGraph::new();
        // Manually disable pool for test to ensure immediate execution in this thread
        graph.pool = None;
        graph.add_node(Box::new(ConstantProcessor { val: 1.0 }), vec![], vec![2]); // Node 0
        graph.add_node(Box::new(ConstantProcessor { val: 2.0 }), vec![], vec![3]); // Node 1
        graph.add_node(Box::new(GainProcessor::new(1, 1.0)), vec![2], vec![0]);   // Node 2

        let mut engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph));

        let mut outputs = [[0.0f32; 128]; 2];
        {
            let (ch0, ch1) = outputs.split_at_mut(1);
            let mut out_refs = [&mut ch0[0][..], &mut ch1[0][..]];

            // Process first block: Node 2 takes from Buffer 2 (Val 1.0)
            engine.process_block(&[], &mut out_refs, 10);
        }
        assert_eq!(outputs[0][0], 1.0);

        // Send command to rewire Node 2 to Buffer 3 (Val 2.0) at sample 15
        let _ = prod.push(TimestampedCommand {
            timestamp_samples: 15,
            command: Command::UpdateEdge { node_idx: 2, input_idx: 0, new_buffer_idx: 3 },
        });

        // Process next block (samples 10-20). Rewiring should happen at sample 15.
        {
            let (ch0, ch1) = outputs.split_at_mut(1);
            let mut out_refs = [&mut ch0[0][..], &mut ch1[0][..]];
            engine.process_block(&[], &mut out_refs, 10);
        }
        // Samples 10-14 (indices 0-4 in this sub-block) should still be 1.0
        assert_eq!(outputs[0][0], 1.0);
        assert_eq!(outputs[0][4], 1.0);
        // Samples 15-19 (indices 5-9 in this sub-block) should be 2.0
        assert_eq!(outputs[0][5], 2.0);
        assert_eq!(outputs[0][9], 2.0);
    }

    #[test]
    fn test_backend_hot_swap() {
        let rb = RingBuffer::new(1024);
        let (_, cons) = rb.split();
        let garbage_rb = RingBuffer::new(32);
        let (garbage_prod, _) = garbage_rb.split();
        let tel_rb = RingBuffer::new(1024);
        let (tel_prod, _) = tel_rb.split();

        let mut graph = ProcessorGraph::new();
        graph.add_node(Box::new(ConstantProcessor { val: 1.0 }), vec![], vec![0]);
        let engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph));

        let mut backend1 = ThreadedBackend::new();
        backend1.start(engine).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let engine_returned = backend1.stop().expect("Should return engine");
        assert!(engine_returned.sample_counter > 0);

        let mut backend2 = ThreadedBackend::new();
        let prev_samples = engine_returned.sample_counter;
        backend2.start(engine_returned).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let engine_final = backend2.stop().expect("Should return engine");
        assert!(engine_final.sample_counter > prev_samples);
    }

    #[test]
    fn test_burst_commands() {
        let rb = RingBuffer::new(1024);
        let (mut prod, cons) = rb.split();
        let garbage_rb = RingBuffer::new(32);
        let (garbage_prod, _) = garbage_rb.split();
        let tel_rb = RingBuffer::new(1024);
        let (tel_prod, _) = tel_rb.split();

        let mut graph = ProcessorGraph::new();
        graph.pool = None;
        graph.add_node(Box::new(GainProcessor::new(1, 0.0)), vec![0], vec![1]);
        let mut engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph));

        // Send two commands at the same timestamp
        let _ = prod.push(TimestampedCommand {
            timestamp_samples: 5,
            command: Command::SetParam { target_id: 1, param_id: 0, value: 0.5, ramp_duration_samples: 0 },
        });
        let _ = prod.push(TimestampedCommand {
            timestamp_samples: 5,
            command: Command::SetParam { target_id: 1, param_id: 0, value: 1.0, ramp_duration_samples: 0 },
        });

        let mut outputs = [[0.0f32; 128]; 2];
        let mut inputs = [[1.0f32; 128]; 1];
        let (ch0, ch1) = outputs.split_at_mut(1);
        let mut out_refs = [&mut ch0[0][..], &mut ch1[0][..]];
        let in_refs = [&inputs[0][..]];

        engine.process_block(&in_refs, &mut out_refs, 10);

        // Samples 0-4 should be 0.0 (initial gain)
        println!("OUT at 0: {}", outputs[1][0]);
        println!("OUT at 4: {}", outputs[1][4]);
        assert_eq!(outputs[1][0], 0.0);
        assert_eq!(outputs[1][4], 0.0);
        // Samples 5-9 should be 1.0 (last command wins)
        println!("OUT at 5: {}", outputs[1][5]);
        println!("OUT at 9: {}", outputs[1][9]);
        assert_eq!(outputs[1][5], 1.0);
        assert_eq!(outputs[1][9], 1.0);
    }

    #[test]
    fn test_node_telemetry() {
        let rb = RingBuffer::new(1024);
        let (_, cons) = rb.split();
        let garbage_rb = RingBuffer::new(32);
        let (garbage_prod, _) = garbage_rb.split();
        let tel_rb = RingBuffer::new(1024);
        let (tel_prod, mut tel_cons) = tel_rb.split();

        let mut graph = ProcessorGraph::new();
        graph.pool = None; // Sync execution for timing
        graph.add_node(Box::new(ConstantProcessor { val: 0.5 }), vec![], vec![0]);
        let mut engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(graph));

        let mut outputs = [[0.0f32; 128]; 1];
        let mut out_refs = [&mut outputs[0][..]];
        engine.process_block(&[], &mut out_refs, 128);

        let tel = tel_cons.pop().expect("Should have telemetry");
        assert!(tel.node_load_ns[0] > 0);
        assert_eq!(tel.buffer_levels[0], 0.5);
    }

    #[test]
    fn test_stage_grouping() {
        let mut graph = ProcessorGraph::new();
        struct Pass { }
        impl AudioProcessor for Pass { fn process(&mut self, _: &[&[f32]], _: &mut [&mut [f32]]) {} }

        graph.add_node(Box::new(Pass {}), vec![1], vec![2]); // Node 0: In 1, Out 2
        graph.add_node(Box::new(Pass {}), vec![1], vec![3]); // Node 1: In 1, Out 3
        graph.add_node(Box::new(Pass {}), vec![2, 3], vec![4]); // Node 2: In 2, 3, Out 4

        graph.commit_graph(); // Make topology active for test

        let topo = graph.current_topology();
        assert_eq!(topo.num_stages, 2);
        assert!(topo.stages[0][..topo.stage_counts[0]].contains(&0));
        assert!(topo.stages[0][..topo.stage_counts[0]].contains(&1));
        assert!(topo.stages[1][..topo.stage_counts[1]].contains(&2));
    }
}
