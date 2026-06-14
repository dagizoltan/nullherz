#[cfg(feature = "test-utils")]
pub mod test_kit;
pub mod telemetry;
pub mod error;

#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: f32,
    pub block_size: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Transport {
    pub bpm: f32,
    pub beat_position: f64,
    pub is_playing: bool,
    pub sample_rate: f32,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ProcessorType {
    Biquad = 1,
    Gain = 2,
    Sampler = 10,
    BiquadEQ = 11,
    Crossfader = 20,
    Summing = 30,
    Spectral = 40,
    Wavetable = 50,
}

impl TryFrom<u32> for ProcessorType {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ProcessorType::Biquad),
            2 => Ok(ProcessorType::Gain),
            10 => Ok(ProcessorType::Sampler),
            11 => Ok(ProcessorType::BiquadEQ),
            20 => Ok(ProcessorType::Crossfader),
            30 => Ok(ProcessorType::Summing),
            40 => Ok(ProcessorType::Spectral),
            50 => Ok(ProcessorType::Wavetable),
            _ => Err(()),
        }
    }
}

pub enum TopologyMutation {
    UpdateEdge {
        node_idx: u32,
        input_idx: u32,
        new_buffer_idx: u32,
    },
    UpdateOutputEdge {
        node_idx: u32,
        output_idx: u32,
        new_buffer_idx: u32,
    },
    SwapProcessor {
        node_idx: u32,
        processor: Box<dyn AudioProcessor>,
    },
    AddNode {
        node_idx: u32,
        processor: Box<dyn AudioProcessor>,
    },
}

/// Shared execution context passed to processors during the audio block cycle.
pub struct ProcessContext<'a> {
    /// Global transport information (BPM, position, play state).
    pub transport: Option<&'a Transport>,
    /// Current sample offset within the physical audio block (used for sample-accurate automation).
    pub sub_block_offset: usize,
    /// Flag indicating if this is the final sub-block for the current engine cycle.
    pub is_last_sub_block: bool,
}

pub trait ParallelExecutor: Send + Sync {
    fn as_any(&mut self) -> &mut dyn std::any::Any;
    fn num_workers(&self) -> usize;
    fn push_job(&mut self, worker_idx: usize, job: Box<dyn std::any::Any + Send>) -> Result<(), Box<dyn std::any::Any + Send>>;
    fn wait_for_completion(&mut self, target_count: usize);
    fn current_completion_count(&self) -> usize;
    fn notify_workers(&mut self);
}

pub trait ExecutionProvider: Send + Sync {
    fn as_any(&mut self) -> &mut dyn std::any::Any;
}

/// Alignment for SIMD (AVX-512 requires 64 bytes).
pub const SIMD_ALIGNMENT: usize = 64;
pub const MAX_BLOCK_SIZE: usize = 256;
pub const MAX_NODES: usize = 64;
pub const MAX_CHANNELS: usize = 16;

pub struct SubBlock {
    pub offset: usize,
    pub len: usize,
    pub is_last: bool,
}

pub struct SubBlockIterator {
    pub total_len: usize,
    pub max_block_size: usize,
    pub current_offset: usize,
}

impl SubBlockIterator {
    pub fn new(total_len: usize, max_block_size: usize) -> Self {
        Self { total_len, max_block_size, current_offset: 0 }
    }

    pub fn next_chunk(&mut self) -> Option<SubBlock> {
        if self.current_offset >= self.total_len { return None; }
        let remaining = self.total_len - self.current_offset;
        let len = remaining.min(self.max_block_size);
        let block = SubBlock {
            offset: self.current_offset,
            len,
            is_last: (self.current_offset + len) == self.total_len,
        };
        self.current_offset += len;
        Some(block)
    }

    pub fn next_chunk_up_to(&mut self, end_offset: usize) -> Option<SubBlock> {
        let end_limit = end_offset.min(self.total_len);
        if self.current_offset >= end_limit { return None; }
        let remaining = end_limit - self.current_offset;
        let len = remaining.min(self.max_block_size);
        let block = SubBlock {
            offset: self.current_offset,
            len,
            is_last: (self.current_offset + len) == self.total_len,
        };
        self.current_offset += len;
        Some(block)
    }
}

/// A SIMD-aligned audio block.
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct AudioBlock {
    pub data: [f32; MAX_BLOCK_SIZE],
    pub len: u32,
}

/// A standard MIDI event representation for real-time IPC.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiEvent {
    pub timestamp_samples: u64,
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
    pub _pad: u8,
}

/// Interface for real-time safe deallocation of processors.
pub trait GarbageProducer: Send + dyn_clone::DynClone {
    fn push_processor(&mut self, processor: Box<dyn AudioProcessor>) -> Result<(), Box<dyn AudioProcessor>>;
}

dyn_clone::clone_trait_object!(GarbageProducer);

/// Command interface for processors to decouple from the control plane.
pub type ProcessorCommand = control_plane::Command;

/// Marker trait for real-time safe components.
/// Types implementing this trait guarantee that their methods do not perform
/// heap allocations, take locks, or execute blocking syscalls.
pub trait RtSafe {}

pub trait ProcessorFactory: Send + Sync {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>>;
    fn name(&self) -> &'static str;
}

/// The core trait for all audio processing nodes in the nullherz engine.
pub trait AudioProcessor: Send {
    /// Executes audio processing for the given buffers.
    /// MUST be real-time safe: no allocations, no locks, no blocking syscalls.
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext);

    /// Executes audio processing, potentially utilizing a parallel executor.
    /// Defaults to calling `process` if no specialized parallel logic is implemented.
    fn process_parallel(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext, _executor: Option<&mut (dyn ParallelExecutor + '_)>) {
        self.process(inputs, outputs, context);
    }

    /// Called when audio configuration (sample rate, block size) changes.
    fn setup(&mut self, _config: AudioConfig) {}

    /// Applies high-level control plane commands (parameters, play/stop).
    fn apply_command(&mut self, _command: &ProcessorCommand) {}

    /// Sets a processor parameter with optional ramping.
    fn set_parameter(&mut self, _param_id: u32, _value: f32, _ramp_duration_samples: u32) {}

    /// Applies structural graph mutations to the processor (routing, swapping).
    fn apply_topology_mutation(&mut self, _mutation: TopologyMutation) {}

    /// Applies real-time MIDI events to the processor.
    fn apply_midi(&mut self, _event: MidiEvent) {}

    /// Gathers performance and signal telemetry from the processor.
    fn collect_telemetry(&self, _node_times: &mut [u64; MAX_NODES], _peak_levels: &mut [f32; MAX_NODES]) {}

    /// Returns metadata about the processor (parameters, name, etc.)
    fn metadata(&self) -> Option<control_plane::SidecarMetadata> { None }

    /// Configures the garbage producer used for real-time safe deallocation.
    fn set_garbage_producer(&mut self, _producer: Box<dyn GarbageProducer>) {}

    /// Allows safe downcasting to concrete processor types.
    fn as_any(&self) -> &dyn std::any::Any { panic!("as_any not implemented") }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { panic!("as_any_mut not implemented") }
}
