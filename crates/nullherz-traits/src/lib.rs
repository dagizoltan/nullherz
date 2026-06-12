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

/// Command interface for processors to decouple from the control plane.
pub type ProcessorCommand = control_plane::Command;

/// MIDI event interface for processors to decouple from the IPC layer.
pub type MidiEvent = ipc_layer::MidiEvent;

/// Producer interface for processors to decouple from the IPC layer.
pub type GarbageProducer = ipc_layer::Producer<Box<dyn AudioProcessor>>;

/// Marker trait for real-time safe components.
/// Types implementing this trait guarantee that their methods do not perform
/// heap allocations, take locks, or execute blocking syscalls.
pub trait RtSafe {}

/// The core trait for all audio processing nodes in the nullherz engine.
pub trait AudioProcessor: Send {
    /// Executes audio processing for the given buffers.
    /// MUST be real-time safe: no allocations, no locks, no blocking syscalls.
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext);

    /// Called when audio configuration (sample rate, block size) changes.
    fn setup(&mut self, _config: AudioConfig) {}

    /// Applies high-level control plane commands (parameters, play/stop).
    fn apply_command(&mut self, _command: &ProcessorCommand) {}

    /// Applies structural graph mutations to the processor (routing, swapping).
    fn apply_topology_mutation(&mut self, _mutation: TopologyMutation) {}

    /// Applies real-time MIDI events to the processor.
    fn apply_midi(&mut self, _event: MidiEvent) {}

    /// Gathers performance and signal telemetry from the processor.
    fn collect_telemetry(&self, _node_times: &mut [u64; 64], _peak_levels: &mut [f32; 64]) {}

    /// Configures the garbage producer used for real-time safe deallocation.
    fn set_garbage_producer(&mut self, _producer: GarbageProducer) {}

    /// Allows safe downcasting to concrete processor types.
    fn as_any(&self) -> &dyn std::any::Any { panic!("as_any not implemented") }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { panic!("as_any_mut not implemented") }
}
