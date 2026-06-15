use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use ipc_layer::{Producer, RingBuffer};
use nullherz_traits::telemetry::Telemetry;
use crate::engine::metrics::EngineMetrics;
use nullherz_traits::AudioProcessor;

pub struct TelemetryCollector {
    pub telemetry_producer: Producer<Telemetry>,
}

impl TelemetryCollector {
    pub fn new(telemetry_producer: Producer<Telemetry>) -> Self {
        Self { telemetry_producer }
    }

    pub fn finalize_block(
        &mut self,
        graph: &mut dyn AudioProcessor,
        metrics: &EngineMetrics,
        xrun_count_atomic: &Arc<AtomicU32>,
        sample_counter: &mut u64,
        start_cycles: u64,
        num_samples: usize,
    ) {
        let mut peak_levels = [0.0f32; 64];
        let mut node_times_cycles = [0u64; 64];

        graph.collect_telemetry(&mut node_times_cycles, &mut peak_levels);

        let ns_per_cycle = f64::from_bits(metrics.ns_per_cycle.load(Ordering::Relaxed));
        let mut node_times = [0u64; 64];
        nullherz_traits::telemetry::TelemetryProcessor::collect_node_times(
            unsafe { std::mem::transmute(&node_times_cycles) },
            ns_per_cycle,
            &mut node_times
        );

        let elapsed_cycles = crate::get_cycles().wrapping_sub(start_cycles);
        let current_ns = (elapsed_cycles as f64 * ns_per_cycle) as u64;
        let peak = metrics.update_peak(current_ns, *sample_counter, num_samples);

        let block_end_sample = *sample_counter + num_samples as u64;
        *sample_counter = block_end_sample;

        let _ = self.telemetry_producer.push(Telemetry {
            process_time_ns: current_ns,
            peak_process_time_ns: peak,
            sample_counter: *sample_counter,
            xrun_count: xrun_count_atomic.load(Ordering::Relaxed),
            resource_leaks: metrics.resource_leaks.load(Ordering::Relaxed),
            node_times_ns: node_times,
            peak_levels,
        });
    }
}
