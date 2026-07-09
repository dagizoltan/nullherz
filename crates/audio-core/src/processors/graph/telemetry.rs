use std::sync::atomic::{AtomicU64, Ordering};
use ipc_layer::AudioBlock;
use crate::processors::graph::GraphTopology;

/// Encapsulates all real-time telemetry gathered during graph execution.
pub struct GraphTelemetry {
    /// Atomic cycle counts per node for performance profiling.
    pub node_times_cycles: [AtomicU64; crate::MAX_NODES],
    /// Atomic peak signal levels (f32 bits) per node for metering.
    pub peak_levels: [std::sync::atomic::AtomicU32; crate::MAX_NODES],
}

impl Default for GraphTelemetry {
    fn default() -> Self {
        Self {
            node_times_cycles: std::array::from_fn(|_| AtomicU64::new(0)),
            peak_levels: std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0)),
        }
    }
}

impl GraphTelemetry {
    pub fn update_peak_levels(
        &self,
        topo: &GraphTopology,
        buffers: &[AudioBlock; crate::MAX_NODES],
        offset: usize,
        num_samples: usize
    ) {
        for n_idx in 0..topo.node_count.min(crate::MAX_NODES) {
            let routing = &topo.routing[n_idx];
            let mut node_peak = if offset == 0 { 0.0f32 } else { f32::from_bits(self.peak_levels[n_idx].load(Ordering::Relaxed)) };

            for o_idx in 0..routing.output_count {
                let v_out = routing.output_indices.get(o_idx).copied().unwrap_or(0).min(crate::MAX_NODES as u32 - 1) as usize;
                let p_idx = topo.virtual_to_physical.get(v_out).copied().unwrap_or(0).min(crate::MAX_NODES as u32 - 1) as usize;
                let data = &buffers[p_idx].data[offset..offset + num_samples];

                use wide::*;
                let mut channel_peak_v = f32x8::ZERO;
                let mut i = 0;
                while i + 8 <= data.len() {
                    let v = f32x8::new(data[i..i+8].try_into().unwrap());
                    channel_peak_v = channel_peak_v.max(v.abs());
                    i += 8;
                }
                let arr: [f32; 8] = channel_peak_v.into();
                let mut channel_peak = arr.iter().fold(0.0f32, |m, &x| m.max(x));
                while i < data.len() {
                    let abs = data[i].abs();
                    if abs > channel_peak { channel_peak = abs; }
                    i += 1;
                }
                if channel_peak > node_peak { node_peak = channel_peak; }
            }
            let current_bits = self.peak_levels[n_idx].load(Ordering::Relaxed);
            if node_peak.to_bits() != current_bits {
                self.peak_levels[n_idx].store(node_peak.to_bits(), Ordering::Relaxed);
            }
        }
    }
}
