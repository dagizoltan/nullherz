use std::sync::atomic::{AtomicU64, Ordering};
use serde_big_array::BigArray;
use serde::{Serialize, Deserialize};
use crate::MAX_NODES;

#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Telemetry {
    pub process_time_ns: u64,
    pub peak_process_time_ns: u64,
    pub sample_counter: u64,
    pub xrun_count: u32,
    pub resource_leaks: u64,
    #[serde(with = "BigArray")]
    pub node_times_ns: [u64; MAX_NODES],
    #[serde(with = "BigArray")]
    pub peak_levels: [f32; MAX_NODES],
    #[serde(with = "BigArray")]
    pub spectrum: [f32; 128],
    #[serde(with = "BigArray")]
    pub goniometer_pts: [f32; 128],
}

pub struct TelemetryProcessor;

impl TelemetryProcessor {
    pub fn collect_node_times(
        node_times_cycles: &[u64; MAX_NODES],
        ns_per_cycle: f64,
        node_times_ns: &mut [u64; MAX_NODES]
    ) {
        for (i, node_time) in node_times_ns.iter_mut().enumerate() {
            *node_time = (node_times_cycles[i] as f64 * ns_per_cycle) as u64;
        }
    }

    pub fn update_peak(peak_ns: &AtomicU64, current_ns: u64) -> u64 {
        let mut peak = peak_ns.load(Ordering::Relaxed);
        if current_ns > peak {
            let _ = peak_ns.compare_exchange(peak, current_ns, Ordering::Relaxed, Ordering::Relaxed);
            peak = current_ns;
        }
        peak
    }
}
