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
        let mut node_peak_times = [0u64; 64];
        let mut peak_levels = [0.0f32; 64];
        let mut node_times_cycles = [0u64; 64];

        graph.collect_telemetry(&mut node_times_cycles, &mut peak_levels);

        let ns_per_cycle = f64::from_bits(metrics.ns_per_cycle.load(Ordering::Relaxed));
        nullherz_traits::telemetry::TelemetryProcessor::collect_node_times(
            &node_times_cycles,
            ns_per_cycle,
            &mut node_times
        );

        for i in 0..64 {
            let cycles = node_times_cycles[i];
            let peak_cycles = nullherz_traits::telemetry::TelemetryProcessor::update_peak(&metrics.node_peak_cycles[i], cycles);
            node_peak_times[i] = (peak_cycles as f64 * ns_per_cycle) as u64;
        }

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

        let mut deck_positions = [0u64; 4];
        let mut deck_playback_rates = [1.0f32; 4];
        if let Some(pg) = graph.as_any_mut().downcast_mut::<crate::processors::ProcessorGraph>() {
            // Deck samplers are the first four samplers in node-index order:
            // the bootstrap allocates deck strips A-D before the preview
            // sampler and any user-added nodes, and node ids are never
            // reused. (The old `!= 111` preview exclusion is gone — the
            // preview node lives at a real allocated index now, safely
            // after the decks.)
            let mut deck_idx = 0;
            for i in 0..pg.node_count {
                let proc = unsafe { &*pg.nodes[i].processor.get() };
                if proc.processor_type() == "sampler" {
                    if deck_idx < 4 {
                        deck_positions[deck_idx] = proc.get_playback_position();
                        deck_playback_rates[deck_idx] = proc.get_parameter(1);
                        deck_idx += 1;
                    } else {
                        break;
                    }
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
            node_peak_times_ns: node_peak_times,
            peak_levels,
            spectrum,
            goniometer_pts,
            dna_latent_space,
            active_clips: [255; 8],
            starting_clips_mask: [0; 8],
            system_time_ns: transport.system_time_ns,
            device_time_ns: transport.device_time_ns,
            clock_jitter_ns: 0, // Should be populated by Engine if available
            remote_node_count: 0,
            remote_cpu_usage: [0.0; 8],
            remote_latency_ms: [0.0; 8],
            calibration_samples: 0,
            sample_rate: transport.sample_rate,
            suggestions: [(0, 0.0); 4],
            active_master_deck: 'A',
            waveform_peaks: [0.0; 256],
            deck_positions,
            deck_playback_rates,
            node_map_keys: [[0u8; 32]; 32],
            node_map_values: [0u32; 32],
            audio_devices: [nullherz_traits::telemetry::DeviceName::default(); 16],
            ..Telemetry::default()
        };
        let _ = telemetry_producer.push_telemetry(telemetry);
        telemetry
    }
}
