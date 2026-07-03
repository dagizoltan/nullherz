pub mod resource_limiter;
pub mod protocol;

use std::process::{Command, Child};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use ipc_layer::{SharedMemory, ShmRingBuffer, ShmSignal, EventFd, AudioBlock, move_to_cgroup};
use nullherz_traits::{AudioProcessor, MAX_CHANNELS};
use crate::resource_limiter::ResourceLimiter;

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
    pub node_idx: u32,
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
    pub last_oom_events: u64,
}

pub type SidecarManager = SidecarSupervisor;

pub struct SidecarSupervisor {
    active_sidecars: Vec<SidecarHandle>,
    pub limiter: ResourceLimiter,
}

impl Default for SidecarSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl SidecarSupervisor {
    pub fn new() -> Self {
        Self {
            active_sidecars: Vec::new(),
            limiter: ResourceLimiter::default(),
        }
    }

    pub fn spawn_sidecar(&mut self, name: &str, binary_path: &str, node_idx: u32, num_channels: usize, failure_policy: FailurePolicy) -> Result<Box<dyn AudioProcessor>, String> {
        let num_channels = num_channels.min(MAX_CHANNELS);

        let estimated_size = self.limiter.check_and_reserve(name, num_channels)?;

        let result = self.spawn_sidecar_internal(name, binary_path, node_idx, num_channels, failure_policy, estimated_size);

        if result.is_err() {
            self.limiter.release(estimated_size);
        }

        result
    }

    fn spawn_sidecar_internal(&mut self, name: &str, binary_path: &str, node_idx: u32, num_channels: usize, failure_policy: FailurePolicy, estimated_size: usize) -> Result<Box<dyn AudioProcessor>, String> {
        // 1. Create SHM for commands
        let cmd_shm_name = format!("/nullherz_cmd_{}", name);
        let (cmd_layout, _) = ShmRingBuffer::<nullherz_traits::Command>::layout(64);
        let shm_cmd = SharedMemory::create(&cmd_shm_name, cmd_layout.size()).map_err(|e| e.to_string())?;
        let cmd_rb_ptr = unsafe { ShmRingBuffer::init(shm_cmd.ptr(), 64) };
        let shm_cmd = Arc::new(shm_cmd);

        // 1b. Create SHM for feedback
        let fb_shm_name = format!("/nullherz_fb_{}", name);
        let (fb_layout, _) = ShmRingBuffer::<nullherz_traits::ProcessorMetadata>::layout(8);
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

        // Perform Handshake
        // In a real implementation, we would wait for the sidecar to write its handshake to SHM.
        // For this hardening step, we initialize the signal with our version.
        unsafe {
            let sig_ptr = shm_signal.ptr() as *mut ShmSignal;
            (*sig_ptr).heartbeat.store(protocol::SIDECAR_PROTOCOL_VERSION, Ordering::Release);
        }

        // Move to high-priority Cgroup and set RT priority
        if let Err(e) = move_to_cgroup("nullherz", child.id() as i32) {
            eprintln!("Warning: could not move sidecar {} to cgroup: {}", name, e);
        }

        // SC-4: Enforce real memory limits via hierarchical cgroups
        // Create a specific group for this sidecar instance
        let group_name = format!("nullherz/sidecar_{}", name);
        if let Err(e) = move_to_cgroup(&group_name, child.id() as i32) {
             eprintln!("Warning: could not move sidecar {} to cgroup {}: {}", name, group_name, e);
        }

        // We'll set a limit of 1.5x the estimated IPC memory or 16MB minimum.
        let rss_limit = (estimated_size * 3 / 2).max(16 * 1024 * 1024);
        if let Err(e) = ipc_layer::set_cgroup_memory_limit(&group_name, rss_limit) {
             eprintln!("Warning: could not set cgroup memory limit for sidecar {}: {}", name, e);
        }
        if let Err(e) = ipc_layer::set_rt_priority_for(child.id() as i32, 80) {
            eprintln!("Warning: could not set RT priority for sidecar {}: {}", name, e);
        }

        let mut sidecar = unsafe {
            nullherz_processors::SidecarProcessor::new(
                cmd_rb_ptr,
                Some(fb_rb_ptr),
                &input_ptrs,
                &output_ptrs,
                signal_ptr,
                Some(efd)
            )
        };
        sidecar.set_shm_references(
            shm_cmd.clone(),
            Some(shm_feedback.clone()),
            shm_inputs.clone(),
            shm_outputs.clone(),
            shm_signal.clone(),
        );

        self.active_sidecars.push(SidecarHandle {
            name: name.to_string(),
            binary_path: binary_path.to_string(),
            node_idx,
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
            failure_policy,
            last_oom_events: 0,
        });

        Ok(Box::new(sidecar))
    }

    pub fn supervise(&mut self) -> (Vec<(u32, Box<dyn AudioProcessor>)>, bool) {
        let mut to_restart = Vec::new();
        let mut enter_safe_mode = false;

        // SC-3: Fix resource quota leak by releasing early if needed.
        // Actually, retain_mut is fine but we must ensure release on failure.

        self.active_sidecars.retain_mut(|handle| {
            if handle.status == SidecarStatus::Bypassing || handle.status == SidecarStatus::Crashed {
                return true; // Already handled, keep in list but don't process
            }

            let exited = match handle.process.try_wait() {
                Ok(Some(status)) => {
                    // Check if it was killed by OOM killer (cgroups)
                    // On Linux, a status of 9 (SIGKILL) might indicate OOM or manual kill.
                    if status.code().is_none() {
                        eprintln!("Sidecar {} terminated by signal (Potential OOM/Cgroup kill)", handle.name);
                    }
                    true
                },
                Ok(None) => false,
                Err(_) => true,
            };

            let timed_out = handle.last_heartbeat.elapsed() > std::time::Duration::from_secs(5);

            // Check for OOM events via cgroups
            let oom_happened = handle.check_oom_events();

            if exited || timed_out || oom_happened {
                if timed_out { let _ = handle.process.kill(); }

                match handle.failure_policy {
                    FailurePolicy::Bypass => {
                        handle.status = SidecarStatus::Bypassing;
                        return true;
                    }
                    FailurePolicy::AutoRestart => {
                        handle.cleanup_cgroup();
                        if handle.restart_count < 5 {
                            handle.status = SidecarStatus::Restarting;
                            let size = self.limiter.estimate_sidecar_memory(handle.num_channels);
                            self.limiter.release(size);
                    to_restart.push((handle.name.clone(), handle.binary_path.clone(), handle.node_idx, handle.num_channels, handle.restart_count + 1, handle.failure_policy));
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
        for (name, path, node_idx, channels, restarts, policy) in to_restart {
            match self.spawn_sidecar(&name, &path, node_idx, channels, policy) {
                Ok(p) => {
                    if let Some(h) = self.active_sidecars.last_mut() {
                        h.restart_count = restarts;
                    }
                    new_processors.push((node_idx, p));
                }
                Err(e) => {
                    eprintln!("Failed to restart sidecar {}: {}", name, e);
                    // SC-3: limiter was reserved in spawn_sidecar and potentially not released
                    // if it failed *after* reservation.
                }
            }
        }
        (new_processors, enter_safe_mode)
    }

    pub fn reap_zombies(&mut self) -> Vec<(u32, Box<dyn AudioProcessor>)> {
        let (procs, _) = self.supervise();
        procs
    }

    pub fn update_heartbeat(&mut self, processor_idx: usize) {
        if let Some(handle) = self.active_sidecars.get_mut(processor_idx) {
            handle.last_heartbeat = std::time::Instant::now();
        }
    }

    pub fn list_stalled_nodes(&self) -> Vec<u32> {
        let mut stalled = Vec::new();
        for handle in &self.active_sidecars {
            if handle.status == SidecarStatus::Running {
                let timed_out = handle.last_heartbeat.elapsed() > std::time::Duration::from_millis(500);
                if timed_out {
                    stalled.push(handle.node_idx);
                }
            }
        }
        stalled
    }
}

impl SidecarHandle {
    pub fn cleanup_cgroup(&self) {
        let group_path = format!("/sys/fs/cgroup/nullherz/sidecar_{}", self.name);
        if std::path::Path::new(&group_path).exists() {
            let _ = std::fs::remove_dir(&group_path);
        }
    }

    pub fn check_oom_events(&mut self) -> bool {
        let group_path = format!("/sys/fs/cgroup/nullherz/sidecar_{}", self.name);
        let events_path = format!("{}/memory.events", group_path);

        if let Ok(content) = std::fs::read_to_string(&events_path) {
            for line in content.lines() {
                if line.starts_with("oom_kill ") {
                    if let Some(count_str) = line.split_whitespace().nth(1) {
                        if let Ok(count) = count_str.parse::<u64>() {
                            if count > self.last_oom_events {
                                self.last_oom_events = count;
                                eprintln!("Sidecar {} OOM event detected! (count: {})", self.name, count);
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Check for memory.high/max pressure
        let pressure_path = format!("{}/memory.pressure", group_path);
        if let Ok(content) = std::fs::read_to_string(&pressure_path) {
            // Memory pressure format: "some avg10=0.00 avg60=0.00 avg300=0.00 total=0"
            for line in content.lines() {
                if line.starts_with("some ") {
                    if let Some(avg10_part) = line.split_whitespace().find(|s| s.starts_with("avg10=")) {
                        if let Ok(val) = avg10_part[6..].parse::<f32>() {
                            if val > 50.0 {
                                eprintln!("Sidecar {} CRITICAL memory pressure detected: {}%", self.name, val);
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supervisor_memory_quota() {
        let mut supervisor = SidecarSupervisor::new();
        supervisor.limiter = ResourceLimiter::new(1024 * 1024); // 1MB limit

        // One sidecar with 16 channels should be around 700-800KB
        supervisor.limiter.check_and_reserve("prefill", 16).unwrap();

        // This should fail due to quota
        let result = supervisor.spawn_sidecar("test", "/usr/bin/true", 0, 16, FailurePolicy::AutoRestart);
        match result {
            Err(e) => assert!(e.contains("exceeds system memory quota")),
            _ => panic!("Should have failed with memory quota error"),
        }
    }

    #[test]
    fn test_supervisor_initial_state() {
        let supervisor = SidecarSupervisor::new();
        assert_eq!(supervisor.active_sidecars.len(), 0);
        assert_eq!(supervisor.limiter.current_usage(), 0);
    }
}

impl Drop for SidecarSupervisor {
    fn drop(&mut self) {
        for handle in self.active_sidecars.iter_mut() {
            let _ = handle.process.kill();
            let _ = handle.process.wait();
            handle.cleanup_cgroup();
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
