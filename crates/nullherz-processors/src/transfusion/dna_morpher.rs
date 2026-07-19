use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata, SoundDNA};
use audio_dsp::SpectralPipeline;
use std::sync::Arc;

/// DnaMorpher Processor
/// Performs high-dimensional interpolation (Slerp) between two DNA profiles
/// to morph the spectral and rhythmic characteristics of a signal.
/// Channels a deck strip carries; each needs its own overlap-add pipeline
/// (the pipeline's input/output rings hold one signal's history).
const STEREO_LANES: usize = 2;

pub struct DnaMorpher {
    pub id: u64,
    pipelines: Vec<SpectralPipeline>,

    // Genetic Profiles
    pub(crate) dna_a: Arc<SoundDNA>,
    pub(crate) dna_b: Arc<SoundDNA>,

    // Interpolated State
    pub(crate) current_latent: [f32; 16],

    // Parameters
    morph_pos: f32, // 0.0 (A) to 1.0 (B)
    /// Spectral resynthesis replaces bin magnitudes with the latent vector —
    /// with default (zero) DNA that silences/whitens the signal. Stay a dry
    /// passthrough until real DNA has been loaded.
    pub(crate) engaged: bool,
}

impl DnaMorpher {
    pub fn new(id: u64, fft_size: usize) -> Self {
        Self {
            id,
            pipelines: (0..STEREO_LANES).map(|_| SpectralPipeline::new(fft_size)).collect(),
            dna_a: Arc::new(SoundDNA::default()),
            dna_b: Arc::new(SoundDNA::default()),
            current_latent: [0.0; 16],
            morph_pos: 0.5,
            engaged: false,
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

        if !self.engaged {
            // Dry passthrough on every channel until a transfusion engages.
            for (inp, out) in inputs.iter().zip(outputs.iter_mut()) {
                let n = inp.len().min(out.len());
                out[..n].copy_from_slice(&inp[..n]);
            }
            return;
        }

        self.update_morph();

        let latent = self.current_latent;
        let n_ch = inputs.len().min(outputs.len());
        for ch in 0..n_ch {
            match self.pipelines.get_mut(ch) {
                Some(pipeline) => pipeline.process(inputs[ch], outputs[ch], |re, im, n, _window, _fft| {
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
                }),
                // Wired wider than we have pipelines: dry passthrough rather
                // than starving the channel.
                None => {
                    let n = inputs[ch].len().min(outputs[ch].len());
                    outputs[ch][..n].copy_from_slice(&inputs[ch][..n]);
                }
            }
        }
    }

    fn reset(&mut self) {
        for pipeline in self.pipelines.iter_mut() {
            pipeline.reset();
        }
    }
}

impl nullherz_traits::MidiResponder for DnaMorpher {}
impl nullherz_traits::SnapshotProvider for DnaMorpher {}

impl AudioProcessor for DnaMorpher {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) = command
            && *target_id == self.id {
                self.set_parameter(*param_id, *value, 0);
            }
    }

    fn apply_topology_mutation(&mut self, mutation: nullherz_traits::TopologyMutation) {
        if let nullherz_traits::TopologyMutation::UpdateMetadata { metadata, .. } = mutation {
            // For DnaMorpher, we might treat updates as setting Slot B
            self.dna_a = self.dna_b.clone();
            self.dna_b = Arc::new(metadata.dna.clone());
            // Real DNA has arrived; the morpher may now shape the signal.
            self.engaged = true;
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

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::SignalProcessor;

    /// Regression: with no DNA loaded, the morpher must be bit-transparent.
    /// Its spectral resynthesis with default (zero) latent used to replace
    /// the whole deck signal with near-silence — the "-45dB mush" bug.
    #[test]
    fn test_unengaged_morpher_is_bit_transparent() {
        let mut m = DnaMorpher::new(1, 1024);
        let input: Vec<f32> = (0..512).map(|i| ((i as f32) * 0.13).sin() * 0.8).collect();
        let mut output = vec![0.0f32; 512];
        let mut ctx = nullherz_traits::ProcessContext {
            transport: None, host: None, sub_block_offset: 0, is_last_sub_block: true,
        };
        let inp: [&[f32]; 1] = [&input];
        let mut out: [&mut [f32]; 1] = [&mut output];
        m.process(&inp, &mut out, &mut ctx);
        assert_eq!(output, input, "unengaged morpher must pass audio through unchanged");
    }

    /// Loading real DNA (UpdateMetadata) engages the resynthesis path.
    #[test]
    fn test_update_metadata_engages_morpher() {
        use nullherz_traits::AudioProcessor;
        let mut m = DnaMorpher::new(1, 1024);
        assert!(!m.engaged);
        let mut meta = nullherz_traits::SampleMetadata::new_empty();
        meta.dna.spectral.latent_space = [0.5; 16];
        m.apply_topology_mutation(nullherz_traits::TopologyMutation::UpdateMetadata {
            node_idx: 1,
            metadata: std::sync::Arc::new(meta),
        });
        assert!(m.engaged, "real DNA must engage the morpher");
    }
}
