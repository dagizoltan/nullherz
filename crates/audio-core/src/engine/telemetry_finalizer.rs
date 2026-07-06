use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use nullherz_traits::{AudioProcessor, TelemetryProducer, telemetry::Telemetry};
use crate::engine::metrics::EngineMetrics;

pub struct TelemetryFinalizer {}

impl TelemetryFinalizer {
    pub fn finalize_block_telemetry(
        graph: &mut dyn AudioProcessor,
        metrics: &EngineMetrics,
        outputs: &mut [&mut [f32]],
        telemetry_producer: &mut Box<dyn TelemetryProducer>,
        xrun_count_atomic: &Arc<AtomicU32>,
        sample_counter: u64,
        start_cycles: u64,
        num_samples: usize,
        fft: &audio_dsp::SimdFft,
        fft_re: &mut audio_dsp::AlignedBuffer,
        fft_im: &mut audio_dsp::AlignedBuffer,
        transport: &nullherz_traits::Transport,
    ) -> Telemetry {
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
        let peak = metrics.update_peak(current_ns, transport.sample_rate, sample_counter, num_samples);

        // 1024-point Spectrum Analysis (AnaWaves Stage 2)
        // Optimized for RT-safety: No allocations, zero-padded input
        let mut spectrum = [0.0f32; 128];
        if !outputs.is_empty() {
            let fft_size = 1024;

            // Clear buffers (re & im) before use
            fft_re.fill(0.0);
            fft_im.fill(0.0);

            let out_l = &outputs[0];
            let len = out_l.len().min(fft_size);
            fft_re[..len].copy_from_slice(&out_l[..len]);

            fft.process(fft_re, fft_im);

            for i in 0..128 {
                let mut sum = 0.0;
                for k in 0..4 {
                    let bin = i * 4 + k;
                    sum += (fft_re[bin] * fft_re[bin] + fft_im[bin] * fft_im[bin]).sqrt();
                }
                spectrum[i] = sum / 4.0;
            }
        }

        // Stage 6: decimate spectrum to latent space representation
        let mut dna_latent_space = [0.0f32; 16];
        for i in 0..16 {
            let mut sum = 0.0;
            for k in 0..8 {
                sum += spectrum[i * 8 + k];
            }
            dna_latent_space[i] = (sum / 8.0).min(1.0);
        }

        let mut goniometer_pts = [0.0f32; 128];
        if outputs.len() >= 2 {
            let left = &outputs[0];
            let right = &outputs[1];
            let step = left.len() / 64;
            if step > 0 {
                for i in 0..64 {
                    let idx = i * step;
                    goniometer_pts[i * 2] = left[idx];
                    goniometer_pts[i * 2 + 1] = right[idx];
                }
            }
        }

        let telemetry = Telemetry {
            process_time_ns: current_ns,
            peak_process_time_ns: peak,
            sample_counter,
            xrun_count: xrun_count_atomic.load(Ordering::Relaxed),
            last_xrun_magnitude_ns: metrics.last_xrun_magnitude_ns.load(Ordering::Relaxed),
            resource_leaks: metrics.resource_leaks.load(Ordering::Relaxed),
            bpm: transport.bpm,
            beat_position: transport.beat_position,
            node_times_ns: node_times,
            peak_levels,
            spectrum,
            goniometer_pts,
            dna_latent_space,
            active_clips: [255; 8],
            starting_clips_mask: [0; 8],
            remote_node_count: 0,
            remote_cpu_usage: [0.0; 8],
            remote_latency_ms: [0.0; 8],
            suggestions: [(0, 0.0); 4],
            active_master_deck: 'A',
        };
        let _ = telemetry_producer.push_telemetry(telemetry.clone());
        telemetry
    }
}
