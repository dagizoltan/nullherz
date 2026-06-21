use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use nullherz_traits::{AudioProcessor, TelemetryProducer, telemetry::Telemetry};
use crate::engine::metrics::EngineMetrics;

pub struct TelemetryFinalizer {}

impl TelemetryFinalizer {
    pub fn finalize_block_telemetry(
        graph: &mut dyn AudioProcessor,
        metrics: &EngineMetrics,
        telemetry_producer: &mut Box<dyn TelemetryProducer>,
        xrun_count_atomic: &Arc<AtomicU32>,
        sample_counter: &mut u64,
        start_cycles: u64,
        num_samples: usize,
    ) {
        let mut node_times = [0u64; 64];
        let mut peak_levels = [0.0f32; 64];
        let mut node_times_cycles = [0u64; 64];

        graph.collect_telemetry(&mut node_times_cycles, &mut peak_levels);

        let ns_per_cycle = f64::from_bits(metrics.ns_per_cycle.load(Ordering::Relaxed));
        nullherz_traits::telemetry::TelemetryProcessor::collect_node_times(
            &node_times_cycles,
            ns_per_cycle,
            &mut node_times
        );

        let elapsed_cycles = crate::get_cycles().wrapping_sub(start_cycles);
        let current_ns = (elapsed_cycles as f64 * ns_per_cycle) as u64;
        let peak = metrics.update_peak(current_ns, *sample_counter, num_samples);

        let block_end_sample = *sample_counter + num_samples as u64;
        *sample_counter = block_end_sample;

        let _ = telemetry_producer.push_telemetry(Telemetry {
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
