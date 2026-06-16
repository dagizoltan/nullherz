use ipc_layer::{ShmRingBuffer, AudioBlock, ShmSignal};

pub struct ResourceLimiter {
    max_memory_bytes: usize,
    current_memory_usage_bytes: usize,
}

impl Default for ResourceLimiter {
    fn default() -> Self {
        Self::new(512 * 1024 * 1024) // 512MB default
    }
}

impl ResourceLimiter {
    pub fn new(max_memory_bytes: usize) -> Self {
        Self {
            max_memory_bytes,
            current_memory_usage_bytes: 0,
        }
    }

    pub fn check_and_reserve(&mut self, name: &str, num_channels: usize) -> Result<usize, String> {
        let estimated_size = self.estimate_sidecar_memory(num_channels);

        if self.current_memory_usage_bytes + estimated_size > self.max_memory_bytes {
            return Err(format!("Sidecar '{}' exceeds system memory quota. (Current: {}MB, Requested: {}MB, Limit: {}MB)",
                name, self.current_memory_usage_bytes / 1024 / 1024, estimated_size / 1024 / 1024, self.max_memory_bytes / 1024 / 1024));
        }

        self.current_memory_usage_bytes += estimated_size;
        Ok(estimated_size)
    }

    pub fn release(&mut self, size_bytes: usize) {
        self.current_memory_usage_bytes = self.current_memory_usage_bytes.saturating_sub(size_bytes);
    }

    pub fn current_usage(&self) -> usize {
        self.current_memory_usage_bytes
    }

    pub fn estimate_sidecar_memory(&self, num_channels: usize) -> usize {
        let (cmd_layout, _) = ShmRingBuffer::<nullherz_traits::Command>::layout(64);
        let (fb_layout, _) = ShmRingBuffer::<nullherz_traits::SidecarMetadata>::layout(8);
        let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(16);
        let sig_size = std::mem::size_of::<ShmSignal>();

        cmd_layout.size() + fb_layout.size() + (audio_layout.size() * num_channels * 2) + sig_size
    }
}
