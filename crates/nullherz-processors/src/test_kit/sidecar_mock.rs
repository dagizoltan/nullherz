use ipc_layer::{SharedMemory, ShmRingBuffer, ShmSignal, AudioBlock};
use crate::sidecar::SidecarProcessor;

pub struct SidecarMockHost {
    pub cmd_shm: SharedMemory,
    pub sig_shm: SharedMemory,
    pub in_shm: Vec<SharedMemory>,
    pub out_shm: Vec<SharedMemory>,
}

impl SidecarMockHost {
    pub fn new(name: &str, num_channels: usize) -> Self {
        let cmd_shm = SharedMemory::create(&format!("mock_cmd_{}", name), 4096).unwrap();
        let sig_shm = SharedMemory::create(&format!("mock_sig_{}", name), 4096).unwrap();

        let mut in_shm = Vec::new();
        let mut out_shm = Vec::new();

        for i in 0..num_channels {
            in_shm.push(SharedMemory::create(&format!("mock_in_{}_{}", name, i), 16384).unwrap());
            out_shm.push(SharedMemory::create(&format!("mock_out_{}_{}", name, i), 16384).unwrap());
        }

        Self { cmd_shm, sig_shm, in_shm, out_shm }
    }

    pub fn create_processor(&self) -> SidecarProcessor {
        unsafe {
            let cmd_ptr = ShmRingBuffer::<nullherz_traits::Command>::init(self.cmd_shm.ptr(), 16);
            let sig_ptr = self.sig_shm.ptr() as *mut ShmSignal;
            std::ptr::write(sig_ptr, ShmSignal::new());

            let mut in_ptrs = Vec::new();
            let mut out_ptrs = Vec::new();

            for i in 0..self.in_shm.len() {
                in_ptrs.push(ShmRingBuffer::<AudioBlock>::init(self.in_shm[i].ptr(), 16));
                out_ptrs.push(ShmRingBuffer::<AudioBlock>::init(self.out_shm[i].ptr(), 16) as *const _);
            }

            SidecarProcessor::new(
                cmd_ptr,
                None,
                &in_ptrs,
                &out_ptrs,
                sig_ptr,
                None
            )
        }
    }

    pub fn simulate_sidecar_response(&mut self) {
        let sig_ptr = self.sig_shm.ptr() as *mut ShmSignal;
        unsafe { (*sig_ptr).pulse_heartbeat(); }

        for i in 0..self.in_shm.len() {
            unsafe {
                let in_rb = self.in_shm[i].ptr() as *mut ShmRingBuffer<AudioBlock>;
                let out_rb = self.out_shm[i].ptr() as *mut ShmRingBuffer<AudioBlock>;
                if let Some(block) = (*in_rb).pop() {
                    let _ = (*out_rb).push(block);
                }
            }
        }
    }
}
