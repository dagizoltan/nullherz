pub mod backends;
pub mod processors;
pub mod engine;
pub mod telemetry;
pub mod error;

pub use engine::AudioEngine;
pub use processors::{AudioProcessor, ProcessorGraph, SidecarProcessor, ProcessorNode, GraphTopology, NodeRouting, CrossfadeState};
pub use backends::{AudioBackend, AlsaBackend, PipewireBackend, JackBackend, ThreadedBackend};
pub use telemetry::Telemetry;

#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: f32,
    pub block_size: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Transport {
    pub bpm: f32,
    pub beat_position: f64,
    pub is_playing: bool,
    pub sample_rate: f32,
}

pub const MAX_CHANNELS: usize = 16;

pub fn setup_rt_thread(priority: i32, cpu_id: Option<usize>) {
    thread_local! {
        static INITIALIZED: std::cell::Cell<bool> = std::cell::Cell::new(false);
    }

    if INITIALIZED.with(|i| i.get()) && cpu_id.is_none() {
        return;
    }

    let _ = ipc_layer::set_rt_priority(priority);

    #[cfg(target_os = "linux")]
    if let Some(id) = cpu_id {
        unsafe {
            let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
            libc::CPU_SET(id, &mut cpuset);
            libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &cpuset);
        }
    }

    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut mxcsr: u32 = 0;
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut mxcsr);
        // Enable Flush-to-Zero (bit 15) and Denormals-Are-Zero (bit 6)
        mxcsr |= 0x8000 | 0x0040;
        std::arch::asm!("ldmxcsr [{}]", in(reg) &mxcsr);
    }

    #[cfg(target_arch = "aarch64")]
    unsafe {
        let mut fpcr: u64;
        std::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
        // Bit 24 is FZ (Flush-to-Zero)
        fpcr |= 1 << 24;
        std::arch::asm!("msr fpcr, {}", in(reg) fpcr);
    }

    INITIALIZED.with(|i| i.set(true));
}
