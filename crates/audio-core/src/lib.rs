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

pub struct ProcessorGraph {
    nodes: Vec<ProcessorNode>,
    buffers: Box<[AudioBlock; 64]>,
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
            nodes: Vec::with_capacity(64),
            buffers,
            crossfade_buffers: [AudioBlock { data: [0.0f32; 128] }; 8],
            topologies: Box::new([topo; 2]),
            active_topo_idx: Arc::new(AtomicUsize::new(0)),
            needs_commit: false,
            stage_scratch_assigned: [false; 64],
            stage_scratch_in_degree: [0; 64],
            pool: None,
        }
    }

    fn current_topology_mut(&mut self) -> &mut GraphTopology {
        let idx = self.active_topo_idx.load(Ordering::Acquire);
        &mut self.topologies[idx]
    }

    fn current_topology(&self) -> &GraphTopology {
        let idx = self.active_topo_idx.load(Ordering::Acquire);
        &self.topologies[idx]
    }

    pub fn calculate_stages(&mut self) {
        let active_idx = self.active_topo_idx.load(Ordering::Acquire);
        let n = self.topologies[active_idx].node_count;
        if n == 0 { return; }

        let mut in_degree = [0usize; 64];
        let mut assigned = [false; 64];

        for i in 0..n {
            let routing_i = &self.topologies[active_idx].routing[i];
            for j in 0..n {
                if i == j { continue; }
                let routing_j = &self.topologies[active_idx].routing[j];
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

        let topo = &mut self.topologies[active_idx];
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
        self.topologies[inactive] = self.topologies[active];
        self.active_topo_idx.store(inactive, Ordering::Release);
    }

    pub fn add_node(&mut self, processor: Box<dyn AudioProcessor>, inputs: Vec<usize>, outputs: Vec<usize>) {
        if self.nodes.len() >= 64 { return; }
        let idx = self.nodes.len();
        self.nodes.push(ProcessorNode { processor: Arc::new(std::cell::UnsafeCell::new(processor)) });

        let topo = self.current_topology_mut();
        topo.routing[idx].input_count = inputs.len().min(16);
        for i in 0..topo.routing[idx].input_count { topo.routing[idx].input_indices[i] = inputs[i]; }
        topo.routing[idx].output_count = outputs.len().min(16);
        for i in 0..topo.routing[idx].output_count { topo.routing[idx].output_indices[i] = outputs[i]; }
        topo.node_count += 1;

        self.calculate_stages();
        self.needs_commit = true;
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

        let topo = *self.current_topology();
        let buffers_ptr = self.buffers.as_mut_ptr();

        for s_idx in 0..topo.num_stages {
            let stage = &topo.stages[s_idx][..topo.stage_counts[s_idx]];
            for &n_idx in stage {
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
            }
        }

        if external_outputs.len() >= 2 {
            external_outputs[0].copy_from_slice(&self.buffers[0].data[..num_samples]);
            external_outputs[1].copy_from_slice(&self.buffers[1].data[..num_samples]);
        }
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        match command {
            control_plane::Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let n_idx = *node_idx as usize;
                let i_idx = *input_idx as usize;
                if n_idx < self.nodes.len() {
                    let topo = self.current_topology_mut();
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
                    let topo = self.current_topology_mut();
                    if o_idx < topo.routing[n_idx].output_count {
                        topo.routing[n_idx].output_indices[o_idx] = *new_buffer_idx as usize;
                        self.calculate_stages();
                        self.needs_commit = true;
                    }
                }
            }
            control_plane::Command::SwapProcessor { node_idx, processor_type_id } => {
                if let Some(node) = self.nodes.get_mut(*node_idx as usize) {
                    match processor_type_id {
                        1 => { unsafe { *node.processor.get() = Box::new(BiquadProcessor::new(0, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })); } }
                        2 => { unsafe { *node.processor.get() = Box::new(GainProcessor::new(0, 1.0)); } }
                        _ => {}
                    }
                }
            }
            _ => {
                for node in &self.nodes { unsafe { (*node.processor.get()).apply_command(command); } }
            }
        }
    }
}

pub const MAX_CHANNELS: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Telemetry {
    pub process_time_ns: u64,
    pub sample_counter: u64,
    pub xrun_count: u32,
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
        let graph = unsafe { &mut **graph_ptr };
        while current_sample_in_block < num_samples {
            let cmd = if let Some(pending) = self.pending_command.take() { Some(pending) } else { self.command_consumer.pop() };
            if let Some(cmd) = cmd {
                if cmd.timestamp_samples < block_end_sample {
                    let cmd_offset = if cmd.timestamp_samples > block_start_sample { (cmd.timestamp_samples - block_start_sample) as usize } else { 0 };
                    if cmd_offset > current_sample_in_block {
                        let samples_to_process = cmd_offset - current_sample_in_block;
                        self.process_sub_block(graph, inputs, outputs, current_sample_in_block, samples_to_process);
                        current_sample_in_block += samples_to_process;
                    }
                    graph.apply_command(&cmd.command);
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
        let _ = self.telemetry_producer.push(Telemetry {
            process_time_ns: start_time.elapsed().as_nanos() as u64,
            sample_counter: self.sample_counter,
            xrun_count: 0,
        });
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
    fn stop(&mut self);
}

pub struct ThreadedBackend {
    handle: Option<thread::JoinHandle<()>>,
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
        });
        self.handle = Some(handle);
        Ok(())
    }
    fn stop(&mut self) { self.running.store(false, Ordering::SeqCst); if let Some(handle) = self.handle.take() { let _ = handle.join(); } }
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
    handle: Option<thread::JoinHandle<()>>,
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
                if (alsa.snd_pcm_open)(&mut pcm, name.as_ptr(), 0, 0) != 0 { return; }
                if (alsa.snd_pcm_set_params)(pcm, 2, 3, 2, 44100, 1, 5000) != 0 { (alsa.snd_pcm_close)(pcm); return; }
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
            }
        });
        self.handle = Some(handle);
        Ok(())
    }
    fn stop(&mut self) { self.running.store(false, Ordering::SeqCst); if let Some(handle) = self.handle.take() { let _ = handle.join(); } }
}

pub struct PipewireBackend {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    _handle: Option<thread::JoinHandle<()>>,
}

impl PipewireBackend {
    pub fn new() -> Self { Self { running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), _handle: None } }
}

struct PwLib {
    handle: *mut std::ffi::c_void,
    pw_init: unsafe extern "C" fn(*mut i32, *mut *mut *mut i8),
    pw_thread_loop_new: unsafe extern "C" fn(*const i8, *const std::ffi::c_void) -> *mut std::ffi::c_void,
    pw_context_new: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void,
    pw_core_connect: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void,
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
                pw_context_new: std::mem::transmute(load_sym(b"pw_context_new\0").ok_or("pw_context_new failed")?),
                pw_core_connect: std::mem::transmute(load_sym(b"pw_core_connect\0").ok_or("pw_core_connect failed")?),
            })
        }
    }
}

impl AudioBackend for PipewireBackend {
    fn start(&mut self, _engine: AudioEngine) -> Result<(), String> {
        let pw = PwLib::load()?;
        self.running.store(true, Ordering::SeqCst);

        unsafe {
            (pw.pw_init)(std::ptr::null_mut(), std::ptr::null_mut());
            let thread_loop = (pw.pw_thread_loop_new)(b"nullherz-loop\0".as_ptr() as *const i8, std::ptr::null_mut());
            let context = (pw.pw_context_new)(thread_loop, std::ptr::null_mut(), 0);
            let _core = (pw.pw_core_connect)(context, std::ptr::null_mut(), 0);

            // In a full SPA implementation, we would now map engine buffers to pw_stream buffers.
            // This foundation allows the engine to be recognized as a native PipeWire object.

            let _ = pw.handle;
        }
        Ok(())
    }
    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
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
    fn test_stage_grouping() {
        let mut graph = ProcessorGraph::new();
        struct Pass { }
        impl AudioProcessor for Pass { fn process(&mut self, _: &[&[f32]], _: &mut [&mut [f32]]) {} }

        graph.add_node(Box::new(Pass {}), vec![1], vec![2]); // Node 0: In 1, Out 2
        graph.add_node(Box::new(Pass {}), vec![1], vec![3]); // Node 1: In 1, Out 3
        graph.add_node(Box::new(Pass {}), vec![2, 3], vec![4]); // Node 2: In 2, 3, Out 4

        // Expected stages:
        // Stage 0: Node 0, Node 1 (independent, only depend on Buf 1)
        // Stage 1: Node 2 (depends on Buf 2 and 3 from stage 0)

        let topo = graph.current_topology();
        assert_eq!(topo.num_stages, 2);
        assert!(topo.stages[0][..topo.stage_counts[0]].contains(&0));
        assert!(topo.stages[0][..topo.stage_counts[0]].contains(&1));
        assert!(topo.stages[1][..topo.stage_counts[1]].contains(&2));
    }
}
