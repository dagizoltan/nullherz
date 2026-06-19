use std::sync::Arc;
use std::collections::HashMap;
use std::sync::RwLock;

pub type SampleBuffer = Arc<Vec<f32>>;

/// A thread-safe registry for managing shared audio buffers.
/// This allows processors like CaptureProcessor to register live takes
/// that can then be used as sources for GranularProcessor.
///
/// NOTE: RT-safety is maintained by ensuring that writes only happen
/// from the control plane / conductor, and the RT thread only performs
/// read operations which are mostly lock-free in common paths (cloning Arc).
pub struct SampleRegistry {
    samples: RwLock<HashMap<u64, SampleBuffer>>,
}

impl Default for SampleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SampleRegistry {
    pub fn new() -> Self {
        Self {
            samples: RwLock::new(HashMap::new()),
        }
    }

    /// Registers a new sample buffer.
    /// MUST NOT be called from the real-time audio thread.
    pub fn register(&self, id: u64, buffer: SampleBuffer) {
        if let Ok(mut samples) = self.samples.write() {
            samples.insert(id, buffer);
        }
    }

    /// Retrieves a sample buffer by ID.
    /// Real-time safe if no writer is active.
    pub fn get(&self, id: u64) -> Option<SampleBuffer> {
        if let Ok(samples) = self.samples.read() {
            samples.get(&id).cloned()
        } else {
            None
        }
    }

    pub fn remove(&self, id: u64) {
        if let Ok(mut samples) = self.samples.write() {
            samples.remove(&id);
        }
    }
}
