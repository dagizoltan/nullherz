use std::sync::Arc;
use crate::*;


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

/// Abstract interface for the audio rendering engine.
/// This allows backends to be decoupled from the concrete `AudioEngine` implementation.
pub trait RenderingEngine: Send + Sync {
    /// Process a block of audio. This is the primary entry point for audio processing.
    fn process_block(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize);
    /// Update the engine configuration (sample rate, block size).
    fn set_config(&mut self, config: AudioConfig);
    /// Returns the target sample rate configured for the engine.
    fn target_sample_rate(&self) -> f32;
    /// Pulls all available snapshots from the signal graph for registration.
    fn pull_all_snapshots(&self, target: &mut Vec<(u64, Arc<Vec<f32>>)>);
    /// Returns a list of all currently active child processors.
    fn list_children(&self) -> Vec<&dyn AudioProcessor>;
}

/// Interface for controlling the audio engine from a non-RT thread.
pub trait RenderingController: Send + Sync {
    /// Schedules a new signal graph to be swapped in at the next block boundary.
    fn set_pending_graph(&self, graph: Box<dyn AudioProcessor>);
}

pub struct BundleIterator<'a> {
    data: &'a [u8; 128],
    count: usize,
    index: usize,
}

impl<'a> Iterator for BundleIterator<'a> {
    type Item = Command;
    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.count || self.index >= 8 { return None; }
        let offset = self.index * 16;
        let target_id = u64::from_le_bytes(self.data[offset..offset+8].try_into().unwrap());
        let param_id = u32::from_le_bytes(self.data[offset+8..offset+12].try_into().unwrap());
        let value = f32::from_le_bytes(self.data[offset+12..offset+16].try_into().unwrap());
        self.index += 1;
        Some(Command::Mixer(MixerCommand::SetParam {
            target_id,
            param_id,
            value,
            ramp_duration_samples: 0
        }))
    }
}

impl Command {
    pub fn bundle_iter(&self) -> Option<BundleIterator<'_>> {
        if let Command::Mixer(MixerCommand::Bundle { count, data }) = self {
            Some(BundleIterator { data, count: *count as usize, index: 0 })
        } else {
            None
        }
    }

    #[deprecated(note = "Use bundle_iter instead to avoid allocation")]
    pub fn unpack_bundle(count: u32, data: [u8; 128]) -> Vec<Command> {
        let mut commands = Vec::with_capacity(count as usize);
        let iter = BundleIterator { data: &data, count: count as usize, index: 0 };
        for cmd in iter {
            commands.push(cmd);
        }
        commands
    }
}
pub struct IdAllocator {
    next_node_id: std::sync::atomic::AtomicU32,
    next_buffer_id: std::sync::atomic::AtomicU32,
}
impl IdAllocator {
    pub fn new(start_node_id: u32, start_buffer_id: u32) -> Self {
        Self { next_node_id: std::sync::atomic::AtomicU32::new(start_node_id), next_buffer_id: std::sync::atomic::AtomicU32::new(start_buffer_id) }
    }
    pub fn allocate_node_id(&self) -> u32 { self.next_node_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed) }
    pub fn allocate_buffer_id(&self, count: u32) -> u32 { self.next_buffer_id.fetch_add(count, std::sync::atomic::Ordering::Relaxed) }
    pub fn current_node_id(&self) -> u32 { self.next_node_id.load(std::sync::atomic::Ordering::Relaxed) }
    pub fn current_buffer_id(&self) -> u32 { self.next_buffer_id.load(std::sync::atomic::Ordering::Relaxed) }
}
impl Default for IdAllocator { fn default() -> Self { Self::new(0, 12) } }
