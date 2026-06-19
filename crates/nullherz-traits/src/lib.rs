#[cfg(feature = "test-utils")]
pub mod test_kit;
pub mod telemetry;
pub mod error;

use std::sync::Arc;

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

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ProcessorTypeId(pub u32);

impl ProcessorTypeId {
    pub const BIQUAD: Self = Self(1);
    pub const GAIN: Self = Self(2);
    pub const SAMPLER: Self = Self(10);
    pub const BIQUAD_EQ: Self = Self(11);
    pub const CROSSFADER: Self = Self(20);
    pub const SUMMING: Self = Self(30);
    pub const SPECTRAL: Self = Self(40);
    pub const WAVETABLE: Self = Self(50);
    pub const MODULATION: Self = Self(60);
    pub const SEQUENCER: Self = Self(70);
    pub const ENVELOPE_FOLLOWER: Self = Self(80);
    pub const GRANULAR: Self = Self(90);
    pub const SPECTRAL_MORPH: Self = Self(100);
    pub const CAPTURE: Self = Self(110);
}

impl From<u32> for ProcessorTypeId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<ProcessorTypeId> for u32 {
    fn from(id: ProcessorTypeId) -> Self {
        id.0
    }
}

/// Represents an action to be performed by the audio engine.
/// Fixed-size strings are used to avoid heap allocations in the RT thread.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    SetParam {
        /// Target ID (e.g. hash of a name or a fixed-size buffer)
        target_id: u64,
        param_id: u32,
        value: f32,
        ramp_duration_samples: u32,
    },
    Play,
    Stop,
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
    UpdateEdgeCrossfaded {
        node_idx: u32,
        input_idx: u32,
        new_buffer_idx: u32,
        duration_samples: u32,
    },
    SwapProcessor {
        node_idx: u32,
        processor_type_id: ProcessorTypeId,
    },
    Bundle {
        // Flat array of parameter updates: [node_id, param_id, value_bits, ...]
        count: u32,
        data: [u64; 12], // Supports up to 4 bundled SetParam commands
    },
    AddNode {
        processor_type_id: ProcessorTypeId,
        node_idx: u32,
    },
    CommitTopology,
    SetSequencerStep {
        track: u32,
        step: u32,
        value: bool,
    },
    // Transfusion-specific commands
    RegisterCapture {
        capture_node_idx: u32,
        sample_id: u64,
    },
    AddSourceFromRegistry {
        granular_node_idx: u32,
        sample_id: u64,
    },
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TopologyCommand {
    AddNode {
        processor_type_id: ProcessorTypeId,
        node_idx: u32,
    },
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
        processor_type_id: ProcessorTypeId,
    },
}

/// A command with an associated timestamp for deterministic execution.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TimestampedCommand {
    pub timestamp_samples: u64,
    pub command: Command,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParameterMetadata {
    pub id: u32,
    pub name: [u8; 32],
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessorMetadata {
    pub processor_id: u64,
    pub num_parameters: u32,
    pub parameters: [ParameterMetadata; 16],
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
    AddSource {
        node_idx: u32,
        buffer: Arc<Vec<f32>>,
    },
}

/// Interface for processors to interact with the engine host (e.g., scheduling commands).
pub trait Host: Send + Sync + 'static {
    /// Pushes a command to be executed by the engine at a specific timestamp.
    fn push_command(&self, timestamp_samples: u64, command: Command);

    /// Requests the host to pull a snapshot from a processor and register it.
    fn request_registration(&self, capture_node_idx: u32, sample_id: u64);
}

/// Shared execution context passed to processors during the audio block cycle.
pub struct ProcessContext<'a> {
    /// Global transport information (BPM, position, play state).
    pub transport: Option<&'a Transport>,
    /// Interface to the engine host.
    pub host: Option<&'a dyn Host>,
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
pub type ProcessorCommand = Command;

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
    fn metadata(&self) -> Option<ProcessorMetadata> { None }

    /// Configures the garbage producer used for real-time safe deallocation.
    fn set_garbage_producer(&mut self, _producer: Box<dyn GarbageProducer>) {}

    /// Resets the internal state of the processor (e.g., filter delays, oscillator phase).
    fn reset(&mut self) {}

    /// Allows safe downcasting to concrete processor types.
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Pulls a snapshot from the processor (e.g. for registration in SampleRegistry).
    fn pull_snapshot(&mut self) -> Option<Arc<Vec<f32>>> { None }
}

pub trait CommandProducer: Send + Sync + dyn_clone::DynClone {
    fn push_command(&self, command: TimestampedCommand) -> Result<(), Command>;
}

dyn_clone::clone_trait_object!(CommandProducer);

pub trait CommandConsumer: Send {
    fn pop_command(&mut self) -> Option<TimestampedCommand>;
}

pub trait TelemetryProducer: Send {
    fn push_telemetry(&mut self, telemetry: crate::telemetry::Telemetry) -> Result<(), crate::telemetry::Telemetry>;
}

pub trait ProcessingKernel: Send {
    #[allow(clippy::too_many_arguments)]
    fn execute(
        &mut self,
        graph: &mut dyn AudioProcessor,
        transport: &mut Transport,
        host: Option<&dyn Host>,
        pool: &mut Option<Box<dyn ParallelExecutor>>,
        command_consumer: &mut Box<dyn CommandConsumer>,
        pending_command: &mut Option<TimestampedCommand>,
        sample_counter: u64,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        num_samples: usize,
    );
}
