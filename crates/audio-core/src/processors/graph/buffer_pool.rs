use ipc_layer::AudioBlock;

pub const MAX_PDC_SAMPLES: usize = 4096;

/// Circular delay lines for PDC alignment.
pub struct PdcLines {
    pub data: Box<[f32]>,
}

impl PdcLines {
    pub fn new() -> Self {
        let total_samples = crate::MAX_NODES * crate::MAX_CHANNELS * MAX_PDC_SAMPLES;
        Self {
            data: vec![0.0f32; total_samples].into_boxed_slice(),
        }
    }

    #[inline(always)]
    pub fn get_sample(&self, node_idx: usize, ch_idx: usize, pos: usize) -> f32 {
        let idx = (node_idx * crate::MAX_CHANNELS * MAX_PDC_SAMPLES) + (ch_idx * MAX_PDC_SAMPLES) + pos;
        self.data[idx]
    }

    #[inline(always)]
    pub fn set_sample(&mut self, node_idx: usize, ch_idx: usize, pos: usize, val: f32) {
        let idx = (node_idx * crate::MAX_CHANNELS * MAX_PDC_SAMPLES) + (ch_idx * MAX_PDC_SAMPLES) + pos;
        self.data[idx] = val;
    }
}

pub struct GraphBufferPool {
    pub(crate) buffers: Box<[AudioBlock; crate::MAX_NODES]>,
    pub(crate) crossfade_buffers: Box<[AudioBlock; crate::MAX_CROSSFADE_BUFFERS]>,
    pub(crate) old_path_buffers: Box<[AudioBlock; crate::MAX_NODES]>,
    pub(crate) pdc_lines: Option<PdcLines>,
    pub(crate) pdc_write_pos: usize,
}

impl GraphBufferPool {
    pub fn new() -> Self {
        let empty_block = AudioBlock { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] };
        Self {
            buffers: Box::new([empty_block; crate::MAX_NODES]),
            crossfade_buffers: Box::new([empty_block; crate::MAX_CROSSFADE_BUFFERS]),
            old_path_buffers: Box::new([empty_block; crate::MAX_NODES]),
            // RT-Safety: Pre-allocate PDC lines in the Orchestration plane
            pdc_lines: Some(PdcLines::new()),
            pdc_write_pos: 0,
        }
    }

    pub fn capture_old_buffers(&mut self) {
        for i in 0..crate::MAX_NODES {
            self.old_path_buffers[i].data.copy_from_slice(&self.buffers[i].data);
        }
    }

    pub fn clear(&mut self) {
        let empty_block = AudioBlock { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] };
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
