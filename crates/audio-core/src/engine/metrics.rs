#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct EngineMetrics {
    pub calibration_start_instant: Arc<std::sync::Mutex<Option<Instant>>>,
    pub calibration_start_cycles: AtomicU64,
    pub ns_per_cycle: Arc<AtomicU64>,
    pub peak_ns: AtomicU64,
    pub node_peak_cycles: [AtomicU64; 64],
    pub resource_leaks: AtomicU64,
    pub last_xrun_magnitude_ns: AtomicU64,
    /// Sliding window of block processing times for predictive X-RUN mitigation.
    pub processing_history_ns: [AtomicU64; 16],
    pub history_idx: AtomicU64,
}

impl Default for EngineMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineMetrics {
    pub fn new() -> Self {
        Self {
            calibration_start_instant: Arc::new(std::sync::Mutex::new(None)),
            calibration_start_cycles: AtomicU64::new(0),
            ns_per_cycle: Arc::new(AtomicU64::new((1.0f64).to_bits())),
            peak_ns: AtomicU64::new(0),
            node_peak_cycles: std::array::from_fn(|_| AtomicU64::new(0)),
            resource_leaks: AtomicU64::new(0),
            last_xrun_magnitude_ns: AtomicU64::new(0),
            processing_history_ns: [
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
            ],
            history_idx: AtomicU64::new(0),
        }
    }

    pub fn update_peak(&self, current_ns: u64, sample_rate: f32, sample_counter: u64, num_samples: usize) -> u64 {
        // Detect X-RUN: If processing time exceeds block duration
        let block_duration_ns = (num_samples as f32 / sample_rate * 1_000_000_000.0) as u64;
        if current_ns > block_duration_ns {
            let magnitude = current_ns - block_duration_ns;
            self.last_xrun_magnitude_ns.store(magnitude, Ordering::Relaxed);
        }

        let ns_bits = self.ns_per_cycle.load(Ordering::Acquire);
        if f64::from_bits(ns_bits) == 1.0 {
            let mut start_lock = self.calibration_start_instant.lock().unwrap();
            if start_lock.is_none() {
                *start_lock = Some(Instant::now());
                self.calibration_start_cycles.store(crate::get_cycles(), Ordering::Relaxed);
            } else if sample_counter > 0 && sample_counter % (num_samples as u64 * 1024) == 0 {
                let start_inst = start_lock.unwrap();
                let elapsed = start_inst.elapsed().as_nanos() as f64;
                let start_c = self.calibration_start_cycles.load(Ordering::Relaxed);
                let elapsed_c = crate::get_cycles().wrapping_sub(start_c) as f64;
                if elapsed_c > 0.0 {
                    let ratio = elapsed / elapsed_c;
                    self.ns_per_cycle.store(ratio.to_bits(), Ordering::Release);
                }
            }
        }

        // Update sliding window history
        let idx = self.history_idx.fetch_add(1, Ordering::Relaxed) as usize % 16;
        self.processing_history_ns[idx].store(current_ns, Ordering::Relaxed);

        // Predictive Analysis: Check if the average of the last 4 blocks is trending upward
        let h_0 = self.processing_history_ns[idx].load(Ordering::Relaxed);
        let h_1 = self.processing_history_ns[(idx + 15) % 16].load(Ordering::Relaxed);
        let h_2 = self.processing_history_ns[(idx + 14) % 16].load(Ordering::Relaxed);
        let h_3 = self.processing_history_ns[(idx + 13) % 16].load(Ordering::Relaxed);

        let last_4_avg = (h_0 + h_1 + h_2 + h_3) / 4;
        if last_4_avg > (block_duration_ns * 9 / 10) {
             // Imminent X-RUN predicted (>90% CPU budget used)
             // Trigger internal telemetry pulse (Placeholder for Conductor feedback)
        }

        let peak = nullherz_traits::telemetry::TelemetryProcessor::update_peak(&self.peak_ns, current_ns);
        if sample_counter > 0 && sample_counter % (num_samples as u64 * 1024) == 0 {
            self.peak_ns.store(current_ns, Ordering::Relaxed);
        }
        peak
    }

    pub fn report_resource_leak(&self, health_signal: &std::sync::atomic::AtomicBool) {
        self.resource_leaks.fetch_add(1, Ordering::Relaxed);
        health_signal.store(true, Ordering::Relaxed);
    }
}
