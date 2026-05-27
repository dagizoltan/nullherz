use std::process::{Command, Child};
use ipc_layer::{SharedMemory, ShmRingBuffer, ShmSignal, EventFd, AudioBlock};
use audio_core::{SidecarProcessor, MAX_CHANNELS};

pub struct SidecarHandle {
    pub process: Child,
    pub shm_cmd: SharedMemory,
    pub shm_inputs: Vec<SharedMemory>,
    pub shm_outputs: Vec<SharedMemory>,
    pub shm_signal: SharedMemory,
}

pub struct SidecarManager {
    active_sidecars: Vec<SidecarHandle>,
}

impl SidecarManager {
    pub fn new() -> Self {
        Self { active_sidecars: Vec::new() }
    }

    pub fn spawn_sidecar(&mut self, name: &str, binary_path: &str, num_channels: usize) -> Result<SidecarProcessor, String> {
        let num_channels = num_channels.min(MAX_CHANNELS);

        // 1. Create SHM for commands
        let cmd_shm_name = format!("/nullherz_cmd_{}", name);
        let (cmd_layout, _) = ShmRingBuffer::<control_plane::Command>::layout(64);
        let shm_cmd = SharedMemory::create(&cmd_shm_name, cmd_layout.size())?;
        let cmd_rb_ptr = unsafe { ShmRingBuffer::init(shm_cmd.ptr(), 64) };

        // 2. Create SHM for audio blocks
        let mut shm_inputs = Vec::new();
        let mut input_ptrs = Vec::new();
        let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(16);
        for i in 0..num_channels {
            let in_name = format!("/nullherz_in_{}_{}", name, i);
            let shm = SharedMemory::create(&in_name, audio_layout.size())?;
            input_ptrs.push(unsafe { ShmRingBuffer::init(shm.ptr(), 16) });
            shm_inputs.push(shm);
        }

        let mut shm_outputs = Vec::new();
        let mut output_ptrs = Vec::new();
        for i in 0..num_channels {
            let out_name = format!("/nullherz_out_{}_{}", name, i);
            let shm = SharedMemory::create(&out_name, audio_layout.size())?;
            output_ptrs.push(unsafe { ShmRingBuffer::init(shm.ptr(), 16) } as *const ShmRingBuffer<AudioBlock>);
            shm_outputs.push(shm);
        }

        // 3. Create SHM for signal
        let sig_name = format!("/nullherz_sig_{}", name);
        let shm_signal = SharedMemory::create(&sig_name, std::mem::size_of::<ShmSignal>())?;
        let signal_ptr = shm_signal.ptr() as *mut ShmSignal;
        unsafe { std::ptr::write(signal_ptr, ShmSignal::new()); }

        // 4. Create EventFd
        let efd = EventFd::create()?;
        let efd_raw = efd.fd();

        // 5. Spawn process
        let mut cmd = Command::new(binary_path);
        cmd.arg("--command-shm").arg(&cmd_shm_name)
           .arg("--channels").arg(num_channels.to_string())
           .arg("--signal-shm").arg(&sig_name)
           .arg("--event-fd").arg(efd_raw.to_string());

        for i in 0..num_channels {
            cmd.arg("--input-shm").arg(format!("/nullherz_in_{}_{}", name, i));
            cmd.arg("--output-shm").arg(format!("/nullherz_out_{}_{}", name, i));
        }

        let child = cmd.spawn()
            .map_err(|e| e.to_string())?;

        let processor = unsafe {
            SidecarProcessor::new(
                cmd_rb_ptr,
                &input_ptrs,
                &output_ptrs,
                signal_ptr,
                Some(efd)
            )
        };

        self.active_sidecars.push(SidecarHandle {
            process: child,
            shm_cmd,
            shm_inputs,
            shm_outputs,
            shm_signal,
        });

        Ok(processor)
    }

    pub fn reap_zombies(&mut self) {
        self.active_sidecars.retain_mut(|handle| {
            match handle.process.try_wait() {
                Ok(Some(_status)) => false, // Process exited, remove handle
                Ok(None) => true,           // Still running
                Err(_) => false,            // Error, assume gone
            }
        });
    }
}
