use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool, AtomicU32, AtomicU64};
use std::thread;
use ipc_layer::{AudioBlock, RingBuffer, Producer, EventFd};
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
    fn process(&mut self, _in: &[&[f32]], _out: &mut [&mut [f32]]) {}
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
    pub topo_ptr: *const GraphTopology,
    pub node_idx: usize, // for telemetry
    pub telemetry_ptr: *const [AtomicU64; 64],
}

unsafe impl Send for Job {}

pub struct TaskPool {
    workers: Vec<thread::JoinHandle<()>>,
    pub(crate) worker_producers: Vec<Producer<Job>>,
    pub(crate) completion: Arc<AtomicUsize>,
    pub(crate) running: Arc<AtomicBool>,
    pub(crate) worker_efds: Vec<EventFd>,
    pub(crate) completion_efd: EventFd,
}

impl TaskPool {
    pub fn new(num_workers: usize) -> Self {
        let mut workers = Vec::new();
        let mut worker_producers = Vec::new();
        let mut worker_efds = Vec::new();
        let completion = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicBool::new(true));
        let completion_efd = EventFd::create().unwrap();

        for _ in 0..num_workers {
            let (prod, mut cons) = RingBuffer::<Job>::new(128).split();
            let running_worker = running.clone();
            let completion_worker = completion.clone();
            let efd = EventFd::create().unwrap();
            let efd_worker = EventFd::from_raw(efd.fd());
            let comp_efd_worker = EventFd::from_raw(completion_efd.fd());

            let handle = thread::spawn(move || {
                while running_worker.load(Ordering::Relaxed) {
                    if let Some(job) = cons.pop() {
                        let node = unsafe { &*job.node_ptr };
                        let num_samples = job.num_samples;
                        let buffers_ptr = job.buffers_ptr;
                        let topo = unsafe { &*job.topo_ptr };
                        let routing = &topo.routing[job.node_idx];

                        let mut node_inputs_storage = [ &[][..]; 16 ];
                        let input_count = routing.input_count.min(16);
                        for i in 0..input_count {
                            let v_idx = routing.input_indices[i].min(63);
                            let p_idx = topo.virtual_to_physical[v_idx].min(63);
                            unsafe { node_inputs_storage[i] = &(&(*buffers_ptr.add(p_idx)).data)[..num_samples.min(128)]; }
                        }

                        let mut node_outputs_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|_| &mut [][..]);
                        let output_count = routing.output_count.min(16);
                        for i in 0..output_count {
                            let v_idx = routing.output_indices[i].min(63);
                            let p_idx = topo.virtual_to_physical[v_idx].min(63);
                            unsafe {
                                node_outputs_reconstructed[i] = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr(), num_samples.min(128));
                            }
                        }

                        #[cfg(target_arch = "x86_64")]
                        let start = unsafe { std::arch::x86_64::_rdtsc() };

                        unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count]); }

                        #[cfg(target_arch = "x86_64")]
                        {
                            let elapsed = unsafe { std::arch::x86_64::_rdtsc() } - start;
                            unsafe { (*job.telemetry_ptr)[job.node_idx].store(elapsed, Ordering::Relaxed); }
                        }

                        completion_worker.fetch_add(1, Ordering::SeqCst);
                        comp_efd_worker.notify();
                    } else {
                        efd_worker.wait();
                    }
                }
            });

            workers.push(handle);
            worker_producers.push(prod);
            worker_efds.push(efd);
        }

        Self { workers, worker_producers, completion, running, worker_efds, completion_efd }
    }
}

impl Drop for TaskPool {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        for efd in &self.worker_efds { efd.notify(); }
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
    pub(crate) topologies: Box<[GraphTopology; 2]>,
    pub(crate) active_topo_idx: Arc<AtomicUsize>,
    pub pool: Option<TaskPool>,
    pub(crate) needs_commit: bool,

    pub(crate) _stage_scratch_assigned: [bool; 64],
    pub(crate) _stage_scratch_in_degree: [usize; 64],

    pub(crate) node_times_cycles: Arc<[AtomicU64; 64]>,
    pub(crate) peak_levels: Arc<[AtomicU32; 64]>,
    pub(crate) _telemetry_offset: AtomicUsize,
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

        let nodes = Box::new(std::array::from_fn(|_| ProcessorNode {
            processor: std::cell::UnsafeCell::new(Box::new(DummyProcessor) as Box<dyn AudioProcessor>),
        }));

        Self {
            nodes,
            node_count: 0,
            buffers,
            _crossfade_buffers: [AudioBlock { data: [0.0f32; 128] }; 8],
            topologies: Box::new([topo; 2]),
            active_topo_idx: Arc::new(AtomicUsize::new(0)),
            needs_commit: false,
            _stage_scratch_assigned: [false; 64],
            _stage_scratch_in_degree: [0; 64],
            pool: Some(TaskPool::new(4)),
            node_times_cycles: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
            peak_levels: Arc::new(std::array::from_fn(|_| AtomicU32::new(0))),
            _telemetry_offset: AtomicUsize::new(0),
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
    }
}

impl AudioProcessor for ProcessorGraph {
    fn process(&mut self, _external_inputs: &[&[f32]], external_outputs: &mut [&mut [f32]]) {
        let num_samples = if !external_outputs.is_empty() { external_outputs[0].len() } else { 0 };
        if num_samples == 0 { return; }

        if self.needs_commit {
            self.commit_graph();
            self.needs_commit = false;
        }

        let active_idx = self.active_topo_idx.load(Ordering::Acquire);
        let topo = &self.topologies[active_idx];
        let topo_ptr = topo as *const GraphTopology;
        let buffers_ptr = self.buffers.as_mut_ptr();

        for s_idx in 0..topo.num_stages {
            let stage = &topo.stages[s_idx][..topo.stage_counts[s_idx]];

            if let Some(pool) = &mut self.pool {
                pool.completion.store(0, Ordering::Release);
                let num_nodes = stage.len();
                for (i, &n_idx) in stage.iter().enumerate() {
                    let worker_idx = i % pool.worker_producers.len();
                    let _ = pool.worker_producers[worker_idx].push(Job {
                        node_ptr: &self.nodes[n_idx] as *const _,
                        num_samples,
                        buffers_ptr,
                        topo_ptr,
                        node_idx: n_idx,
                        telemetry_ptr: Arc::as_ptr(&self.node_times_cycles) as *const _,
                    });
                    pool.worker_efds[worker_idx].notify();
                }

                let mut spins = 0;
                while pool.completion.load(Ordering::Acquire) < num_nodes {
                    if spins < 1000 {
                        std::hint::spin_loop();
                        spins += 1;
                    } else {
                        let _ = pool.completion_efd.wait();
                    }
                }
            } else {
                for &n_idx in stage {
                    let node = &self.nodes[n_idx];
                    let routing = &topo.routing[n_idx];
                    let mut node_inputs_storage = [ &[][..]; 16 ];
                    let input_count = routing.input_count.min(16);
                    for i in 0..input_count {
                        let v_idx = routing.input_indices[i].min(63);
                        let p_idx = topo.virtual_to_physical[v_idx].min(63);
                        unsafe { node_inputs_storage[i] = &(&(*buffers_ptr.add(p_idx)).data)[..num_samples.min(128)]; }
                    }
                    let mut node_outputs_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|_| &mut [][..]);
                    let output_count = routing.output_count.min(16);
                    for i in 0..output_count {
                        let v_idx = routing.output_indices[i].min(63);
                        let p_idx = topo.virtual_to_physical[v_idx].min(63);
                        unsafe {
                            node_outputs_reconstructed[i] = std::slice::from_raw_parts_mut((*buffers_ptr.add(p_idx)).data.as_mut_ptr(), num_samples.min(128));
                        }
                    }

                    #[cfg(target_arch = "x86_64")]
                    let start = unsafe { std::arch::x86_64::_rdtsc() };

                    unsafe { (*node.processor.get()).process(&node_inputs_storage[..input_count], &mut node_outputs_reconstructed[..output_count]); }

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

        let num_nodes_to_process = self.node_count.min(64);
        for i in 0..num_nodes_to_process {
            let mut peak = 0.0f32;
            // The buffers are indexed by physical index in topo.
            // But node telemetry is indexed by node index.
            // Let's use the active topology to find the physical buffer for each node.
            let p_idx = topo.virtual_to_physical[i];
            for sample in &self.buffers[p_idx].data[..num_samples] {
                let abs = sample.abs();
                if abs > peak { peak = abs; }
            }
            self.peak_levels[i].store(peak.to_bits(), Ordering::Relaxed);
        }
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        match command {
            control_plane::Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let n_idx = *node_idx as usize;
                let i_idx = *input_idx as usize;
                if n_idx < self.node_count && i_idx < 16 {
                    let topo = self.inactive_topology_mut();
                    if i_idx < topo.routing[n_idx].input_count {
                        topo.routing[n_idx].input_indices[i_idx] = (*new_buffer_idx as usize).min(63);
                        self.calculate_stages();
                        self.needs_commit = true;
                    }
                }
            }
            control_plane::Command::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let n_idx = *node_idx as usize;
                let o_idx = *output_idx as usize;
                if n_idx < self.node_count && o_idx < 16 {
                    let topo = self.inactive_topology_mut();
                    if o_idx < topo.routing[n_idx].output_count {
                        topo.routing[n_idx].output_indices[o_idx] = (*new_buffer_idx as usize).min(63);
                        self.calculate_stages();
                        self.needs_commit = true;
                    }
                }
            }
            control_plane::Command::SwapProcessor { node_idx, processor_type_id } => {
                let n_idx = *node_idx as usize;
                if n_idx < self.node_count {
                    let node = &self.nodes[n_idx];
                    match processor_type_id {
                        1 => { unsafe { *node.processor.get() = Box::new(crate::processors::standard::BiquadProcessor::new(0, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })); } }
                        2 => { unsafe { *node.processor.get() = Box::new(crate::processors::standard::GainProcessor::new(0, 1.0)); } }
                        20 => { unsafe { *node.processor.get() = Box::new(crate::processors::standard::CrossfaderProcessor::new()); } }
                        _ => {}
                    }
                }
            }
            control_plane::Command::AddNode { processor_type_id, node_idx } => {
                let id = *node_idx as u64;
                let processor: Box<dyn AudioProcessor> = match processor_type_id {
                    1 => Box::new(crate::processors::standard::BiquadProcessor::new(id, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })),
                    2 => Box::new(crate::processors::standard::GainProcessor::new(id, 1.0)),
                    3 => Box::new(crate::processors::standard::SimdBiquadProcessor::new(audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })),
                    4 => Box::new(crate::processors::complex::WavetableProcessor::new(44100.0)),
                    5 => Box::new(crate::processors::complex::SpectralProcessor::new(512)),
                    10 => Box::new(crate::processors::complex::ModulationProcessor::new(0, 0, 1.0, 0.0)),
                    20 => Box::new(crate::processors::standard::CrossfaderProcessor::new()),
                    30 => Box::new(crate::processors::standard::SummingProcessor::new()),
                    40 => Box::new(crate::processors::complex::SequencerProcessor::new(44100.0, 120.0)),
                    _ => Box::new(crate::processors::standard::GainProcessor::new(0, 0.0)),
                };
                self.add_node(processor, vec![], vec![]);
            }
            _ => {
                for node in self.nodes.iter() { unsafe { (*node.processor.get()).apply_command(command); } }
            }
        }
    }
    fn collect_telemetry(&self, node_times: &mut [u64; 64], peak_levels: &mut [f32; 64]) {
        for i in 0..64 {
            node_times[i] = self.node_times_cycles[i].load(Ordering::Relaxed);
            peak_levels[i] = f32::from_bits(self.peak_levels[i].load(Ordering::Relaxed));
        }
    }
}
