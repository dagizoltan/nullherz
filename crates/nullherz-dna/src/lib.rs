use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, Ordering};

pub type SampleBuffer = Arc<Vec<f32>>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SampleMetadata {
    pub bpm: f32,
    #[serde(skip)]
    pub transients: Arc<Vec<u64>>, // Use Arc for RT-safe cloning
    pub root_key: Option<f32>,
    pub hot_cues: [Option<u64>; 8],
    pub loop_points: Option<(u64, u64)>,
    pub beat_grid_offset: u64,
    #[serde(skip)]
    pub peaks: Arc<Vec<f32>>,
}

impl SampleMetadata {
    pub fn new_empty() -> Self {
        Self {
            bpm: 0.0,
            transients: Arc::new(Vec::new()),
            root_key: None,
            hot_cues: [None; 8],
            loop_points: None,
            beat_grid_offset: 0,
            peaks: Arc::new(Vec::new()),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LibraryTrack {
    pub id: u64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub metadata: SampleMetadata,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LibraryDatabase {
    pub tracks: Vec<LibraryTrack>,
}

impl LibraryDatabase {
    pub fn load(path: &str) -> Self {
        if let Ok(content) = std::fs::read_to_string(path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self, path: &str) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
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
    // simple reader count to prevent freeing during iteration
    readers: std::sync::atomic::AtomicUsize,
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
            readers: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Registers a new sample buffer.
    /// MUST NOT be called from the real-time audio thread.
    pub fn register(&self, id: u64, buffer: SampleBuffer) {
        self.register_with_metadata(id, buffer, SampleMetadata::new_empty());
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
        if self.readers.load(Ordering::SeqCst) > 0 {
            return; // Try again later
        }
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
        self.readers.fetch_add(1, Ordering::SeqCst);
        let ptr = self.inner.load(Ordering::Acquire);
        let res = unsafe { (*ptr).get(&id).cloned() };
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
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
        self.readers.fetch_add(1, Ordering::SeqCst);
        let ptr = self.inner.load(Ordering::Acquire);
        let res = unsafe { (*ptr).keys().cloned().collect() };
        self.readers.fetch_sub(1, Ordering::SeqCst);
        res
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
