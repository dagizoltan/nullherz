use std::process::{Command, Child};
use std::sync::Arc;
use ipc_layer::{SharedMemory, ShmRingBuffer, ShmSignal, EventFd, AudioBlock, move_to_cgroup};
use audio_core::{MAX_CHANNELS};
use nullherz_processors::SidecarProcessor;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidecarStatus {
    Starting,
    Running,
    Bypassing,
    Restarting,
    Crashed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FailurePolicy {
    Bypass,
    AutoRestart,
    SafeMode,
}

pub struct SidecarHandle {
    pub name: String,
    pub binary_path: String,
    pub num_channels: usize,
    pub process: Child,
    pub shm_cmd: Arc<SharedMemory>,
    pub shm_feedback: Arc<SharedMemory>,
    pub shm_inputs: Vec<Arc<SharedMemory>>,
    pub shm_outputs: Vec<Arc<SharedMemory>>,
    pub shm_signal: Arc<SharedMemory>,
    pub last_heartbeat: std::time::Instant,
    pub last_heartbeat_version: u64,
    pub status: SidecarStatus,
    pub restart_count: u32,
    pub failure_policy: FailurePolicy,
}

pub type SidecarManager = SidecarSupervisor;

#[derive(Default)]
pub struct SidecarSupervisor {
    active_sidecars: Vec<SidecarHandle>,
    pub current_memory_usage_bytes: usize,
}

const MAX_SIDECAR_MEMORY_BYTES: usize = 512 * 1024 * 1024; // 512MB Quota

impl SidecarSupervisor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn_sidecar(&mut self, name: &str, binary_path: &str, num_channels: usize) -> Result<SidecarProcessor, String> {
        let num_channels = num_channels.min(MAX_CHANNELS);

        // Calculate estimated memory usage for this sidecar
        let (cmd_layout, _) = ShmRingBuffer::<control_plane::Command>::layout(64);
        let (fb_layout, _) = ShmRingBuffer::<control_plane::SidecarMetadata>::layout(8);
        let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(16);
        let sig_size = std::mem::size_of::<ShmSignal>();

        let estimated_size = cmd_layout.size() + fb_layout.size() + (audio_layout.size() * num_channels * 2) + sig_size;

        if self.current_memory_usage_bytes + estimated_size > MAX_SIDECAR_MEMORY_BYTES {
            return Err(format!("Sidecar '{}' exceeds system memory quota. (Current: {}MB, Requested: {}MB, Limit: {}MB)",
                name, self.current_memory_usage_bytes / 1024 / 1024, estimated_size / 1024 / 1024, MAX_SIDECAR_MEMORY_BYTES / 1024 / 1024));
        }

        // 1. Create SHM for commands
        let cmd_shm_name = format!("/nullherz_cmd_{}", name);
        let (cmd_layout, _) = ShmRingBuffer::<control_plane::Command>::layout(64);
        let shm_cmd = SharedMemory::create(&cmd_shm_name, cmd_layout.size()).map_err(|e| e.to_string())?;
        let cmd_rb_ptr = unsafe { ShmRingBuffer::init(shm_cmd.ptr(), 64) };
        let shm_cmd = Arc::new(shm_cmd);

        // 1b. Create SHM for feedback
        let fb_shm_name = format!("/nullherz_fb_{}", name);
        let (fb_layout, _) = ShmRingBuffer::<control_plane::SidecarMetadata>::layout(8);
        let shm_feedback = SharedMemory::create(&fb_shm_name, fb_layout.size()).map_err(|e| e.to_string())?;
        let fb_rb_ptr = unsafe { ShmRingBuffer::init(shm_feedback.ptr(), 8) };
        let shm_feedback = Arc::new(shm_feedback);

        // 2. Create SHM for audio blocks
        let mut shm_inputs = Vec::new();
        let mut input_ptrs = Vec::new();
        let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(16);
        for i in 0..num_channels {
            let in_name = format!("/nullherz_in_{}_{}", name, i);
            let shm = SharedMemory::create(&in_name, audio_layout.size()).map_err(|e| e.to_string())?;
            input_ptrs.push(unsafe { ShmRingBuffer::init(shm.ptr(), 16) });
            shm_inputs.push(Arc::new(shm));
        }

        let mut shm_outputs = Vec::new();
        let mut output_ptrs = Vec::new();
        for i in 0..num_channels {
            let out_name = format!("/nullherz_out_{}_{}", name, i);
            let shm = SharedMemory::create(&out_name, audio_layout.size()).map_err(|e| e.to_string())?;
            output_ptrs.push(unsafe { ShmRingBuffer::init(shm.ptr(), 16) } as *const ShmRingBuffer<AudioBlock>);
            shm_outputs.push(Arc::new(shm));
        }

        // 3. Create SHM for signal
        let sig_name = format!("/nullherz_sig_{}", name);
        let shm_signal = SharedMemory::create(&sig_name, std::mem::size_of::<ShmSignal>()).map_err(|e| e.to_string())?;
        let signal_ptr = shm_signal.ptr() as *mut ShmSignal;
        unsafe { std::ptr::write(signal_ptr, ShmSignal::new()); }
        let shm_signal = Arc::new(shm_signal);

        // 4. Create EventFd
        let efd = EventFd::create().map_err(|e| e.to_string())?;
        let efd_raw = efd.fd();

        // 5. Spawn process
        let mut cmd = Command::new(binary_path);
        #[cfg(unix)]
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(move || {
                let flags = libc::fcntl(efd_raw, libc::F_GETFD);
                if flags != -1 {
                    libc::fcntl(efd_raw, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
                }
                Ok(())
            });
        }
        cmd.arg("--command-shm").arg(&cmd_shm_name)
           .arg("--feedback-shm").arg(&fb_shm_name)
           .arg("--channels").arg(num_channels.to_string())
           .arg("--signal-shm").arg(&sig_name)
           .arg("--event-fd").arg(efd_raw.to_string());

        for i in 0..num_channels {
            cmd.arg("--input-shm").arg(format!("/nullherz_in_{}_{}", name, i));
            cmd.arg("--output-shm").arg(format!("/nullherz_out_{}_{}", name, i));
        }

        let child = cmd.spawn()
            .map_err(|e| e.to_string())?;

        // Move to high-priority Cgroup and set RT priority
        if let Err(e) = move_to_cgroup("nullherz", child.id() as i32) {
            eprintln!("Warning: could not move sidecar {} to cgroup: {}", name, e);
        }
        if let Err(e) = ipc_layer::set_rt_priority_for(child.id() as i32, 80) {
            eprintln!("Warning: could not set RT priority for sidecar {}: {}", name, e);
        }

        let mut processor = unsafe {
            SidecarProcessor::new(
                cmd_rb_ptr,
                Some(fb_rb_ptr),
                &input_ptrs,
                &output_ptrs,
                signal_ptr,
                Some(efd)
            )
        };
        processor.set_shm_references(
            shm_cmd.clone(),
            Some(shm_feedback.clone()),
            shm_inputs.clone(),
            shm_outputs.clone(),
            shm_signal.clone(),
        );

        self.active_sidecars.push(SidecarHandle {
            name: name.to_string(),
            binary_path: binary_path.to_string(),
            num_channels,
            process: child,
            shm_cmd,
            shm_feedback,
            shm_inputs,
            shm_outputs,
            shm_signal,
            last_heartbeat: std::time::Instant::now(),
            last_heartbeat_version: 0,
            status: SidecarStatus::Running,
            restart_count: 0,
            failure_policy: FailurePolicy::AutoRestart,
        });

        self.current_memory_usage_bytes += estimated_size;

        Ok(processor)
    }

    pub fn supervise(&mut self) -> (Vec<SidecarProcessor>, bool) {
        let mut to_restart = Vec::new();
        let mut enter_safe_mode = false;

        let (cmd_layout, _) = ShmRingBuffer::<control_plane::Command>::layout(64);
        let (fb_layout, _) = ShmRingBuffer::<control_plane::SidecarMetadata>::layout(8);
        let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(16);
        let sig_size = std::mem::size_of::<ShmSignal>();

        self.active_sidecars.retain_mut(|handle| {
            if handle.status == SidecarStatus::Bypassing || handle.status == SidecarStatus::Crashed {
                return true; // Already handled, keep in list but don't process
            }

            let exited = match handle.process.try_wait() {
                Ok(Some(_status)) => true,
                Ok(None) => false,
                Err(_) => true,
            };

            let timed_out = handle.last_heartbeat.elapsed() > std::time::Duration::from_secs(5);

            if exited || timed_out {
                if timed_out { let _ = handle.process.kill(); }

                match handle.failure_policy {
                    FailurePolicy::Bypass => {
                        handle.status = SidecarStatus::Bypassing;
                        return true;
                    }
                    FailurePolicy::AutoRestart => {
                        if handle.restart_count < 5 {
                            handle.status = SidecarStatus::Restarting;
                            let size = cmd_layout.size() + fb_layout.size() + (audio_layout.size() * handle.num_channels * 2) + sig_size;
                            self.current_memory_usage_bytes = self.current_memory_usage_bytes.saturating_sub(size);
                            to_restart.push((handle.name.clone(), handle.binary_path.clone(), handle.num_channels, handle.restart_count + 1));
                            return false;
                        } else {
                            handle.status = SidecarStatus::Crashed;
                        }
                    }
                    FailurePolicy::SafeMode => {
                        enter_safe_mode = true;
                    }
                }
            }

            // Check heartbeat from SHM
            let sig_ptr = handle.shm_signal.ptr() as *const ShmSignal;
            let current_heartbeat = unsafe { (*sig_ptr).get_heartbeat() };
            if current_heartbeat != handle.last_heartbeat_version {
                handle.last_heartbeat = std::time::Instant::now();
                handle.last_heartbeat_version = current_heartbeat;
            }

            true
        });

        let mut new_processors = Vec::new();
        for (name, path, channels, restarts) in to_restart {
            if let Ok(p) = self.spawn_sidecar(&name, &path, channels) {
                if let Some(h) = self.active_sidecars.last_mut() {
                    h.restart_count = restarts;
                }
                new_processors.push(p);
            }
        }
        (new_processors, enter_safe_mode)
    }

    pub fn reap_zombies(&mut self) -> Vec<SidecarProcessor> {
        let (procs, _) = self.supervise();
        procs
    }

    pub fn update_heartbeat(&mut self, sidecar_idx: usize) {
        if let Some(handle) = self.active_sidecars.get_mut(sidecar_idx) {
            handle.last_heartbeat = std::time::Instant::now();
        }
    }
}

impl Drop for SidecarSupervisor {
    fn drop(&mut self) {
        for handle in self.active_sidecars.iter_mut() {
            let _ = handle.process.kill();
            let _ = handle.process.wait();
        }
    }
}

#[derive(Default)]
pub struct SidecarRegistry {
    pub known_binaries: Vec<String>,
}

impl SidecarRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scan_directory(&mut self, path: &str) -> Result<(), String> {
        let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if let Some(s) = path.to_str().filter(|_| path.is_file()) {
                self.known_binaries.push(s.to_string());
            }
        }
        Ok(())
    }
}
