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

#[derive(Clone, Copy)]
pub struct Job {
    pub node_ptr: *const ProcessorNode,
    pub num_samples: usize,
    pub buffers_ptr: *mut AudioBlock,
    pub x_buffers_ptr: *mut AudioBlock,
    pub input_indices: [usize; 16],
    pub output_indices: [usize; 16],
    pub input_count: usize,
    pub output_count: usize,
    pub node_idx: usize, // for telemetry
    pub telemetry_ptr: *const [AtomicU64; 64],
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

        for _ in 0..num_workers {
            let (prod, mut cons) = RingBuffer::<Job>::new(128).split();
            let running_worker = running.clone();
            let completion_worker = completion.clone();

            let handle = thread::spawn(move || {
                let _ = ipc_layer::set_rt_priority(85);
                let mut spins = 0;
                while running_worker.load(Ordering::Relaxed) {
                    if let Some(job) = cons.pop() {
                        spins = 0;
                        let node = unsafe { &*job.node_ptr };
                        let num_samples = job.num_samples;
                        let buffers_ptr = job.buffers_ptr;

                        let mut node_inputs_storage = [ &[][..]; 16 ];
                        let input_count = job.input_count.min(16);
                        for i in 0..input_count {
                            let p_idx = job.input_indices[i];
                            if p_idx >= 64 {
                                let x_idx = p_idx - 64;
                                unsafe { node_inputs_storage[i] = &(&(*job.x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                            } else {
                                unsafe { node_inputs_storage[i] = &(&(*buffers_ptr.add(p_idx.min(63))).data)[..num_samples]; }
                            }
                        }

                        let mut node_outputs_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|_| &mut [][..]);
                        let output_count = job.output_count.min(16);
                        for i in 0..output_count {
                            let p_idx = job.output_indices[i].min(63);
                            unsafe {
                                node_outputs_reconstructed[i] = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr(), num_samples);
                            }
                        }

                        #[cfg(target_arch = "x86_64")]
                        let start = unsafe { std::arch::x86_64::_rdtsc() };

                        let mut inner_context = crate::processors::ProcessContext {
                            pool: None,
                            transport: job.transport.as_ref(),
                            is_last_sub_block: job.is_last_sub_block
                        };
                        unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count], &mut inner_context); }

                        #[cfg(target_arch = "x86_64")]
                        {
                            let elapsed = unsafe { std::arch::x86_64::_rdtsc() } - start;
                            unsafe { (*job.telemetry_ptr)[job.node_idx].store(elapsed, Ordering::Relaxed); }
                        }

                        completion_worker.fetch_add(1, Ordering::SeqCst);
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

pub struct ProcessorGraph {
    pub(crate) nodes: Box<[ProcessorNode; 64]>,
    pub(crate) node_count: usize,
    pub(crate) buffers: Box<[AudioBlock; 64]>,
    pub(crate) _crossfade_buffers: [AudioBlock; 8],
    pub(crate) _old_path_buffers: Box<[AudioBlock; 64]>,
    pub(crate) topologies: Box<[GraphTopology; 2]>,
    pub(crate) active_topo_idx: Arc<AtomicUsize>,
    pub(crate) needs_commit: bool,

    pub(crate) _stage_scratch_assigned: [bool; 64],
    pub(crate) _stage_scratch_in_degree: [usize; 64],

    pub(crate) node_times_cycles: Arc<[AtomicU64; 64]>,
    pub(crate) peak_levels: Arc<[AtomicU32; 64]>,
    pub(crate) _telemetry_offset: AtomicUsize,
    pub(crate) garbage_producer: Option<ipc_layer::Producer<Box<dyn AudioProcessor>>>,
}

impl ProcessorGraph {
    pub fn new() -> Self {
        let buffers = Box::new([AudioBlock { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; 64]);
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

        let nodes = Box::new(std::array::from_fn(|_| ProcessorNode {
            processor: std::cell::UnsafeCell::new(Box::new(DummyProcessor) as Box<dyn AudioProcessor>),
        }));

        Self {
            nodes,
            node_count: 0,
            buffers,
            _crossfade_buffers: [AudioBlock { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; 8],
            _old_path_buffers: Box::new([AudioBlock { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0 }; 64]),
            topologies: Box::new([topo; 2]),
            active_topo_idx: Arc::new(AtomicUsize::new(0)),
            needs_commit: false,
            _stage_scratch_assigned: [false; 64],
            _stage_scratch_in_degree: [0; 64],
            node_times_cycles: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
            peak_levels: Arc::new(std::array::from_fn(|_| AtomicU32::new(0))),
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

        let mut in_degree = [0usize; 64];
        let mut adj = [[0usize; 64]; 64];
        let mut adj_count = [0usize; 64];

        // 1. Build adjacency list and in-degrees efficiently
        let mut v_to_producers = [[0usize; 64]; 64];
        let mut v_producer_counts = [0usize; 64];
        for j in 0..n {
            let routing_j = &topo.routing[j];
            for k in 0..routing_j.output_count {
                let v_out = routing_j.output_indices[k];
                if v_out < 64 {
                    v_to_producers[v_out][v_producer_counts[v_out]] = j;
                    v_producer_counts[v_out] += 1;
                }
            }
        }

        for i in 0..n {
            let routing_i = &topo.routing[i];
            for l in 0..routing_i.input_count {
                let v_in = routing_i.input_indices[l];
                if v_in < 64 {
                    for m in 0..v_producer_counts[v_in] {
                        let j = v_to_producers[v_in][m];
                        if i == j { continue; }
                        let mut exists = false;
                        for x in 0..adj_count[j] {
                            if adj[j][x] == i {
                                exists = true;
                                break;
                            }
                        }
                        if !exists {
                            adj[j][adj_count[j]] = i;
                            adj_count[j] += 1;
                            in_degree[i] += 1;
                        }
                    }
                }
            }
        }

        // 2. Kahn's algorithm with Write-After-Write (WAW) tracking
        let mut processed_count = 0;
        let mut is_processed = [false; 64];
        topo.num_stages = 0;

        while processed_count < n {
            let mut stage_nodes = [0usize; 64];
            let mut stage_count = 0;
            let mut physical_buffers_in_stage = [false; 64];

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

            for i in 0..stage_count {
                let node_idx = stage_nodes[i];
                topo.stages[topo.num_stages][i] = node_idx;
                is_processed[node_idx] = true;
                processed_count += 1;
            }
            topo.stage_counts[topo.num_stages] = stage_count;
            topo.num_stages += 1;

            for i in 0..stage_count {
                let node_idx = stage_nodes[i];
                for m in 0..adj_count[node_idx] {
                    let dependent = adj[node_idx][m];
                    in_degree[dependent] -= 1;
                }
            }
        }
    }

    pub fn commit_graph(&mut self) {
        let active = self.active_topo_idx.load(Ordering::Acquire);
        let inactive = (active + 1) % 2;
        self.active_topo_idx.store(inactive, Ordering::Release);
        self.needs_commit = false;
    }

    pub fn add_node(&mut self, processor: Box<dyn AudioProcessor>, inputs: Vec<usize>, outputs: Vec<usize>) {
        if self.node_count >= 64 { return; }
        let idx = self.node_count;
        unsafe { *self.nodes[idx].processor.get() = processor; }
        self.node_count += 1;

        let topo = self.inactive_topology_mut();
        topo.routing[idx].input_count = inputs.len().min(16);
        for i in 0..topo.routing[idx].input_count { topo.routing[idx].input_indices[i] = inputs[i]; }
        topo.routing[idx].output_count = outputs.len().min(16);
        for i in 0..topo.routing[idx].output_count { topo.routing[idx].output_indices[i] = outputs[i]; }
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
        let mut block_x_map = [[0u8; 16]; 64];

        let crossfades_mut_ptr = &self.topologies[active_idx].crossfades as *const [Option<CrossfadeState>; 8] as *mut [Option<CrossfadeState>; 8];

        for i in 0..8 {
            let x_state_opt = unsafe { &mut (*crossfades_mut_ptr)[i] };
            if let Some(state) = x_state_opt {
                let x_buf_idx = i;
                let old_data = &self._old_path_buffers[state.old_buffer_idx].data[..num_samples];
                let new_data = &self.buffers[state.new_buffer_idx].data[..num_samples];
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
                pool.completion.store(0, Ordering::Release);
                let num_nodes = stage.len();
                for (i, &n_idx) in stage.iter().enumerate() {
                    let worker_idx = i % pool.worker_producers.len();
                    let routing = &topo.routing[n_idx];
                    let mut resolved_inputs = [0usize; 16];
                    let mut resolved_outputs = [0usize; 16];

                    for j in 0..routing.input_count.min(16) {
                        let v_idx = routing.input_indices[j].min(63);
                        let mut p_idx = topo.virtual_to_physical[v_idx];

                        // Apply crossfade override
                        let p_override = block_x_map[n_idx][j];
                        if p_override != 0 {
                            p_idx = p_override as usize;
                        }
                        resolved_inputs[j] = p_idx;
                    }

                    for j in 0..routing.output_count.min(16) {
                        let v_idx = routing.output_indices[j].min(63);
                        resolved_outputs[j] = topo.virtual_to_physical[v_idx];
                    }

                    let _ = pool.worker_producers[worker_idx].push(Job {
                        node_ptr: &self.nodes[n_idx] as *const _,
                        num_samples,
                        buffers_ptr,
                        x_buffers_ptr,
                        input_indices: resolved_inputs,
                        output_indices: resolved_outputs,
                        input_count: routing.input_count,
                        output_count: routing.output_count,
                        node_idx: n_idx,
                        telemetry_ptr: Arc::as_ptr(&self.node_times_cycles) as *const _,
                        transport: context.transport.copied(),
                        is_last_sub_block: context.is_last_sub_block,
                    });
                }

                let mut spins = 0;
                while pool.completion.load(Ordering::Acquire) < num_nodes {
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
                    let mut node_inputs_storage = [ &[][..]; 16 ];
                    let input_count = routing.input_count.min(16);
                    for i in 0..input_count {
                        let v_idx = routing.input_indices[i].min(63);
                        let mut p_idx = topo.virtual_to_physical[v_idx];
                        let p_override = block_x_map[n_idx][i];
                        if p_override != 0 {
                            p_idx = p_override as usize;
                        }

                        if p_idx >= 64 {
                            let x_idx = p_idx - 64;
                            unsafe { node_inputs_storage[i] = &(&(*x_buffers_ptr.add(x_idx)).data)[..num_samples]; }
                        } else {
                            unsafe { node_inputs_storage[i] = &(&(*buffers_ptr.add(p_idx.min(63))).data)[..num_samples]; }
                        }
                    }
                    let mut node_outputs_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|_| &mut [][..]);
                    let output_count = routing.output_count.min(16);
                    for i in 0..output_count {
                        let v_idx = routing.output_indices[i].min(63);
                        let p_idx = topo.virtual_to_physical[v_idx].min(63);
                        unsafe {
                            node_outputs_reconstructed[i] = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr(), num_samples);
                        }
                    }

                    #[cfg(target_arch = "x86_64")]
                    let start = unsafe { std::arch::x86_64::_rdtsc() };

                    let mut inner_context = crate::processors::ProcessContext { pool: None, transport: context.transport, is_last_sub_block: context.is_last_sub_block };
                    unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count], &mut inner_context); }

                    #[cfg(target_arch = "x86_64")]
                    {
                        let elapsed = unsafe { std::arch::x86_64::_rdtsc() } - start;
                        self.node_times_cycles[n_idx].store(elapsed, Ordering::Relaxed);
                    }
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

        // Before finishing process, copy current buffers to old_path_buffers for crossfading in next block
        // ONLY if this is the last sub-block of the engine cycle to preserve "previous block" state
        if is_last_sub_block {
            for i in 0..64 {
                self._old_path_buffers[i].data[..num_samples].copy_from_slice(&self.buffers[i].data[..num_samples]);
            }
        }

        #[cfg(target_arch = "x86_64")]
        let has_avx2 = is_x86_feature_detected!("avx2");

        for n_idx in 0..topo.node_count.min(64) {
            let routing = &topo.routing[n_idx];
            let mut peak = 0.0f32;

            for o_idx in 0..routing.output_count {
                let v_out = routing.output_indices[o_idx].min(63);
                let p_idx = topo.virtual_to_physical[v_out].min(63);
                let data = &self.buffers[p_idx].data[..num_samples];

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
                        for &val in &res { if val > peak { peak = val; } }
                        while j < num_samples {
                            let abs = data[j].abs();
                            if abs > peak { peak = abs; }
                            j += 1;
                        }
                    }
                } else {
                    for &sample in data {
                        let abs = sample.abs();
                        if abs > peak { peak = abs; }
                    }
                }
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                for &sample in data {
                    let abs = sample.abs();
                    if abs > peak { peak = abs; }
                }
            }

                self.peak_levels[n_idx].store(peak.to_bits(), Ordering::Relaxed);
            }
        }
    }
    fn setup(&mut self, config: crate::AudioConfig) {
        for node in self.nodes.iter() {
            unsafe { (*node.processor.get()).setup(config); }
        }
    }

    fn apply_topology_command(&mut self, command: &control_plane::TopologyCommand) {
        match command {
            control_plane::TopologyCommand::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let n_idx = *node_idx as usize;
                let i_idx = *input_idx as usize;
                if n_idx < self.node_count && i_idx < 16 {
                    let topo = self.inactive_topology_mut();
                    if i_idx < topo.routing[n_idx].input_count {
                        topo.routing[n_idx].input_indices[i_idx] = (*new_buffer_idx as usize).min(63);
                    }
                }
            }
            control_plane::TopologyCommand::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let n_idx = *node_idx as usize;
                let o_idx = *output_idx as usize;
                if n_idx < self.node_count && o_idx < 16 {
                    let topo = self.inactive_topology_mut();
                    if o_idx < topo.routing[n_idx].output_count {
                        topo.routing[n_idx].output_indices[o_idx] = (*new_buffer_idx as usize).min(63);
                    }
                }
            }
            control_plane::TopologyCommand::SwapProcessor { node_idx, processor_type_id } => {
                let n_idx = *node_idx as usize;
                if n_idx < self.node_count {
                    let node = &self.nodes[n_idx];
                    let mut new_proc: Box<dyn AudioProcessor> = match processor_type_id {
                        1 => Box::new(crate::processors::standard::BiquadProcessor::new(0, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })),
                        2 => Box::new(crate::processors::standard::GainProcessor::new(0, 1.0)),
                        20 => Box::new(crate::processors::standard::CrossfaderProcessor::new()),
                        _ => return,
                    };

                    if let Some(ref prod) = self.garbage_producer {
                        new_proc.set_garbage_producer(prod.clone());
                    }

                    let old_proc = unsafe { std::ptr::replace(node.processor.get(), new_proc) };
                    if let Some(ref mut prod) = self.garbage_producer {
                        if let Err(leaked) = prod.push(old_proc) {
                            let _ = Box::into_raw(leaked);
                        }
                    }
                }
            }
            control_plane::TopologyCommand::AddNode { processor_type_id, node_idx } => {
                let id = *node_idx as u64;
                let processor: Box<dyn AudioProcessor> = match processor_type_id {
                    1 => Box::new(crate::processors::standard::BiquadProcessor::new(id, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })),
                    2 => Box::new(crate::processors::standard::GainProcessor::new(id, 1.0)),
                    3 => Box::new(crate::processors::standard::SimdBiquadProcessor::new(id, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })),
                    4 => Box::new(crate::processors::complex::WavetableProcessor::new(44100.0)),
                    5 => Box::new(crate::processors::complex::SpectralProcessor::new(512)),
                    10 => Box::new(crate::processors::complex::ModulationProcessor::new(0, 0, 1.0, 0.0)),
                    20 => Box::new(crate::processors::standard::CrossfaderProcessor::new()),
                    30 => Box::new(crate::processors::standard::SummingProcessor::new()),
                    40 => Box::new(crate::processors::complex::SequencerProcessor::new(44100.0, 120.0)),
                    _ => Box::new(crate::processors::standard::GainProcessor::new(0, 0.0)),
                };
                if self.node_count < 64 {
                    let idx = self.node_count;
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
    fn collect_telemetry(&self, node_times: &mut [u64; 64], peak_levels: &mut [f32; 64]) {
        for i in 0..64 {
            node_times[i] = self.node_times_cycles[i].load(Ordering::Relaxed);
            peak_levels[i] = f32::from_bits(self.peak_levels[i].load(Ordering::Relaxed));
        }
    }
}
