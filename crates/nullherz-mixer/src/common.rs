#[derive(Debug, Clone)]
pub struct MixerConfig {
    pub master_l: usize,
    pub master_r: usize,
    pub dj_a_l: usize,
    pub dj_a_r: usize,
    pub dj_b_l: usize,
    pub dj_b_r: usize,
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            master_l: 0,
            master_r: 1,
            dj_a_l: 2,
            dj_a_r: 3,
            dj_b_l: 4,
            dj_b_r: 5,
        }
    }
}

pub struct BufferAllocator {
    next_id: u32,
    max_id: u32,
}

impl BufferAllocator {
    pub fn new(start_id: u32, max_id: u32) -> Self {
        Self { next_id: start_id, max_id }
    }

    pub fn allocate(&mut self) -> Result<u32, String> {
        if self.next_id < self.max_id {
            let id = self.next_id;
            self.next_id += 1;
            Ok(id)
        } else {
            Err("Buffer allocation limit reached".into())
        }
    }
}
