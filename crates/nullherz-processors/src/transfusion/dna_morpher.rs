use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata, SoundDNA};
use audio_dsp::SpectralPipeline;
use std::sync::Arc;

/// DnaMorpher Processor
/// Performs high-dimensional interpolation (Slerp) between two DNA profiles
/// to morph the spectral and rhythmic characteristics of a signal.
pub struct DnaMorpher {
    pub id: u64,
    pipeline: SpectralPipeline,

    // Genetic Profiles
    pub(crate) dna_a: Arc<SoundDNA>,
    pub(crate) dna_b: Arc<SoundDNA>,

    // Interpolated State
    pub(crate) current_latent: [f32; 16],

    // Parameters
    morph_pos: f32, // 0.0 (A) to 1.0 (B)
}

impl DnaMorpher {
    pub fn new(id: u64, fft_size: usize) -> Self {
        Self {
            id,
            pipeline: SpectralPipeline::new(fft_size),
            dna_a: Arc::new(SoundDNA::default()),
            dna_b: Arc::new(SoundDNA::default()),
            current_latent: [0.0; 16],
            morph_pos: 0.5,
        }
    }

    fn update_morph(&mut self) {
        audio_dsp::util::slerp_nd(
            &self.dna_a.spectral.latent_space,
            &self.dna_b.spectral.latent_space,
            self.morph_pos,
            &mut self.current_latent
        );
    }
}

impl nullherz_traits::SignalProcessor for DnaMorpher {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        self.update_morph();

        let input = inputs[0];
        let output = &mut outputs[0];
        let latent = self.current_latent;

        self.pipeline.process(input, output, |re, im, n, _window, _fft| {
            let bins_per_map_entry = n / 2 / 16;
            if bins_per_map_entry == 0 { return; }

            for i in 0..16 {
                let target_mag = latent[i].max(0.0).min(1.0);
                let start_bin = i * bins_per_map_entry;
                let end_bin = (i + 1) * bins_per_map_entry;

                for bin in start_bin..end_bin {
                    let current_mag = (re[bin] * re[bin] + im[bin] * im[bin]).sqrt().max(1e-9);
                    let scale = target_mag / current_mag;
                    re[bin] *= scale;
                    im[bin] *= scale;
                }
            }
        });
    }

    fn reset(&mut self) {
        self.pipeline.reset();
    }
}

impl nullherz_traits::MidiResponder for DnaMorpher {}
impl nullherz_traits::SnapshotProvider for DnaMorpher {}

impl AudioProcessor for DnaMorpher {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) = command {
            if *target_id == self.id {
                self.set_parameter(*param_id, *value, 0);
            }
        }
    }

    fn apply_topology_mutation(&mut self, mutation: nullherz_traits::TopologyMutation) {
        match mutation {
            nullherz_traits::TopologyMutation::UpdateMetadata { metadata, .. } => {
                // For DnaMorpher, we might treat updates as setting Slot B
                self.dna_a = self.dna_b.clone();
                self.dna_b = Arc::new(metadata.dna.clone());
            }
            _ => {}
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if param_id == 0 {
            self.morph_pos = value.clamp(0.0, 1.0);
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

        let name = "Morph Position";
        let name_bytes = name.as_bytes();
        parameters[0].name[..name_bytes.len()].copy_from_slice(name_bytes);

        Some(ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 1,
            parameters,
        })
    }
}
