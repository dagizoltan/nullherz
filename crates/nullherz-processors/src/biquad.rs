use nullherz_traits::{AudioProcessor, ProcessContext, Command, ProcessorCommand};
use audio_dsp::BiquadCoefficients;
use crate::dsp_kernel_processor::MultiChannelDspProcessor;

pub struct BiquadProcessor {
    inner: MultiChannelDspProcessor<audio_dsp::BiquadFilter>,
}

impl BiquadProcessor {
    pub fn new(id: u64, coeffs: BiquadCoefficients) -> Self {
        let filter = audio_dsp::BiquadFilter::new(coeffs);
        Self {
            inner: MultiChannelDspProcessor::new(id, filter, nullherz_traits::MAX_CHANNELS),
        }
    }
}

impl nullherz_traits::RtSafe for BiquadProcessor {}

impl nullherz_traits::SignalProcessor for BiquadProcessor {
fn set_safe_mode(&mut self, enabled: bool) {
        if enabled {
            // Neutral coefficients (allpass/bypass) in safe mode
            self.inner.set_parameter(0, 1.0, 0); // b0=1.0 is default in our impl
        }
    }
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext) {
        self.inner.process(inputs, outputs, context);
    }
fn reset(&mut self) {
        self.inner.reset();
    }
}

impl nullherz_traits::MidiResponder for BiquadProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for BiquadProcessor { }

impl AudioProcessor for BiquadProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        if !value.is_finite() || value.abs() > 10.0 { return; }
        let mut coeffs = self.inner.kernels[0].target_coeffs;
        match param_id {
            0 => coeffs.b0 = value,
            1 => coeffs.b1 = value,
            2 => coeffs.b2 = value,
            3 => coeffs.a1 = value,
            4 => coeffs.a2 = value,
            _ => return,
        }
        for f in self.inner.kernels.iter_mut() {
            f.set_coeffs_ramped(coeffs, ramp_duration_samples);
        }
    }
fn apply_command(&mut self, command: &ProcessorCommand) {
        self.inner.apply_command(command);
    }
fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: -1.0,
            max: 1.0,
            default: 0.0,
        }; 16];

        let names = [b"b0", b"b1", b"b2", b"a1", b"a2"];
        for (i, name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(*name);
        }

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.inner.id,
            num_parameters: 5,
            parameters,
        })
    }
}

pub struct SimdBiquadProcessor {
    inner: audio_dsp::SimdBiquad,
    id: u64,
}

impl SimdBiquadProcessor {
    pub fn new(id: u64, coeffs: audio_dsp::BiquadCoefficients) -> Self {
        Self { inner: audio_dsp::SimdBiquad::new(coeffs), id }
    }
}

impl nullherz_traits::SignalProcessor for SimdBiquadProcessor {
fn set_safe_mode(&mut self, enabled: bool) {
        if enabled {
            self.inner.coeffs = audio_dsp::BiquadCoefficients::default();
        }
    }
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        let len = inputs[0].len();
        let num_channels = inputs.len().min(outputs.len()).min(nullherz_traits::MAX_CHANNELS);

        if (8..16).contains(&num_channels) {
            let mut in_ptrs = [std::ptr::null(); 8];
            let mut out_ptrs = [std::ptr::null_mut(); 8];
            for i in 0..8 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            self.inner.process_8_channels(in_ptrs, out_ptrs, len);
        } else if num_channels >= 16 {
            let mut in_ptrs = [std::ptr::null(); 16];
            let mut out_ptrs = [std::ptr::null_mut(); 16];
            for i in 0..16 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            #[cfg(target_arch = "x86_64")]
            unsafe { self.inner.process_16_channels(in_ptrs, out_ptrs, len); }
        } else {
            for ch in 0..num_channels {
                self.inner.process_scalar(ch, inputs[ch], outputs[ch]);
            }
        }
    }
fn reset(&mut self) {
        self.inner.z1.fill(0.0);
        self.inner.z2.fill(0.0);
    }
}

impl nullherz_traits::MidiResponder for SimdBiquadProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for SimdBiquadProcessor { }

impl AudioProcessor for SimdBiquadProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() || value.abs() > 10.0 { return; }
        let mut coeffs = self.inner.coeffs;
        match param_id {
            0 => coeffs.b0 = value,
            1 => coeffs.b1 = value,
            2 => coeffs.b2 = value,
            3 => coeffs.a1 = value,
            4 => coeffs.a2 = value,
            _ => return,
        }
        self.inner.coeffs = coeffs;
    }
fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, ramp_duration_samples }) = *command
            && target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
    }
}
