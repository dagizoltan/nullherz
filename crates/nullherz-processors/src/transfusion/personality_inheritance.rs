use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata, SpectralPersonality};
use audio_dsp::SpectralPipeline;
use std::sync::Arc;

/// PersonalityInheritance Processor
/// Realizes the AnaWaves Stage 2 "Transfusion" by allowing one node to inherit
/// the SpectralPersonality (energy map) of another.
pub struct PersonalityInheritanceProcessor {
    pub id: u64,
    pipeline: SpectralPipeline,

    // The "Source" DNA to inherit from
    source_personality: Arc<SpectralPersonality>,

    // Parameters
    transfusion_bias: f32, // 0.0 = original, 1.0 = full inheritance
}

impl PersonalityInheritanceProcessor {
    pub fn new(id: u64, fft_size: usize) -> Self {
        Self {
            id,
            pipeline: SpectralPipeline::new(fft_size),
            source_personality: Arc::new(SpectralPersonality::default()),
            transfusion_bias: 0.5,
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

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        let input = inputs[0];
        let output = &mut outputs[0];
        let bias = self.transfusion_bias;
        let personality = &self.source_personality;

        self.pipeline.process(input, output, |re, im, n, _window, _fft| {
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
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        match param_id {
            0 => self.transfusion_bias = value.clamp(0.0, 1.0),
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
