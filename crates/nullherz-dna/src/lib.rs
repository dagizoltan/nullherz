use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};

pub type SampleBuffer = Arc<Vec<f32>>;

#[derive(Debug, Clone, Default)]
pub struct SampleMetadata {
    pub bpm: f32,
    pub transients: Arc<Vec<u64>>, // Use Arc for RT-safe cloning
    pub root_key: Option<f32>,
    pub hot_cues: [Option<u64>; 8],
    pub loop_points: Option<(u64, u64)>,
    pub beat_grid_offset: u64,
}

#[derive(Clone)]
pub struct RegisteredSample {
    pub buffer: SampleBuffer,
    pub metadata: SampleMetadata,
}

/// A thread-safe registry for managing shared audio buffers.
/// This allows processors like CaptureProcessor to register live takes
/// that can then be used as sources for GranularProcessor.
///
/// Uses an atomic-swap pattern for RT-safe, lock-free reads.
pub struct SampleRegistry {
    inner: AtomicPtr<HashMap<u64, RegisteredSample>>,
    write_lock: Mutex<()>,
    garbage: Mutex<Vec<*mut HashMap<u64, RegisteredSample>>>,
}

unsafe impl Send for SampleRegistry {}
unsafe impl Sync for SampleRegistry {}

impl Default for SampleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SampleRegistry {
    pub fn new() -> Self {
        let initial_map = Box::new(HashMap::new());
        Self {
            inner: AtomicPtr::new(Box::into_raw(initial_map)),
            write_lock: Mutex::new(()),
            garbage: Mutex::new(Vec::new()),
        }
    }

    /// Registers a new sample buffer.
    /// MUST NOT be called from the real-time audio thread.
    pub fn register(&self, id: u64, buffer: SampleBuffer) {
        self.register_with_metadata(id, buffer, SampleMetadata::default());
    }

    pub fn register_with_metadata(&self, id: u64, buffer: SampleBuffer, metadata: SampleMetadata) {
        let _lock = self.write_lock.lock().unwrap();

        let old_ptr = self.inner.load(Ordering::Acquire);
        let mut new_map = unsafe { (*old_ptr).clone() };
        new_map.insert(id, RegisteredSample { buffer, metadata });

        let new_ptr = Box::into_raw(Box::new(new_map));
        self.inner.store(new_ptr, Ordering::Release);

        // Defer reclamation to the non-RT side to prevent Use-After-Free
        // for mid-block reads on the audio thread.
        self.garbage.lock().unwrap().push(old_ptr);
    }

    /// Safely reclaims memory from old registry versions.
    /// MUST be called periodically from a non-real-time maintenance thread.
    pub fn drain_garbage(&self) {
        let mut g = self.garbage.lock().unwrap();
        for ptr in g.drain(..) {
            unsafe {
                drop(Box::from_raw(ptr));
            }
        }
    }

    /// Retrieves a registered sample by ID.
    /// Real-time safe and lock-free.
    pub fn get(&self, id: u64) -> Option<RegisteredSample> {
        let ptr = self.inner.load(Ordering::Acquire);
        unsafe { (*ptr).get(&id).cloned() }
    }

    pub fn get_buffer(&self, id: u64) -> Option<SampleBuffer> {
        self.get(id).map(|s| s.buffer)
    }

    pub fn remove(&self, id: u64) {
        let _lock = self.write_lock.lock().unwrap();

        let old_ptr = self.inner.load(Ordering::Acquire);
        let mut new_map = unsafe { (*old_ptr).clone() };
        new_map.remove(&id);

        let new_ptr = Box::into_raw(Box::new(new_map));
        self.inner.store(new_ptr, Ordering::Release);
    }

    pub fn list_ids(&self) -> Vec<u64> {
        let ptr = self.inner.load(Ordering::Acquire);
        unsafe { (*ptr).keys().cloned().collect() }
    }
}

impl Drop for SampleRegistry {
    fn drop(&mut self) {
        let ptr = self.inner.load(Ordering::Acquire);
        unsafe {
            drop(Box::from_raw(ptr));
        }
        self.drain_garbage();
    }
}
