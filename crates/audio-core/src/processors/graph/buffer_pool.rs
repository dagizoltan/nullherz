
pub const MAX_PDC_SAMPLES: usize = 4096;

/// Circular delay lines for PDC alignment.
pub struct PdcLines {
    pub data: Box<[f32]>,
    /// Interpolation scratch for the SERIAL executor path (the worker pool
    /// carries a per-thread scratch instead). Rows are fully overwritten for
    /// the `[..num_samples]` range before they are read, so reuse across
    /// nodes is safe. Owning it here (allocated once, off the RT thread)
    /// replaces a 32 KB stack zero-init that ran per node, per sub-block,
    /// whether or not any input delay was active.
    pub scratch: Box<[[f32; ipc_layer::MAX_BLOCK_SIZE]; crate::MAX_CHANNELS]>,
}

impl PdcLines {
    pub fn new() -> Self {
        let total_samples = crate::MAX_NODES * crate::MAX_CHANNELS * MAX_PDC_SAMPLES;
        Self {
            data: vec![0.0f32; total_samples].into_boxed_slice(),
            scratch: Box::new([[0.0f32; ipc_layer::MAX_BLOCK_SIZE]; crate::MAX_CHANNELS]),
        }
    }

    #[inline(always)]
    pub fn get_sample(&self, node_idx: usize, ch_idx: usize, pos: usize) -> f32 {
        let idx = (node_idx * crate::MAX_CHANNELS * MAX_PDC_SAMPLES) + (ch_idx * MAX_PDC_SAMPLES) + pos;
        self.data[idx]
    }

    /// Reads with 4-point Lagrange interpolation for fractional sub-sample PDC.
    #[inline(always)]
    pub fn get_sample_interpolated(&self, node_idx: usize, ch_idx: usize, pos_integral: usize, frac: f32) -> f32 {
        let max_len = MAX_PDC_SAMPLES;
        let p1 = pos_integral;
        let p0 = if p1 == 0 { max_len - 1 } else { p1 - 1 };
        let p2 = (p1 + 1) % max_len;
        let p3 = (p1 + 2) % max_len;

        let a = self.get_sample(node_idx, ch_idx, p0);
        let b = self.get_sample(node_idx, ch_idx, p1);
        let c = self.get_sample(node_idx, ch_idx, p2);
        let d = self.get_sample(node_idx, ch_idx, p3);

        let c0 = b;
        let c1 = c - (1.0/3.0)*a - 0.5*b - (1.0/6.0)*d;
        let c2 = 0.5*(a + c) - b;
        let c3 = (1.0/6.0)*(d - a) + 0.5*(b - c);

        c3*frac*frac*frac + c2*frac*frac + c1*frac + c0
    }

    #[inline(always)]
    pub fn set_sample(&mut self, node_idx: usize, ch_idx: usize, pos: usize, val: f32) {
        let idx = (node_idx * crate::MAX_CHANNELS * MAX_PDC_SAMPLES) + (ch_idx * MAX_PDC_SAMPLES) + pos;
        self.data[idx] = val;
    }
}

/// An engine-internal graph buffer, sized by the render CAPACITY.
///
/// Deliberately not [`AudioBlock`]. `AudioBlock` is the shared-memory protocol
/// unit exchanged with sidecars, fixed at `IPC_BLOCK_SIZE` so that SHM ring
/// bandwidth does not scale with the render block. Using it for the graph pool
/// silently pinned the graph's per-call render capacity to the *IPC* size:
/// every backend chunked to `MAX_BLOCK_SIZE` (1024) on the strength of that
/// constant's documented meaning, so any device period above `IPC_BLOCK_SIZE`
/// indexed `data[offset..offset + 1024]` on a 256-element array and panicked —
/// on the SCHED_FIFO audio thread, where a panic is an instant hard stop.
///
/// Keeping the two types distinct is what makes `MAX_BLOCK_SIZE` mean what it
/// says: engine-internal buffers are cheap to grow, protocol buffers are not.
#[derive(Clone, Copy)]
#[repr(C, align(64))]
pub struct RenderBlock {
    pub data: [f32; ipc_layer::MAX_BLOCK_SIZE],
    pub len: u32,
}

impl RenderBlock {
    pub const fn silent() -> Self {
        Self { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0 }
    }
}

impl Default for RenderBlock {
    fn default() -> Self { Self::silent() }
}

pub struct GraphBufferPool {
    pub(crate) buffers: Box<[RenderBlock; crate::MAX_BUFFERS]>,
    pub(crate) crossfade_buffers: Box<[RenderBlock; crate::MAX_CROSSFADE_BUFFERS]>,
    pub(crate) old_path_buffers: Box<[RenderBlock; crate::MAX_BUFFERS]>,
    pub(crate) pdc_lines: Option<PdcLines>,
    pub(crate) pdc_write_pos: usize,
}

impl GraphBufferPool {
    pub fn new() -> Self {
        let empty_block = RenderBlock::silent();
        Self {
            buffers: Box::new([empty_block; crate::MAX_BUFFERS]),
            crossfade_buffers: Box::new([empty_block; crate::MAX_CROSSFADE_BUFFERS]),
            old_path_buffers: Box::new([empty_block; crate::MAX_BUFFERS]),
            // RT-Safety: Pre-allocate PDC lines in the Orchestration plane
            pdc_lines: Some(PdcLines::new()),
            pdc_write_pos: 0,
        }
    }

    /// Snapshot the live buffers for crossfade morphing, copying only the
    /// frames this block actually rendered.
    ///
    /// `frames` matters on the RT path. This runs once per block for the whole
    /// duration of a crossfade, and copying the full buffer width copies
    /// whatever the *capacity* is rather than what is in use — at a 256-frame
    /// device period against a 1024-frame capacity that is 4x the memory
    /// traffic, three quarters of it stale zeroes. Bounded by capacity so an
    /// over-large `frames` clamps instead of panicking.
    pub fn capture_old_buffers(&mut self, frames: usize) {
        let n = frames.min(ipc_layer::MAX_BLOCK_SIZE);
        for i in 0..crate::MAX_BUFFERS {
            self.old_path_buffers[i].data[..n].copy_from_slice(&self.buffers[i].data[..n]);
        }
    }

    pub fn clear(&mut self) {
        let empty_block = RenderBlock::silent();
        self.buffers.fill(empty_block);
        self.crossfade_buffers.fill(empty_block);
        self.old_path_buffers.fill(empty_block);
    }
}

impl Default for GraphBufferPool {
    fn default() -> Self {
        Self::new()
    }
}
