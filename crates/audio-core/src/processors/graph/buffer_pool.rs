use ipc_layer::AudioBlock;

pub struct GraphBufferPool {
    pub(crate) buffers: Box<[AudioBlock; crate::MAX_NODES]>,
    pub(crate) crossfade_buffers: [AudioBlock; crate::MAX_CROSSFADE_BUFFERS],
    pub(crate) old_path_buffers: Box<[AudioBlock; crate::MAX_NODES]>,
}

impl GraphBufferPool {
    pub fn new() -> Self {
        let empty_block = AudioBlock { data: [0.0f32; ipc_layer::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] };
        Self {
            buffers: Box::new([empty_block; crate::MAX_NODES]),
            crossfade_buffers: [empty_block; crate::MAX_CROSSFADE_BUFFERS],
            old_path_buffers: Box::new([empty_block; crate::MAX_NODES]),
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
