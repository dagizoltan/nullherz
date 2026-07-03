use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata, SpectralPersonality, RhythmicDNA, ArtifactProfile, SpatialDNA};
use audio_dsp::SpectralPipeline;
use std::sync::Arc;

/// PersonalityInheritance Processor
/// Realizes the AnaWaves Stage 2 & 3 "Transfusion" by allowing one node to inherit
/// the SpectralPersonality (energy map) and RhythmicDNA of another.
pub struct PersonalityInheritanceProcessor {
    pub id: u64,
    pipeline: SpectralPipeline,

    // The "Source" DNA to inherit from
    source_personality: Arc<SpectralPersonality>,
    source_rhythmic: Arc<RhythmicDNA>,
    source_artifacts: Arc<ArtifactProfile>,
    source_spatial: Arc<SpatialDNA>,

    // Parameters
    transfusion_bias: f32, // 0.0 = original, 1.0 = full inheritance
    rhythmic_bias: f32,    // 0.0 = original, 1.0 = full rhythmic transfusion
    artifact_bias: f32,    // 0.0 = original, 1.0 = full artifact transfusion
    spatial_bias: f32,     // 0.0 = original, 1.0 = full spatial transfusion

    // Layer 3: Rhythmic Pulse Inheritance (Delay Line)
    delay_buffer: Vec<f32>,
    write_ptr: usize,
}

impl PersonalityInheritanceProcessor {
    pub fn new(id: u64, fft_size: usize) -> Self {
        Self {
            id,
            pipeline: SpectralPipeline::new(fft_size),
            source_personality: Arc::new(SpectralPersonality::default()),
            source_rhythmic: Arc::new(RhythmicDNA::default()),
            source_artifacts: Arc::new(ArtifactProfile::default()),
            source_spatial: Arc::new(SpatialDNA::default()),
            transfusion_bias: 0.5,
            rhythmic_bias: 0.5,
            artifact_bias: 0.5,
            spatial_bias: 0.5,
            delay_buffer: vec![0.0; 44100], // 1 second max delay
            write_ptr: 0,
        }
    }

    pub fn set_source_personality(&mut self, personality: Arc<SpectralPersonality>) {
        self.source_personality = personality;
    }
}

impl nullherz_traits::SignalProcessor for PersonalityInheritanceProcessor {
    fn reset(&mut self) {
        self.pipeline.reset();
    }

    fn latency_samples(&self) -> usize {
        self.pipeline.fft.size
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        let input = inputs[0];
        let output = &mut outputs[0];
        let bias = self.transfusion_bias;
        let r_bias = self.rhythmic_bias;
        let a_bias = self.artifact_bias;
        let s_bias = self.spatial_bias;
        let personality = &self.source_personality;
        let rhythmic = &self.source_rhythmic;
        let artifacts = &self.source_artifacts;
        let spatial = &self.source_spatial;

        // Apply Rhythmic Micro-timing (Layer 3)
        let mut rhythmic_input = vec![0.0; input.len()];
        if let Some(transport) = context.transport {
            let samples_per_beat = (transport.sample_rate as f64 * 60.0) / transport.bpm as f64;
            let current_beat = transport.beat_position;

            for (i, &sample) in input.iter().enumerate() {
                let sample_beat = current_beat + (i as f64 / samples_per_beat);
                let _beat_in_pattern = (sample_beat % 4.0) as usize;
                let step = ((sample_beat * 4.0) % 64.0) as usize;

                // Micro-timing offset (Early/Late)
                let micro_offset_ms = rhythmic.micro_timing[step % 12] as f32; // micro_timing is in ms
                let delay_samples = (micro_offset_ms * transport.sample_rate * 0.001) * r_bias;

                // Read from delay line with linear interpolation
                let read_ptr = (self.write_ptr as f32 - delay_samples + self.delay_buffer.len() as f32) % self.delay_buffer.len() as f32;
                let idx0 = read_ptr.floor() as usize;
                let idx1 = (idx0 + 1) % self.delay_buffer.len();
                let frac = read_ptr - idx0 as f32;
                let delayed_sample = self.delay_buffer[idx0] * (1.0 - frac) + self.delay_buffer[idx1] * frac;

                // Onset Mask Gating (Layer 3)
                let mask_val = (rhythmic.onset_mask[step / 16] >> (step % 16)) & 1;
                let target_gain = if mask_val == 1 { 1.2 } else { 0.8 };
                let gain = 1.0 + (target_gain - 1.0) * r_bias;

                rhythmic_input[i] = delayed_sample * gain;

                // Update delay buffer
                self.delay_buffer[self.write_ptr] = sample;
                self.write_ptr = (self.write_ptr + 1) % self.delay_buffer.len();
            }
        } else {
            rhythmic_input.copy_from_slice(input);
        }

        // Apply Artifact Profile (Layer 4) - Simplified Noise Floor Injection
        if a_bias > 0.0 {
            let noise_gain = (10.0f32.powf(artifacts.noise_floor_db / 20.0)) * a_bias;
            for (i, s) in rhythmic_input.iter_mut().enumerate() {
                // RT-Safe pseudo-random noise (simple hash to avoid TLs/Locks)
                let n = ((i as u32).wrapping_mul(1103515245).wrapping_add(12345)) as f32 / 2147483647.0;
                let noise = (n * 2.0 - 1.0) * noise_gain;
                *s += noise;
            }
        }

        // Apply Spatial Transfusion (Layer 5) - Mid/Side width manipulation
        let mut width_scale = 1.0;
        if s_bias > 0.0 {
            // Transfuse stereo width from source
            let target_width = spatial.stereo_width;
            width_scale = 1.0 + (target_width - 1.0) * s_bias;
        }

        self.pipeline.process(&rhythmic_input, output, |re, im, n, _window, _fft| {
            // Apply Spatial DNA to spectral bins (simplified widening/narrowing)
            for bin in 0..n {
                // High frequencies are usually more susceptible to width changes
                let freq_norm = bin as f32 / n as f32;
                let bin_width_scale = 1.0 + (width_scale - 1.0) * freq_norm;
                re[bin] *= bin_width_scale;
                im[bin] *= bin_width_scale;
            }

            // energy_map is 64 bins covering 0-20kHz.
            // We map these 64 bins back to the N FFT bins.
            let bins_per_map_entry = n / 2 / 64;
            if bins_per_map_entry == 0 { return; }

            for i in 0..64 {
                let target_mag = personality.energy_map[i] as f32 / 255.0;

                let start_bin = i * bins_per_map_entry;
                let end_bin = (i + 1) * bins_per_map_entry;

                for bin in start_bin..end_bin {
                    let current_mag = (re[bin] * re[bin] + im[bin] * im[bin]).sqrt().max(1e-9);

                    // Transfusion: Adjust magnitude towards target personality
                    // We use a simple gain scaling here to "shape" the spectrum.
                    let ratio = target_mag / current_mag;
                    let scale = 1.0 + (ratio - 1.0) * bias;

                    re[bin] *= scale;
                    im[bin] *= scale;

                    // Mirror for real FFT if necessary (though our SimdFft is complex-to-complex usually)
                    // But typically we only process the positive frequencies in these simple morphs.
                }
            }
        });
    }
}

impl nullherz_traits::MidiResponder for PersonalityInheritanceProcessor {
    fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { }
}

impl nullherz_traits::SnapshotProvider for PersonalityInheritanceProcessor { }

impl AudioProcessor for PersonalityInheritanceProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        match command {
            nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) => {
                if *target_id == self.id {
                    self.set_parameter(*param_id, *value, 0);
                }
            }
            _ => {}
        }
    }

    fn apply_topology_mutation(&mut self, mutation: nullherz_traits::TopologyMutation) {
        if let nullherz_traits::TopologyMutation::UpdateMetadata { node_idx: _, metadata } = mutation {
            // In a real scenario, node_idx would be checked against our routing,
            // but here we might accept metadata updates to refresh the personality.
            self.source_personality = Arc::new(metadata.dna.spectral.clone());
            self.source_rhythmic = Arc::new(metadata.dna.rhythmic.clone());
            self.source_artifacts = Arc::new(metadata.dna.artifacts.clone());
            self.source_spatial = Arc::new(metadata.dna.spatial.clone());
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        match param_id {
            0 => self.transfusion_bias = value.clamp(0.0, 1.0),
            1 => self.rhythmic_bias = value.clamp(0.0, 1.0),
            2 => self.artifact_bias = value.clamp(0.0, 1.0),
            3 => self.spatial_bias = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn metadata(&self) -> Option<ProcessorMetadata> {
        let mut parameters = [ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 1.0,
            default: 0.5,
        }; 16];

        let names = [
            (0, "Transfusion Bias", 0.0, 1.0, 0.5),
            (1, "Rhythmic Bias", 0.0, 1.0, 0.5),
            (2, "Artifact Bias", 0.0, 1.0, 0.5),
            (3, "Spatial Bias", 0.0, 1.0, 0.5),
        ];

        for (i, (id, name, min, max, default)) in names.iter().enumerate() {
            parameters[i].id = *id;
            parameters[i].min = *min;
            parameters[i].max = *max;
            parameters[i].default = *default;
            let name_bytes = name.as_bytes();
            parameters[i].name[..name_bytes.len()].copy_from_slice(name_bytes);
        }

        Some(ProcessorMetadata {
            processor_id: self.id,
            num_parameters: names.len() as u32,
            parameters,
        })
    }
}
