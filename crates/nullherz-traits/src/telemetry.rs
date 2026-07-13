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
    pub node_peak_times_ns: [u64; MAX_NODES],
    #[serde(with = "BigArray")]
    pub peak_levels: [f32; MAX_NODES],
    #[serde(with = "BigArray")]
    pub spectrum: [f32; 128],
    #[serde(with = "BigArray")]
    pub goniometer_pts: [f32; 128],
    pub dna_latent_space: [f32; 16],
    /// Row-wise active clip index (255 = none)
    pub active_clips: [u8; 8],
    /// Bitmask of clips in "Starting/Quantizing" state (Row per byte)
    pub starting_clips_mask: [u8; 8],
    /// System monotonic clock time in nanoseconds.
    pub system_time_ns: u64,
    /// Device-specific hardware clock time in nanoseconds.
    pub device_time_ns: u64,
    /// Estimated clock jitter in nanoseconds.
    pub clock_jitter_ns: u64,
    pub remote_node_count: u32,
    pub remote_cpu_usage: [f32; 8], // Support up to 8 remote nodes in telemetry
    pub remote_latency_ms: [f32; 8],
    pub calibration_samples: u32,
    pub sample_rate: f32,
    /// Proactive matchmaking suggestions: (Sample ID, Similarity Score)
    pub suggestions: [(u64, f32); 4],
    pub active_master_deck: char,
    /// Downsampled peak waveform data for 4 DJ decks for real-time visual feedback.
    #[serde(with = "BigArray")]
    pub waveform_peaks: [f32; 256],
    pub deck_positions: [u64; 4],
    pub deck_playback_rates: [f32; 4],
    /// Current mapping of well-known node names to indices.
    #[serde(with = "BigArray")]
    pub node_map_keys: [[u8; 32]; 32],
    #[serde(with = "BigArray")]
    pub node_map_values: [u32; 32],
    /// List of detected audio devices.
    pub audio_devices: [DeviceName; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeviceName {
    #[serde(with = "BigArray")]
    pub name: [u8; 64],
}

impl Default for DeviceName {
    fn default() -> Self {
        Self { name: [0u8; 64] }
    }
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
            node_peak_times_ns: [0; MAX_NODES],
            peak_levels: [0.0; MAX_NODES],
            spectrum: [0.0; 128],
            goniometer_pts: [0.0; 128],
            dna_latent_space: [0.0; 16],
            active_clips: [255; 8],
            starting_clips_mask: [0; 8],
            system_time_ns: 0,
            device_time_ns: 0,
            clock_jitter_ns: 0,
            remote_node_count: 0,
            remote_cpu_usage: [0.0; 8],
            remote_latency_ms: [0.0; 8],
            calibration_samples: 0,
            sample_rate: 44100.0,
            suggestions: [(0, 0.0); 4],
            active_master_deck: 'A',
            waveform_peaks: [0.0; 256],
            deck_positions: [0; 4],
            deck_playback_rates: [1.0; 4],
            node_map_keys: [[0u8; 32]; 32],
            node_map_values: [0u32; 32],
            audio_devices: [DeviceName::default(); 16],
        }
    }
}

impl Telemetry {
    /// Returns the interpolated beat position using the high-precision monotonic clock to eliminate UI playhead stuttering.
    pub fn get_interpolated_beat_position(&self) -> f64 {
        if self.bpm <= 0.0 || self.system_time_ns == 0 {
            return self.beat_position;
        }

        let now_ns = {
            static BASELINE: std::sync::OnceLock<(std::time::Instant, u64)> = std::sync::OnceLock::new();
            #[cfg(not(any(target_os = "windows", target_arch = "wasm32")))]
            let &(base_instant, base_ns) = BASELINE.get_or_init(|| {
                let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
                unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts); }
                let ns = (ts.tv_sec as u64 * 1_000_000_000) + ts.tv_nsec as u64;
                (std::time::Instant::now(), ns)
            });

            #[cfg(any(target_os = "windows", target_arch = "wasm32"))]
            let &(base_instant, base_ns) = BASELINE.get_or_init(|| {
                (std::time::Instant::now(), 0)
            });

            base_ns + base_instant.elapsed().as_nanos() as u64
        };

        if now_ns > self.system_time_ns {
            let elapsed_ns = now_ns - self.system_time_ns;
            let elapsed_sec = elapsed_ns as f64 / 1_000_000_000.0;
            // Cap interpolation to 1.0 second to prevent runaway drift if playback pauses/glitches
            if elapsed_sec < 1.0 {
                let elapsed_beats = elapsed_sec * (self.bpm as f64 / 60.0);
                return self.beat_position + elapsed_beats;
            }
        }
        self.beat_position
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
