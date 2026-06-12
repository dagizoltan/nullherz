use std::sync::atomic::AtomicU64;

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
