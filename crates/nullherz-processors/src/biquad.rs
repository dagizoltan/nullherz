use nullherz_traits::{AudioProcessor, MidiHandler, CommandHandler, TopologyHandler, TelemetryProvider};

pub struct BiquadProcessor {
    filters: [audio_dsp::BiquadFilter; crate::MAX_CHANNELS],
    id: u64,
}

impl BiquadProcessor {
    pub fn new(id: u64, coeffs: audio_dsp::BiquadCoefficients) -> Self {
        let filters = std::array::from_fn(|_| audio_dsp::BiquadFilter::new(coeffs));
        Self { filters, id }
    }
}

impl nullherz_traits::RtSafe for BiquadProcessor {}

impl MidiHandler for BiquadProcessor {}
impl TopologyHandler for BiquadProcessor {}
impl TelemetryProvider for BiquadProcessor {}

impl CommandHandler for BiquadProcessor {
    fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        match *command {
            nullherz_traits::Command::SetParam { target_id, param_id, value, ramp_duration_samples }
                if target_id == self.id =>
            {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
            _ => {}
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        let mut coeffs = self.filters[0].target_coeffs;
        match param_id {
            0 => coeffs.b0 = value,
            1 => coeffs.b1 = value,
            2 => coeffs.b2 = value,
            3 => coeffs.a1 = value,
            4 => coeffs.a2 = value,
            _ => return,
        }
        for f in self.filters.iter_mut() {
            f.set_coeffs_ramped(coeffs, ramp_duration_samples);
        }
    }
}

impl AudioProcessor for BiquadProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        use audio_dsp::DspKernel;
        if inputs.is_empty() || outputs.is_empty() { return; }
        let num_channels = inputs.len().min(outputs.len()).min(crate::MAX_CHANNELS);

        for (ch, filter) in self.filters.iter_mut().enumerate().take(num_channels) {
            filter.process(&inputs[ch..ch+1], &mut outputs[ch..ch+1]);
        }
    }

    fn reset(&mut self) {
        use audio_dsp::DspKernel;
        for f in self.filters.iter_mut() {
            f.reset();
        }
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

impl MidiHandler for SimdBiquadProcessor {}
impl TopologyHandler for SimdBiquadProcessor {}
impl TelemetryProvider for SimdBiquadProcessor {}

impl CommandHandler for SimdBiquadProcessor {
    fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        match *command {
            nullherz_traits::Command::SetParam { target_id, param_id, value, ramp_duration_samples }
                if target_id == self.id =>
            {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
            _ => {}
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
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
}

impl AudioProcessor for SimdBiquadProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        let len = inputs[0].len();
        let num_channels = inputs.len().min(outputs.len()).min(crate::MAX_CHANNELS);

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
