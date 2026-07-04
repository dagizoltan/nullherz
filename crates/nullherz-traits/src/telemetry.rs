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
    pub last_xrun_magnitude_ns: u64,
    pub resource_leaks: u64,
    pub bpm: f32,
    pub beat_position: f64,
    #[serde(with = "BigArray")]
    pub node_times_ns: [u64; MAX_NODES],
    #[serde(with = "BigArray")]
    pub peak_levels: [f32; MAX_NODES],
    #[serde(with = "BigArray")]
    pub spectrum: [f32; 128],
    #[serde(with = "BigArray")]
    pub goniometer_pts: [f32; 128],
    #[serde(with = "BigArray")]
    pub dna_energy_map: [u8; 64],
    /// Row-wise active clip index (255 = none)
    pub active_clips: [u8; 8],
    /// Bitmask of clips in "Starting/Quantizing" state (Row per byte)
    pub starting_clips_mask: [u8; 8],
}

pub struct TelemetryProcessor;

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            process_time_ns: 0,
            peak_process_time_ns: 0,
            sample_counter: 0,
            xrun_count: 0,
            last_xrun_magnitude_ns: 0,
            resource_leaks: 0,
            bpm: 120.0,
            beat_position: 0.0,
            node_times_ns: [0; MAX_NODES],
            peak_levels: [0.0; MAX_NODES],
            spectrum: [0.0; 128],
            goniometer_pts: [0.0; 128],
            dna_energy_map: [0; 64],
            active_clips: [255; 8],
            starting_clips_mask: [0; 8],
        }
    }
}

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
