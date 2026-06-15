use nullherz_traits::AudioProcessor;

pub struct GainProcessor {
    gains: [audio_dsp::Gain; crate::MAX_CHANNELS],
    id: u64,
    bypassed: bool,
}

impl GainProcessor {
    pub fn new(id: u64, initial_gain: f32) -> Self {
        let gains = std::array::from_fn(|_| audio_dsp::Gain::new(initial_gain, 0.05));
        Self { gains, id, bypassed: false }
    }
}

impl nullherz_traits::RtSafe for GainProcessor {}

impl nullherz_traits::Bypassable for GainProcessor {
    fn set_bypass(&mut self, bypassed: bool) { self.bypassed = bypassed; }
    fn is_bypassed(&self) -> bool { self.bypassed }
}

impl AudioProcessor for GainProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        use audio_dsp::DspKernel;
        if inputs.is_empty() || outputs.is_empty() { return; }
        let num_channels = inputs.len().min(outputs.len()).min(crate::MAX_CHANNELS);

        if self.bypassed {
            for i in 0..num_channels {
                outputs[i].copy_from_slice(inputs[i]);
            }
            return;
        }

        for (i, gain) in self.gains.iter_mut().enumerate().take(num_channels) {
            gain.process(&inputs[i..i+1], &mut outputs[i..i+1]);
        }
    }
    fn set_parameter(&mut self, param_id: u32, value: f32, ramp_duration_samples: u32) {
        use audio_dsp::DspKernel;
        for g in self.gains.iter_mut() {
            g.set_parameter(param_id, value, ramp_duration_samples);
        }
    }

    fn apply_command(&mut self, command: &control_plane::Command) {
        match *command {
            control_plane::Command::SetParam { target_id, param_id, value, ramp_duration_samples }
                if target_id == self.id =>
            {
                if param_id == 999 {
                    use nullherz_traits::Bypassable;
                    self.set_bypass(value > 0.5);
                } else {
                    self.set_parameter(param_id, value, ramp_duration_samples);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::test_kit::{VirtualClockHost, ConformanceSuite};

    #[test]
    fn test_gain_processor_ramping_sub_blocks() {
        let mut host = VirtualClockHost::new();
        let mut gain = GainProcessor::new(1, 0.0);

        // Command: Set gain to 1.0 over 100 samples, starting at sample 10.
        let commands = vec![
            (10, control_plane::Command::SetParam {
                target_id: 1,
                param_id: 0,
                value: 1.0,
                ramp_duration_samples: 100,
            })
        ];

        // Process 256 samples (across multiple sub-blocks if host decides)
        // We use a custom run to verify specific points
        let input = vec![1.0f32; 256];
        let mut output = vec![0.0f32; 256];

        let inputs = [ &input[..] ];

        let mut outputs_ptr = [ &mut output[..] ];
        host.process_with_commands_and_buffers(&mut gain, 256, &commands, &inputs, &mut outputs_ptr);

        // Verify:
        // Samples 0-10: Gain should be 0.0
        for i in 0..10 {
            assert!(output[i].abs() < 1e-6, "Expected silence at sample {}, got {}", i, output[i]);
        }
        // Samples 110+: Gain should be 1.0 (approx)
        for i in 110..256 {
            assert!((output[i] - 1.0).abs() < 1e-5, "Expected gain 1.0 at sample {}, got {}", i, output[i]);
        }
        // Midpoint
        assert!((output[60] - 0.5).abs() < 0.02);
    }

    #[test]
    fn test_gain_processor_conformance() {
        let mut gain = GainProcessor::new(1, 1.0);
        ConformanceSuite::verify_sub_block_consistency(&mut gain).expect("Sub-block consistency failed");
    }

    #[test]
    fn test_gain_processor_bypass() {
        let mut gain = GainProcessor::new(1, 0.0); // Silence
        let host = VirtualClockHost::new();
        let input = [1.0f32; 64];
        let mut output = [0.0f32; 64];

        // Normal: silence
        let mut ctx = nullherz_traits::ProcessContext { transport: Some(&host.transport), sub_block_offset: 0, is_last_sub_block: true };
        gain.process(&[&input], &mut [&mut output], &mut ctx);
        assert_eq!(output[0], 0.0);

        // Bypassed: passthrough
        use nullherz_traits::Bypassable;
        gain.set_bypass(true);
        gain.process(&[&input], &mut [&mut output], &mut ctx);
        assert_eq!(output[0], 1.0);
    }
}
