use std::sync::Arc;
use crate::*;

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

pub trait MidiConsumer: Send {
    fn pop(&mut self) -> Option<MidiEvent>;
}

pub trait TopologyMutationConsumer: Send {
    fn pop(&mut self) -> Option<TopologyMutation>;
}

#[derive(Debug)]
pub struct MmapBuffer {
    mmap: memmap2::Mmap,
    pub sample_count: usize,
}

impl MmapBuffer {
    pub fn from_mmap(mmap: memmap2::Mmap) -> Self {
        let sample_count = mmap.len() / std::mem::size_of::<f32>();
        Self { mmap, sample_count }
    }

    pub fn open<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Ok(Self::from_mmap(mmap))
    }

    pub fn as_slice(&self) -> &[f32] {
        let ptr = self.mmap.as_ptr() as *const f32;
        unsafe { std::slice::from_raw_parts(ptr, self.sample_count) }
    }
}

impl PartialEq for MmapBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl AsRef<[f32]> for MmapBuffer {
    fn as_ref(&self) -> &[f32] {
        self.as_slice()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SampleBuffer {
    Heap(Arc<Vec<f32>>),
    Mmap(Arc<MmapBuffer>),
}

impl SampleBuffer {
    pub fn as_slice(&self) -> &[f32] {
        match self {
            SampleBuffer::Heap(v) => v.as_slice(),
            SampleBuffer::Mmap(m) => m.as_slice(),
        }
    }

    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        match (a, b) {
            (SampleBuffer::Heap(h1), SampleBuffer::Heap(h2)) => Arc::ptr_eq(h1, h2),
            (SampleBuffer::Mmap(m1), SampleBuffer::Mmap(m2)) => Arc::ptr_eq(m1, m2),
            _ => false,
        }
    }

    pub fn to_voice_buffer(&self) -> audio_dsp::SampleBufferRef {
        match self {
            SampleBuffer::Heap(arc) => audio_dsp::SampleBufferRef::Heap(arc.clone()),
            SampleBuffer::Mmap(mmap) => audio_dsp::SampleBufferRef::Shared(mmap.clone() as Arc<dyn AsRef<[f32]> + Send + Sync>),
        }
    }
}

impl From<SampleBuffer> for audio_dsp::SampleBufferRef {
    fn from(buf: SampleBuffer) -> Self {
        buf.to_voice_buffer()
    }
}

impl From<&SampleBuffer> for audio_dsp::SampleBufferRef {
    fn from(buf: &SampleBuffer) -> Self {
        buf.to_voice_buffer()
    }
}

impl std::ops::Deref for SampleBuffer {
    type Target = [f32];
    fn deref(&self) -> &[f32] {
        self.as_slice()
    }
}

impl From<Arc<Vec<f32>>> for SampleBuffer {
    fn from(v: Arc<Vec<f32>>) -> Self {
        SampleBuffer::Heap(v)
    }
}

impl From<Vec<f32>> for SampleBuffer {
    fn from(v: Vec<f32>) -> Self {
        SampleBuffer::Heap(Arc::new(v))
    }
}

#[derive(Clone)]
pub struct RegisteredSample {
    pub buffer: SampleBuffer,
    pub metadata: Arc<SampleMetadata>,
}

/// What a registry currently holds. See [`SampleRegistry::residency`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Residency {
    /// Samples currently registered.
    pub count: u32,
    /// Decoded audio held by those samples, in bytes.
    pub bytes: u64,
}

pub trait SampleRegistry: Send + Sync {
    fn get(&self, id: u64) -> Option<RegisteredSample>;

    /// Resident sample count and decoded audio bytes, in O(1).
    ///
    /// Deliberately NOT defaulted, for the same reason as `remove` below: a
    /// default returning zero would read as "nothing is resident" from an
    /// implementor that simply does not track it, and residency is what the
    /// caller uses to decide whether to evict.
    ///
    /// O(1) is part of the contract, not an implementation note. The conductor
    /// reads this twice a second from `tick()` — the thread that feeds the RT
    /// command ring — and it used to compute it by walking every registered
    /// sample and cloning each one: 678 µs at 16k samples, unbounded in library
    /// size, on a thread with a 5.8 ms budget. Implement it by accounting on
    /// register/remove, never by walking.
    fn residency(&self) -> Residency;
    fn register(&self, id: u64, buffer: SampleBuffer);
    fn register_with_metadata(&self, id: u64, buffer: SampleBuffer, metadata: Arc<SampleMetadata>);
    fn drain_garbage(&self);
    fn list_ids(&self) -> Vec<u64>;

    /// Drop the registry's reference to a sample, returning it to the caller.
    ///
    /// Deliberately has NO default implementation. A default returning `None`
    /// would let an implementor silently never evict while callers believed
    /// residency was bounded — and unbounded residency is exactly the bug this
    /// exists to fix (a 500-track library scan held every decoded track in RAM
    /// forever, tens of gigabytes).
    ///
    /// **The returned value is the point.** The registry's `Arc` is dropped
    /// wherever the caller drops the return value, so an eviction driven from a
    /// background thread frees there. Discarding it inline is fine ONLY off the
    /// audio thread: if a sampler currently holds the last other reference,
    /// dropping the final `Arc` is a multi-megabyte `free()`, and on the
    /// SCHED_FIFO thread that is a dropout. Evicting a sample that is loaded on
    /// a deck therefore needs a garbage-return channel that does not exist yet
    /// — until it does, only evict samples no processor holds.
    ///
    /// Returns `None` if the id was not registered.
    #[must_use = "dropping the evicted sample here frees it on THIS thread; ensure that is not the audio thread"]
    fn remove(&self, id: u64) -> Option<RegisteredSample>;
}

pub trait CommandBundleConsumer: Send {
    fn pop(&mut self) -> Option<Vec<Command>>;
}

