#![deny(clippy::disallowed_methods, clippy::disallowed_types)]
pub mod processors;
pub mod engine;
pub mod rt_logging;

#[cfg(test)]
mod engine_tests;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod decoupling_tests;

pub use engine::AudioEngine;
pub use processors::{AudioProcessor, ProcessorGraph, ProcessorNode, GraphTopology, NodeRouting, CrossfadeState};
pub use nullherz_traits::telemetry::Telemetry;
pub use nullherz_traits::error::AudioError;

pub use nullherz_traits::{
    AudioConfig, Transport, MAX_BUFFERS, MAX_CHANNELS, MAX_NODES,
    MAX_CROSSFADE_BUFFERS, MAX_MUTATIONS, DEFAULT_WORKER_COUNT, MAX_COMMANDS_PER_BLOCK
};

#[inline(always)]
pub fn get_cycles() -> u64 {
    #[cfg(target_arch = "x86_64")]
    { unsafe { std::arch::x86_64::_rdtsc() } }
    #[cfg(target_arch = "aarch64")]
    {
        unsafe {
            let val: u64;
            std::arch::asm!("mrs {}, cntvct_el0", out(reg) val, options(nomem, nostack));
            val
        }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        // Fallback for non-x86/ARM platforms using a monotonic clock if possible.
        // Since this is for telemetry/calibration, resolution matters.
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        { 0 }
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        {
            static BASELINE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
            let start = BASELINE.get_or_init(std::time::Instant::now);
            start.elapsed().as_nanos() as u64
        }
    }
}

#[macro_export]
macro_rules! assert_finite_block {
    ($block:expr, $node_idx:expr) => {
        #[cfg(debug_assertions)]
        {
            for (i, sample) in $block.iter().enumerate() {
                if !sample.is_finite() {
                    // RT-9: Safety Violation. eprintln! takes a lock.
                    // In a production RT thread, we must use RtLogger.
                    // However, since this is a panic-inducing debug check,
                    // we accept the jitter to provide diagnostic info before the crash.
                    eprintln!(
                        "RT-FATAL: Non-finite sample detected at Node {}, offset {}: {}",
                        $node_idx, i, sample
                    );
                    panic!("Non-finite sample in RT path");
                }
            }
        }
    };
}
