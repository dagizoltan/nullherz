use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct EngineMetrics {
    pub calibration_start_instant: Arc<std::sync::Mutex<Option<Instant>>>,
    pub calibration_start_cycles: AtomicU64,
    pub ns_per_cycle: Arc<AtomicU64>,
    pub peak_ns: AtomicU64,
    pub resource_leaks: AtomicU64,
}

impl EngineMetrics {
    pub fn new() -> Self {
        Self {
            calibration_start_instant: Arc::new(std::sync::Mutex::new(None)),
            calibration_start_cycles: AtomicU64::new(0),
            ns_per_cycle: Arc::new(AtomicU64::new((1.0f64).to_bits())),
            peak_ns: AtomicU64::new(0),
            resource_leaks: AtomicU64::new(0),
        }
    }

    pub fn calibrate(&self, current_sample: u64, num_samples: usize) {
        let ns_bits = self.ns_per_cycle.load(Ordering::Acquire);
        if f64::from_bits(ns_bits) == 1.0 {
            let mut start_lock = self.calibration_start_instant.lock().unwrap();
            if start_lock.is_none() {
                *start_lock = Some(Instant::now());
                self.calibration_start_cycles.store(crate::get_cycles(), Ordering::Relaxed);
            } else if current_sample.is_multiple_of(num_samples as u64 * 1024) {
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
    }

    pub fn report_resource_leak(&self, health_signal: &std::sync::atomic::AtomicBool) {
        self.resource_leaks.fetch_add(1, Ordering::Relaxed);
        health_signal.store(true, Ordering::Relaxed);
    }

    pub fn update_peak(&self, current_ns: u64, sample_counter: u64, num_samples: usize) -> u64 {
        let peak = crate::telemetry::TelemetryProcessor::update_peak(&self.peak_ns, current_ns);
        if sample_counter.is_multiple_of(num_samples as u64 * 1024) {
            self.peak_ns.store(current_ns, Ordering::Relaxed);
        }
        peak
    }
}
