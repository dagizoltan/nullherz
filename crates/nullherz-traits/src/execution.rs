use std::sync::Arc;
use crate::*;

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
    /// # Safety
    /// `data` must point to a valid memory region of at least `size` bytes that
    /// stays alive and unaliased-for-writes until `wait_for_completion` returns;
    /// `exec_fn` will be called by a worker thread with that pointer.
    unsafe fn push_job_raw(&mut self, worker_idx: usize, data: *const u8, size: usize, exec_fn: fn(*const u8)) -> bool;
    fn wait_for_completion(&mut self, target_count: usize);
    fn current_completion_count(&self) -> usize;
    fn notify_workers(&mut self);
}

pub trait ExecutionProvider: Send + Sync {
    fn as_any(&mut self) -> &mut dyn std::any::Any;
}

/// Alignment for SIMD (AVX-512 requires 64 bytes).
pub const SIMD_ALIGNMENT: usize = 64;

/// CAPACITY of an in-process render block, not the realtime block size.
///
/// The realtime block comes from `system_config.json` and should stay at
/// 128–256: at large block sizes a stereo buffer pair no longer fits
/// comfortably in L2 once several decks are live, so big blocks are WORSE per
/// sample for live playback. The capacity exists so that (a) a weak machine can
/// run a 512/1024 safe-mode block, and (b) offline rendering can use larger
/// blocks, where it is markedly cheaper per sample.
///
/// DISTINCT from `IPC_BLOCK_SIZE`. This one sizes engine-internal buffers, which
/// are cheap to grow. `AudioBlock` — the shared-memory type sent to sidecars —
/// is a protocol ABI whose size multiplies across every SHM ring, so it has its
/// own constant and does not follow this one.
pub const MAX_BLOCK_SIZE: usize = 1024;

/// Frames per `AudioBlock`, the shared-memory audio unit exchanged with
/// sidecars. This is a PROTOCOL ABI size — deliberately NOT `MAX_BLOCK_SIZE`.
///
/// `AudioBlock` is allocated in fixed 16-deep SHM rings, one per input, output
/// and sidechain channel of every sidecar. At 256 frames a block is 1088 bytes
/// and a ring is ~17 KB; the size scales linearly, so tying it to the render
/// capacity would multiply every ring by the same factor while `len` still
/// reported the (much smaller) realtime block — paying the bandwidth and
/// residency for frames that are never used.
///
/// Because these are separate, a render block LARGER than this must be split
/// before it crosses the IPC boundary. The sidecar bridge currently clamps
/// instead of chunking; until that is fixed it asserts rather than truncating
/// silently.
pub const IPC_BLOCK_SIZE: usize = 256;

/// Node (processor) address space. See `NodeConventions` for the LOGICAL id
/// range that must stay disjoint from this one.
///
/// Raised from 64: the 4-deck bootstrap already used ~46, and exposing tap
/// points between EQ bands means decomposing processors into more nodes. Cost
/// is modest but not free — `CompiledGraphPlan` holds `[[u32; MAX_NODES];
/// MAX_NODES]`, so its allocation grows as 4N² (21 KB at 64, ~74 KB at 128).
/// That is allocation, not traversal: the graph walk reads only populated
/// stages, so the working set stays proportional to the nodes actually in use.
pub const MAX_NODES: usize = 128;
/// Audio-buffer (edge) address space, decoupled from MAX_NODES. A stereo
/// console needs roughly two buffers per strip stage: 4 decks x 8 stages x 2
/// channels plus buses and master already exceeds 64, and the graph clamps or
/// wraps out-of-range buffer indices SILENTLY (aliasing two edges onto one
/// buffer — instant corruption, no error). Kept within u8 sentinel range:
/// `block_x_map` encodes crossfade overrides as `MAX_BUFFERS + k` in a u8, so
/// MAX_BUFFERS + MAX_CROSSFADE_BUFFERS must stay <= 255.
///
/// Raised from 128 with the node-space increase: the 4-deck console already
/// reaches buffer ~87, and decomposing deck EQs to expose per-band tap points
/// adds roughly eight buffers per deck — 128 would have been one feature from
/// the wall. This is also the TAP BUDGET: a tap is a subscription on an existing
/// buffer edge, so the number of addressable taps is bounded by this constant.
/// The u8 encoding caps it at 247; 240 leaves headroom for crossfade sentinels.
pub const MAX_BUFFERS: usize = 240;
pub const MAX_CHANNELS: usize = 16;
pub const MAX_CROSSFADE_BUFFERS: usize = 8;

/// Typed index into the audio-buffer (edge) address space.
///
/// Buffer ids and node ids are DIFFERENT address spaces (`MAX_BUFFERS` vs
/// `MAX_NODES`); comparing a buffer id against `MAX_NODES` silently corrupted
/// audio twice (compiler PDC pass, worker pool). Carrying buffer ids as a
/// distinct type makes that mistake a compile error. serde-transparent: the
/// wire/persisted format is still a bare u32.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct BufferId(pub u32);

impl BufferId {
    #[inline]
    pub const fn index(self) -> usize { self.0 as usize }
    #[inline]
    pub const fn is_valid(self) -> bool { (self.0 as usize) < MAX_BUFFERS }
}

/// A RESOLVED physical buffer slot: either a pool buffer or a crossfade
/// scratch buffer. This is the only place the `MAX_BUFFERS + k` sentinel
/// encoding (used by `block_x_map` overrides) may be interpreted — executor
/// and worker pool must both go through it, so the split point cannot drift
/// between them again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSlot {
    Pool(usize),
    Crossfade(usize),
}

impl BufferSlot {
    #[inline]
    pub const fn from_raw(p_idx: usize) -> Self {
        if p_idx >= MAX_BUFFERS { BufferSlot::Crossfade(p_idx - MAX_BUFFERS) } else { BufferSlot::Pool(p_idx) }
    }

    /// Encode a crossfade slot for a `block_x_map` u8 cell (the encode
    /// counterpart of `from_raw`). Requires
    /// `MAX_BUFFERS + MAX_CROSSFADE_BUFFERS <= 255` — 0 stays reserved for
    /// "no override".
    #[inline]
    pub const fn encode_crossfade(x_idx: usize) -> u8 {
        (MAX_BUFFERS + x_idx) as u8
    }
}

// The block_x_map u8 sentinel space must fit: 0 = no override, then pool ids,
// then crossfade slots.
const _: () = assert!(MAX_BUFFERS + MAX_CROSSFADE_BUFFERS <= 255);
/// Topology mutations that can be staged for one commit.
///
/// Overflow is a SILENT DROP — `apply_mutation` is guarded by
/// `if self.pending_mutation_count < MAX_MUTATIONS`, so a graph that needs more
/// than this arrives partially built with no error. The 4-deck bootstrap already
/// issues roughly that many AddNode/Connect mutations, so 64 was on the edge
/// before the node-space increase and would be over it once processors are
/// decomposed for tap points.
pub const MAX_MUTATIONS: usize = 256;
pub const DEFAULT_WORKER_COUNT: usize = 4;
pub const MAX_COMMANDS_PER_BLOCK: usize = 256;

/// Centralized LOGICAL node id conventions.
///
/// These are sentinels for UI/protocol use, deliberately far above MAX_NODES so
/// they can never collide with (or be mistaken for) a real graph index. The
/// conductor translates them to actual allocated node indices (e.g. PREVIEW
/// -> `node_names["preview_node"]` in `apply_mixer_commands`). Never pass one
/// to the engine as a graph index — the graph's `idx < MAX_NODES` guards
/// (`topology_coordinator::apply_mutation`) drop them.
///
/// They used to live at 70–73 and 111, which was "safely" out of range only
/// because MAX_NODES happened to be 64. Raising MAX_NODES to 128 would have
/// made all five of them LEGAL graph indices, silently turning every sentinel
/// into a command aimed at a real node. They now sit in a range no graph will
/// ever reach, and `_SENTINELS_ABOVE_MAX_NODES` below turns any future
/// MAX_NODES increase that reopens the hole into a compile error rather than
/// an audio bug.
pub struct NodeConventions;
impl NodeConventions {
    /// Base of the logical-sentinel range. Nothing below this is a sentinel.
    pub const LOGICAL_BASE: u32 = 0xFFFF_FF00;

    pub const PREVIEW: u32 = Self::LOGICAL_BASE + 0x01;
    pub const DECK_A_SEQUENCER: u32 = Self::LOGICAL_BASE + 0x10;
    pub const DECK_B_SEQUENCER: u32 = Self::LOGICAL_BASE + 0x11;
    pub const DECK_C_SEQUENCER: u32 = Self::LOGICAL_BASE + 0x12;
    pub const DECK_D_SEQUENCER: u32 = Self::LOGICAL_BASE + 0x13;

    pub fn sequencer_for_deck(deck_id: char) -> u32 {
        Self::DECK_A_SEQUENCER + (deck_id.to_ascii_uppercase() as u32 - 'A' as u32)
    }

    /// True when `node_idx` is a logical sentinel rather than a graph index.
    ///
    /// Prefer this over comparing against individual constants: it stays correct
    /// as sentinels are added, and it reads as the question the caller is
    /// actually asking.
    #[inline]
    pub const fn is_logical(node_idx: u32) -> bool {
        node_idx >= Self::LOGICAL_BASE
    }
}

/// The sentinel range must stay disjoint from the graph index space, or a
/// sentinel becomes addressable as a real node. Raising MAX_NODES past the
/// sentinel base breaks the build here instead of corrupting live decks.
const _SENTINELS_ABOVE_MAX_NODES: () =
    assert!(NodeConventions::LOGICAL_BASE as usize > MAX_NODES);

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
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AudioBlock {
    pub data: [f32; IPC_BLOCK_SIZE],
    pub len: u32,
    pub _pad: [u32; 15], // Pad to 64-byte alignment (1024 + 4 + 60 = 1088)
}

/// A standard MIDI event representation for real-time IPC.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
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

use std::cell::Cell;

thread_local! {
    static IS_RT_THREAD: Cell<bool> = const { Cell::new(false) };
}

pub fn mark_as_rt_thread() {
    IS_RT_THREAD.with(|cell| cell.set(true));
}

pub fn is_rt_thread() -> bool {
    IS_RT_THREAD.with(|cell| cell.get())
}

#[macro_export]
macro_rules! assert_rt_safe {
    () => {
        #[cfg(debug_assertions)]
        {
            if $crate::is_rt_thread() {
                // Stage 4 Hardening: Integration with allocator-aware guard.
                // In a full implementation, this calls into a global allocator
                // that tracks per-thread allocation flags.
            }
        }
    };
}

/// Helper to ensure a closure does not allocate if called from an RT thread.
pub fn run_rt_safe<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    assert_rt_safe!();
    f()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProcessorCapability {
    pub supports_parallel: bool,
    pub is_instrument: bool,
    pub has_midi_input: bool,
    pub has_audio_input: bool,
    pub has_audio_output: bool,
}

impl Default for ProcessorCapability {
    fn default() -> Self {
        Self {
            supports_parallel: false,
            is_instrument: false,
            has_midi_input: false,
            has_audio_input: true,
            has_audio_output: true,
        }
    }
}

pub trait ProcessorFactory: Send + Sync {
    fn create_processor(&self, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>>;
    fn name(&self) -> &'static str;
    fn type_id(&self) -> ProcessorTypeId;
    fn capabilities(&self) -> ProcessorCapability {
        ProcessorCapability::default()
    }
}

pub trait SignalProcessor: Send {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext);
    fn process_parallel(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext, _executor: Option<&mut (dyn ParallelExecutor + '_)>) {
        self.process(inputs, outputs, context);
    }
    fn setup(&mut self, _config: AudioConfig) {}
    fn set_safe_mode(&mut self, _enabled: bool) {}
    fn reset(&mut self) {}
    fn latency_samples(&self) -> usize { 0 }
}

pub trait MidiResponder: Send {
    fn apply_midi(&mut self, _event: MidiEvent, _context: Option<&ProcessContext>) {}
}

pub trait SnapshotProvider: Send {
    fn pull_snapshot(&mut self) -> Option<Arc<Vec<f32>>> { None }
    fn pull_all_snapshots(&mut self, _target: &mut Vec<(u64, Arc<Vec<f32>>)>) {}
}

pub trait AudioProcessor: SignalProcessor + MidiResponder + SnapshotProvider + Send {
    fn apply_command(&mut self, _command: &ProcessorCommand) {}
    fn set_parameter(&mut self, _param_id: u32, _value: f32, _ramp_duration_samples: u32) {}
    fn get_parameter(&self, _param_id: u32) -> f32 { 0.0 }
    fn serialize_state(&self) -> Vec<u8> { Vec::new() }
    fn apply_topology_mutation(&mut self, _mutation: TopologyMutation) {}
    fn collect_telemetry(&self, _node_times: &mut [u64; MAX_NODES], _peak_levels: &mut [f32; MAX_NODES]) {}
    fn metadata(&self) -> Option<ProcessorMetadata> { None }
    fn set_garbage_producer(&mut self, _producer: Box<dyn GarbageProducer>) {}
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    fn list_children(&self) -> Vec<&dyn AudioProcessor> { Vec::new() }
    fn resource_id(&self) -> Option<u64> { None }
    fn load_state(&mut self, _data: &[u8]) {}
    fn processor_type(&self) -> &'static str { "" }
    fn get_playback_position(&self) -> u64 { 0 }
}


#[cfg(test)]
mod sub_block_properties {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// The invariant the whole sample-accurate command kernel rests on:
        /// for ANY block length, the iterator emits contiguous, non-overlapping
        /// chunks that cover every sample exactly once, each within the max
        /// block size, with is_last set on exactly the final chunk.
        #[test]
        fn chunks_cover_every_sample_exactly_once(
            total_len in 0usize..4096,
            max_block in 1usize..512,
        ) {
            let mut it = SubBlockIterator::new(total_len, max_block);
            let mut covered = 0usize;
            let mut last_seen = false;
            while let Some(sb) = it.next_chunk() {
                prop_assert!(!last_seen, "no chunk may follow is_last");
                prop_assert_eq!(sb.offset, covered, "chunks must be contiguous");
                prop_assert!(sb.len >= 1 && sb.len <= max_block);
                covered += sb.len;
                last_seen = sb.is_last;
            }
            prop_assert_eq!(covered, total_len, "every sample exactly once");
            prop_assert_eq!(last_seen, total_len > 0, "is_last on the final chunk iff non-empty");
        }

        /// Mixing command-split boundaries (next_chunk_up_to) with plain chunks
        /// must preserve the same exactly-once coverage — this models a block
        /// containing sample-accurate commands at arbitrary timestamps.
        #[test]
        fn command_splits_preserve_coverage(
            total_len in 1usize..2048,
            max_block in 1usize..300,
            splits in proptest::collection::vec(0usize..2048, 0..8),
        ) {
            let mut boundaries = splits;
            boundaries.sort_unstable();

            let mut it = SubBlockIterator::new(total_len, max_block);
            let mut covered = 0usize;
            for b in boundaries {
                while it.current_offset < b.min(total_len) {
                    match it.next_chunk_up_to(b) {
                        Some(sb) => {
                            prop_assert_eq!(sb.offset, covered);
                            prop_assert!(sb.len <= max_block);
                            prop_assert!(sb.offset + sb.len <= b.max(sb.offset + sb.len).min(total_len.max(sb.offset + sb.len)));
                            covered += sb.len;
                        }
                        None => break,
                    }
                }
            }
            while let Some(sb) = it.next_chunk() {
                prop_assert_eq!(sb.offset, covered);
                covered += sb.len;
            }
            prop_assert_eq!(covered, total_len, "splits must never lose or duplicate samples");
        }
    }
}
