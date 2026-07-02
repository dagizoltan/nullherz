use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata};
use audio_dsp::{SpectralPipeline, AlignedBuffer, SpectralWindowShape};

pub struct SpectralMorphProcessor {
    pub id: u64,
    pipeline: SpectralPipeline,
    modulator_pipeline: SpectralPipeline,
    modulator_env: AlignedBuffer,
    has_modulator_spectrum: bool,
    dummy_out: [f32; ipc_layer::MAX_BLOCK_SIZE],

    // Parameters
    morph_amount: f32,
    smoothness: f32,
}

impl SpectralMorphProcessor {
    pub fn new(id: u64, fft_size: usize) -> Self {
        Self {
            id,
            pipeline: SpectralPipeline::new(fft_size),
            modulator_pipeline: SpectralPipeline::new(fft_size),
            modulator_env: AlignedBuffer::new(fft_size),
            has_modulator_spectrum: false,
            dummy_out: [0.0; ipc_layer::MAX_BLOCK_SIZE],
            morph_amount: 1.0,
            smoothness: 0.5,
        }
    }
}

impl nullherz_traits::SignalProcessor for SpectralMorphProcessor {
fn reset(&mut self) {
        self.has_modulator_spectrum = false;
        self.pipeline.reset();
        self.modulator_pipeline.reset();
        self.modulator_env.fill(0.0);
    }
fn latency_samples(&self) -> usize {
        self.pipeline.fft.size
    }
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.len() < 2 || outputs.is_empty() { return; }

        let carrier = inputs[0];
        let modulator = inputs[1];
        let output = &mut outputs[0];

        let mut has_spectrum = false;
        let smoothness = self.smoothness;

        {
            let env = &mut self.modulator_env;
            self.modulator_pipeline.process(modulator, &mut self.dummy_out[..modulator.len().min(ipc_layer::MAX_BLOCK_SIZE)], |re, im, n, _window, _fft| {
                let window_size = (smoothness * (n as f32 / 8.0)) as usize;
                audio_dsp::util::extract_spectral_envelope(re, im, env, window_size);
                has_spectrum = true;
            });
        }
        self.has_modulator_spectrum = has_spectrum;

        let env_ref = &self.modulator_env;
        let has_mod = self.has_modulator_spectrum;
        let morph = self.morph_amount;

        self.pipeline.process(carrier, output, |re, im, n, _window, _fft| {
            if has_mod {
                for i in 0..n {
                    let m_mag = env_ref[i];
                    let scale = 1.0 + (m_mag - 1.0) * morph;
                    re[i] *= scale;
                    im[i] *= scale;
                }
            }
        });
    }
}

impl nullherz_traits::MidiResponder for SpectralMorphProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for SpectralMorphProcessor { }

impl AudioProcessor for SpectralMorphProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        match *command {
            nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, ramp_duration_samples }) => {
                if target_id == self.id {
                    self.set_parameter(param_id, value, ramp_duration_samples);
                }
            }
            nullherz_traits::Command::Extension(nullherz_traits::OpaqueEnvelope { domain_id, target_id, opcode, data }) => {
                if domain_id == 0x53504543 && target_id == self.id { // "SPEC"
                    match opcode {
                        0x01 => { // Set Window Shape (alternate via opcode)
                            let shape_val = data[0];
                            let shape = match shape_val {
                                0 => SpectralWindowShape::Hann,
                                1 => SpectralWindowShape::Hamming,
                                2 => SpectralWindowShape::Blackman,
                                3 => SpectralWindowShape::Rectangular,
                                _ => SpectralWindowShape::Hann,
                            };
                            self.pipeline.update_window(shape);
                            self.modulator_pipeline.update_window(shape);
                        }
                        0x02 => { // Advanced Morph Parameter (e.g. Spectral Tilt offset)
                             // data[0..4] could be a f32
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        match param_id {
            0 => self.morph_amount = value.clamp(0.0, 1.0),
            1 => self.smoothness = value.clamp(0.0, 1.0),
            2 => {
                let shape = match value as u32 {
                    0 => SpectralWindowShape::Hann,
                    1 => SpectralWindowShape::Hamming,
                    2 => SpectralWindowShape::Blackman,
                    3 => SpectralWindowShape::Rectangular,
                    _ => SpectralWindowShape::Hann,
                };
                self.pipeline.update_window(shape);
                self.modulator_pipeline.update_window(shape);
            }
            _ => {}
        }
    }
fn metadata(&self) -> Option<ProcessorMetadata> {
        let mut parameters = [ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 1.0,
            default: 1.0,
        }; 16];

        let names = [
            (0, "Morph", 0.0, 1.0, 1.0),
            (1, "Smoothness", 0.0, 1.0, 0.5),
            (2, "Window", 0.0, 3.0, 0.0),
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
            processor_id: 0,
            num_parameters: names.len() as u32,
            parameters,
        })
    }
}
