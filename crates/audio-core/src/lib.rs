use audio_dsp::Filter;
use control_plane::TimestampedCommand;
use ipc_layer::{Consumer, Producer, AudioBlock, ShmRingBuffer, ShmSignal, EventFd};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::thread;
use std::time::{Duration, Instant};

pub trait AudioProcessor: Send {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]);
    fn apply_command(&mut self, _command: &control_plane::Command) {}
}

pub struct CrossfadeState {
    pub node_idx: usize,
    pub input_idx: usize,
    pub old_buffer_idx: usize,
    pub new_buffer_idx: usize,
    pub remaining_samples: u32,
    pub total_samples: u32,
}

pub struct ProcessorNode {
    pub processor: Box<dyn AudioProcessor>,
    pub input_indices: Vec<usize>,
    pub output_indices: Vec<usize>,
}

pub struct ProcessorGraph {
    nodes: Vec<ProcessorNode>,
    buffers: Vec<AudioBlock>,
    crossfades: Vec<CrossfadeState>,
    crossfade_buffers: [AudioBlock; 8],
    virtual_to_physical: [usize; 64],
}

impl ProcessorGraph {
    pub fn serialize_to_json(&self) -> String {
        let mut json = String::from("{\"nodes\": [");
        for (i, node) in self.nodes.iter().enumerate() {
            if i > 0 { json.push_str(", "); }
            json.push_str(&format!("{{\"inputs\": {:?}, \"outputs\": {:?}}}", node.input_indices, node.output_indices));
        }
        json.push_str("]}");
        json
    }

    pub fn update_scratchpad(&mut self) {
        // Calculate buffer lifetimes
        let mut first_use = [usize::MAX; 64];
        let mut last_use = [0usize; 64];

        for (n_idx, node) in self.nodes.iter().enumerate() {
            for &idx in &node.input_indices {
                if idx < 64 {
                    first_use[idx] = first_use[idx].min(n_idx);
                    last_use[idx] = last_use[idx].max(n_idx);
                }
            }
            for &idx in &node.output_indices {
                if idx < 64 {
                    first_use[idx] = first_use[idx].min(n_idx);
                    last_use[idx] = last_use[idx].max(n_idx);
                }
            }
        }

        // Include crossfade old buffers in lifetime calculation
        // They must stay alive for the entire block processing if they are being used.
        for xf in &self.crossfades {
            if xf.old_buffer_idx < 64 {
                first_use[xf.old_buffer_idx] = 0;
                last_use[xf.old_buffer_idx] = self.nodes.len();
            }
        }

        // Greedy allocation of physical buffers
        let mut physical_last_use = [0usize; 64];
        for i in 0..64 {
            if first_use[i] == usize::MAX { continue; }

            // Try to find an existing physical buffer that is free
            let mut allocated = false;
            for p in 0..64 {
                if physical_last_use[p] < first_use[i] {
                    self.virtual_to_physical[i] = p;
                    physical_last_use[p] = last_use[i];
                    allocated = true;
                    break;
                }
            }
            if !allocated {
                // This shouldn't happen with 64 physical buffers unless the graph is very wide
                self.virtual_to_physical[i] = i;
            }
        }
    }

    pub fn detect_cycle(&self) -> bool {
        let n = self.nodes.len();
        if n == 0 { return false; }

        // Use a bitset for visiting/recursion stack to avoid heap allocation
        let mut visited = 0u64;
        let mut rec_stack = 0u64;

        for i in 0..n {
            if self.is_cyclic(i, &mut visited, &mut rec_stack) { return true; }
        }
        false
    }

    fn is_cyclic(&self, v: usize, visited: &mut u64, rec_stack: &mut u64) -> bool {
        let bit = 1 << v;
        if (*rec_stack & bit) != 0 { return true; }
        if (*visited & bit) != 0 { return false; }

        *visited |= bit;
        *rec_stack |= bit;

        let node = &self.nodes[v];
        // For each node that depends on this node's output
        // Buffer index is node.output_indices.
        // We find other nodes that have these indices in their input_indices.
        for out_idx in &node.output_indices {
            for (next_v, next_node) in self.nodes.iter().enumerate() {
                if next_node.input_indices.contains(out_idx) {
                    if self.is_cyclic(next_v, visited, rec_stack) { return true; }
                }
            }
        }

        *rec_stack &= !bit;
        false
    }

    pub fn new() -> Self {
        let mut buffers = Vec::with_capacity(64);
        for _ in 0..64 { buffers.push(AudioBlock { data: [0.0f32; 128] }); }
        let mut v2p = [0; 64];
        for i in 0..64 { v2p[i] = i; }
        Self {
            nodes: Vec::new(),
            buffers,
            crossfades: Vec::with_capacity(8),
            crossfade_buffers: [AudioBlock { data: [0.0f32; 128] }; 8],
            virtual_to_physical: v2p,
        }
    }
    pub fn add_node(&mut self, processor: Box<dyn AudioProcessor>, inputs: Vec<usize>, outputs: Vec<usize>) {
        if self.nodes.len() >= 64 { return; } // Prevent overflow in cycle detection bitset
        let _max_idx = outputs.iter().chain(inputs.iter()).cloned().max().unwrap_or(0);
        self.nodes.push(ProcessorNode { processor, input_indices: inputs, output_indices: outputs });
        // Correct topological order might be needed here, but for now we update scratchpad
        self.update_scratchpad();
    }
}

impl AudioProcessor for ProcessorGraph {
    fn process(&mut self, _external_inputs: &[&[f32]], external_outputs: &mut [&mut [f32]]) {
        let num_samples = if !external_outputs.is_empty() { external_outputs[0].len() } else { 0 };
        if num_samples == 0 { return; }

        let buffers_ptr = self.buffers.as_mut_ptr();

        // Handle crossfades
        for i in 0..self.crossfades.len() {
            let xf = &mut self.crossfades[i];
            let samples_to_fade = (xf.remaining_samples as usize).min(num_samples);

            unsafe {
                let old_p = self.virtual_to_physical[xf.old_buffer_idx.min(63)];
                let new_p = self.virtual_to_physical[xf.new_buffer_idx.min(63)];
                let old_buf = &(*buffers_ptr.add(old_p)).data;
                let new_buf = &(*buffers_ptr.add(new_p)).data;
                let target_buf = &mut self.crossfade_buffers[i].data;

                for j in 0..samples_to_fade {
                    let fade_in = (xf.total_samples - xf.remaining_samples + j as u32) as f32 / xf.total_samples as f32;
                    let fade_out = 1.0 - fade_in;
                    target_buf[j] = old_buf[j] * fade_out + new_buf[j] * fade_in;
                }

                // Fill remainder of block with new buffer if fade finished
                if samples_to_fade < num_samples {
                    target_buf[samples_to_fade..num_samples].copy_from_slice(&new_buf[samples_to_fade..num_samples]);
                }
            }

            xf.remaining_samples -= samples_to_fade as u32;
        }

        for (n_idx, node) in self.nodes.iter_mut().enumerate() {
            let mut node_inputs_storage = [ &[][..]; 16 ];
            let num_inputs = node.input_indices.len().min(16);
            for i in 0..num_inputs {
                let idx = node.input_indices[i];

                let mut crossfaded = false;
                for (xf_idx, xf) in self.crossfades.iter().enumerate() {
                    if xf.node_idx == n_idx && xf.input_idx == i {
                        node_inputs_storage[i] = &self.crossfade_buffers[xf_idx].data[..num_samples];
                        crossfaded = true;
                        break;
                    }
                }

                if !crossfaded {
                    unsafe {
                        let p_idx = self.virtual_to_physical[idx.min(63)];
                        let buf_ptr: *const AudioBlock = buffers_ptr.add(p_idx);
                        let buf_ref: &AudioBlock = &*buf_ptr;
                        node_inputs_storage[i] = &buf_ref.data[..num_samples];
                    }
                }
            }
            let mut node_outputs_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
            let num_outputs = node.output_indices.len().min(16);
            for i in 0..num_outputs {
                let idx = node.output_indices[i];
                unsafe {
                    let p_idx = self.virtual_to_physical[idx.min(63)];
                    let buf_ptr: *mut AudioBlock = buffers_ptr.add(p_idx);
                    let buf_ref: &mut AudioBlock = &mut *buf_ptr;
                    node_outputs_ptrs[i] = buf_ref.data.as_mut_ptr();
                }
            }
            let mut node_outputs_reconstructed: [&mut [f32]; 16] = std::array::from_fn(|i| {
                if i < num_outputs { unsafe { std::slice::from_raw_parts_mut(node_outputs_ptrs[i], num_samples) } } else { &mut [] }
            });
            node.processor.process(&node_inputs_storage[..num_inputs], &mut node_outputs_reconstructed[..num_outputs]);
        }
        if external_outputs.len() >= 2 && self.buffers.len() >= 2 {
            external_outputs[0].copy_from_slice(&self.buffers[0].data[..num_samples]);
            external_outputs[1].copy_from_slice(&self.buffers[1].data[..num_samples]);
        }

        // Cleanup finished crossfades
        for i in (0..self.crossfades.len()).rev() {
            if self.crossfades[i].remaining_samples == 0 {
                self.crossfades.swap_remove(i);
            }
        }
    }
    fn apply_command(&mut self, command: &control_plane::Command) {
        match command {
            control_plane::Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let mut old = 0;
                let mut found = false;
                if let Some(node) = self.nodes.get_mut(*node_idx as usize) {
                    if let Some(input) = node.input_indices.get_mut(*input_idx as usize) {
                        if (*new_buffer_idx as usize) < self.buffers.len() {
                            old = *input;
                            *input = *new_buffer_idx as usize;
                            found = true;
                        }
                    }
                }
                if found && self.detect_cycle() {
                    if let Some(node) = self.nodes.get_mut(*node_idx as usize) {
                        node.input_indices[*input_idx as usize] = old;
                    }
                } else if found {
                    self.update_scratchpad();
                }
            }
            control_plane::Command::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let mut old = 0;
                let mut found = false;
                if let Some(node) = self.nodes.get_mut(*node_idx as usize) {
                    if let Some(output) = node.output_indices.get_mut(*output_idx as usize) {
                        if (*new_buffer_idx as usize) < self.buffers.len() {
                            old = *output;
                            *output = *new_buffer_idx as usize;
                            found = true;
                        }
                    }
                }
                if found && self.detect_cycle() {
                    if let Some(node) = self.nodes.get_mut(*node_idx as usize) {
                        node.output_indices[*output_idx as usize] = old;
                    }
                } else if found {
                    self.update_scratchpad();
                }
            }
            control_plane::Command::Bundle { count, data: _ } => {
                for _i in 0..*count {
                    // For prototype, we'd decode data into commands.
                }
            }
            control_plane::Command::SwapProcessor { node_idx, processor_type_id } => {
                if let Some(node) = self.nodes.get_mut(*node_idx as usize) {
                    // For prototype, we implement a few hardcoded swaps
                    match processor_type_id {
                        1 => { node.processor = Box::new(BiquadProcessor::new(0, audio_dsp::BiquadCoefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 })); }
                        2 => { node.processor = Box::new(GainProcessor::new(0, 1.0)); }
                        _ => {}
                    }
                }
            }
            control_plane::Command::UpdateEdgeCrossfaded { node_idx, input_idx, new_buffer_idx, duration_samples } => {
                let mut old_buffer_idx = 0;
                let mut found = false;
                if let Some(node) = self.nodes.get_mut(*node_idx as usize) {
                    if let Some(input) = node.input_indices.get_mut(*input_idx as usize) {
                        if (*new_buffer_idx as usize) < self.buffers.len() {
                            old_buffer_idx = *input;
                            *input = *new_buffer_idx as usize;
                            found = true;
                        }
                    }
                }
                if found {
                    if self.detect_cycle() {
                        if let Some(node) = self.nodes.get_mut(*node_idx as usize) {
                            node.input_indices[*input_idx as usize] = old_buffer_idx;
                        }
                        return;
                    }
                    self.update_scratchpad();
                    if self.crossfades.len() < self.crossfades.capacity() {
                        self.crossfades.push(CrossfadeState {
                            node_idx: *node_idx as usize,
                            input_idx: *input_idx as usize,
                            old_buffer_idx,
                            new_buffer_idx: *new_buffer_idx as usize,
                            remaining_samples: *duration_samples,
                            total_samples: *duration_samples,
                        });
                    }
                }
            }
            _ => {
                for node in &mut self.nodes { node.processor.apply_command(command); }
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
        unsafe { (*self.signal).pulse_heartbeat(); }
        while let Some(meta) = self.poll_feedback() {
            self.last_metadata = Some(meta);
        }
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
    fn test_cycle_prevention() {
        let mut graph = ProcessorGraph::new();
        struct Pass { }
        impl AudioProcessor for Pass { fn process(&mut self, _: &[&[f32]], _: &mut [&mut [f32]]) {} }

        graph.add_node(Box::new(Pass {}), vec![1], vec![2]); // Node 0: In 1, Out 2
        graph.add_node(Box::new(Pass {}), vec![2], vec![3]); // Node 1: In 2, Out 3

        // Try to create cycle: Node 2: In 3, Out 1
        graph.add_node(Box::new(Pass {}), vec![3], vec![1]);

        assert!(graph.detect_cycle(), "Cycle should be detected");

        // Now try via command
        let mut graph2 = ProcessorGraph::new();
        graph2.add_node(Box::new(Pass {}), vec![1], vec![2]); // Node 0
        graph2.add_node(Box::new(Pass {}), vec![2], vec![3]); // Node 1
        graph2.add_node(Box::new(Pass {}), vec![4], vec![1]); // Node 2 (no cycle yet, In 4)

        assert!(!graph2.detect_cycle());

        // Command to change Node 2 In 4 -> In 3 (Cycle!)
        graph2.apply_command(&control_plane::Command::UpdateEdge {
            node_idx: 2,
            input_idx: 0,
            new_buffer_idx: 3
        });

        assert!(!graph2.detect_cycle(), "Cycle should have been prevented and reverted");
        assert_eq!(graph2.nodes[2].input_indices[0], 4, "Input should have reverted to 4");
    }

    #[test]
    fn test_graph_serialization() {
        let mut graph = ProcessorGraph::new();
        graph.add_node(Box::new(ConstantProcessor { val: 1.0 }), vec![1], vec![2]);
        let json = graph.serialize_to_json();
        assert!(json.contains("\"inputs\": [1]"));
        assert!(json.contains("\"outputs\": [2]"));
    }

    #[test]
    fn test_parameter_ramping() {
        let (cmd_p, cmd_c) = RingBuffer::<TimestampedCommand>::new(16).split();
        let (gar_p, _gar_c) = RingBuffer::<Box<Box<dyn AudioProcessor>>>::new(16).split();
        let (tel_p, _tel_c) = RingBuffer::<Telemetry>::new(16).split();

        let gain_proc = GainProcessor::new(123, 0.0);
        let mut graph = ProcessorGraph::new();
        graph.add_node(Box::new(ConstantProcessor { val: 1.0 }), vec![], vec![2]);
        graph.add_node(Box::new(gain_proc), vec![2], vec![0]);

        let mut engine = AudioEngine::new(cmd_c, gar_p, tel_p, Box::new(graph));

        let mut out_l = [0.0f32; 10];
        let mut out_r = [0.0f32; 10];
        let mut out_refs = [&mut out_l[..], &mut out_r[..]];

        let mut producer = cmd_p;
        producer.push(TimestampedCommand {
            timestamp_samples: 0,
            command: Command::SetParam { target_id: 123, param_id: 0, value: 1.0, ramp_duration_samples: 10 }
        }).unwrap();

        engine.process_block(&[], &mut out_refs, 10);

        // Check if ramp occurred. 0.0 -> 1.0 over 10 samples means steps of 0.1
        for i in 0..10 {
            let expected = (i + 1) as f32 * 0.1;
            assert!((out_l[i] - expected).abs() < 0.0001, "Sample {} mismatch: got {}, want {}", i, out_l[i], expected);
        }
    }

    #[test]
    fn test_sample_accurate_rewiring() {
        let (cmd_p, cmd_c) = RingBuffer::<TimestampedCommand>::new(16).split();
        let (gar_p, _gar_c) = RingBuffer::<Box<Box<dyn AudioProcessor>>>::new(16).split();
        let (tel_p, _tel_c) = RingBuffer::<Telemetry>::new(16).split();

        let mut graph = ProcessorGraph::new();
        graph.add_node(Box::new(ConstantProcessor { val: 1.0 }), vec![], vec![2]); // Node 0 -> Buf 2
        graph.add_node(Box::new(ConstantProcessor { val: 2.0 }), vec![], vec![3]); // Node 1 -> Buf 3

        // Passthrough node
        struct Pass { }
        impl AudioProcessor for Pass {
            fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
                if !inputs.is_empty() && !outputs.is_empty() { outputs[0].copy_from_slice(inputs[0]); }
            }
        }
        graph.add_node(Box::new(Pass {}), vec![2], vec![0]); // Node 2: Input Buf 2, Output Buf 0

        let mut engine = AudioEngine::new(cmd_c, gar_p, tel_p, Box::new(graph));

        let mut out_l = [0.0f32; 10];
        let mut out_r = [0.0f32; 10];
        let mut out_refs = [&mut out_l[..], &mut out_r[..]];

        // Command to switch Node 2's input from Buf 2 (1.0) to Buf 3 (2.0) at sample 5
        let mut producer = cmd_p;
        producer.push(TimestampedCommand {
            timestamp_samples: 5,
            command: Command::UpdateEdge { node_idx: 2, input_idx: 0, new_buffer_idx: 3 }
        }).unwrap();

        engine.process_block(&[], &mut out_refs, 10);

        for i in 0..5 { assert_eq!(out_l[i], 1.0, "Sample {} should be 1.0", i); }
        for i in 5..10 { assert_eq!(out_l[i], 2.0, "Sample {} should be 2.0", i); }
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
    handle: Option<thread::JoinHandle<()>>,
}

pub struct PipewireBackend {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
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
        if let control_plane::Command::SetParam { target_id, param_id: _, value, ramp_duration_samples: _ } = command {
            if *target_id == self.id {
                // For now, Biquad doesn't support ramping coefficients easily without stability issues,
                // so we just update them. But we could implement interpolating filter forms here.
            }
        }
    }
}

pub struct SimdBiquadProcessor {
    filter: audio_dsp::SimdBiquad,
}

impl SimdBiquadProcessor {
    pub fn new(coeffs: audio_dsp::BiquadCoefficients) -> Self {
        Self { filter: audio_dsp::SimdBiquad::new(coeffs) }
    }
}

impl AudioProcessor for SimdBiquadProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        let num_channels = inputs.len().min(outputs.len()).min(8);
        if num_channels == 0 { return; }
        let len = inputs[0].len();

        #[cfg(target_arch = "x86_64")]
        {
            if num_channels == 8 && is_x86_feature_detected!("avx2") {
                let mut in_ptrs = [std::ptr::null(); 8];
                let mut out_ptrs = [std::ptr::null_mut(); 8];
                for i in 0..8 {
                    in_ptrs[i] = inputs[i].as_ptr();
                    out_ptrs[i] = outputs[i].as_mut_ptr();
                }
                unsafe {
                    self.filter.process_8_channels(in_ptrs, out_ptrs, len);
                }
                return;
            }
        }

        // Fallback for non-x86, no AVX2, or fewer than 8 channels
        for i in 0..num_channels {
            self.filter.process_scalar(i, inputs[i], outputs[i]);
        }
    }
}
impl AlsaBackend {
    pub fn new() -> Self { Self { running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), handle: None } }
}
struct PwLib {
    handle: *mut std::ffi::c_void,
    pw_init: unsafe extern "C" fn(*mut i32, *mut *mut *mut i8),
    pw_main_loop_new: unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void,
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
                pw_main_loop_new: std::mem::transmute(load_sym(b"pw_main_loop_new\0").ok_or("pw_main_loop_new failed")?),
            })
        }
    }
}

impl AudioBackend for PipewireBackend {
    fn start(&mut self, _engine: AudioEngine) -> Result<(), String> {
        let _pw = PwLib::load()?;
        self.running.store(true, Ordering::SeqCst);
        // PipeWire SPA integration foundation:
        // We would setup a pw_thread_loop and an SPA node here.
        Ok(())
    }
    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
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
