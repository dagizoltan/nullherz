use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorCommand, Command};
use audio_dsp::{EnvelopeFollower, DspKernel};

pub struct CompressorProcessor {
    pub id: u64,
    envelope_follower: EnvelopeFollower,
    threshold: f32,
    ratio: f32,
    makeup_gain: f32,
    attack_ms: f32,
    release_ms: f32,
    pub(crate) env_buffer: audio_dsp::AlignedBuffer,
}

impl CompressorProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        Self {
            id,
            envelope_follower: EnvelopeFollower::new(sample_rate, 10.0, 100.0),
            threshold: 0.5,
            ratio: 4.0,
            makeup_gain: 1.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            env_buffer: audio_dsp::AlignedBuffer::new(nullherz_traits::MAX_BLOCK_SIZE),
        }
    }
}

impl nullherz_traits::RtSafe for CompressorProcessor {}

impl nullherz_traits::SignalProcessor for CompressorProcessor {
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() { return; }

        let input = inputs[0];
        let sidechain = if inputs.len() >= 2 { inputs[1] } else { inputs[0] };
        let output = &mut outputs[0];
        let len = input.len();

        if self.env_buffer.len() < len {
             // Hardening: This should not happen on RT thread if MAX_BLOCK_SIZE is respected.
             return;
        }

        self.envelope_follower.process_with_sidechain(input, sidechain, &mut self.env_buffer[..len]);

        let threshold_db = 20.0 * self.threshold.log10();
        let inv_ratio = 1.0 / self.ratio;

        for i in 0..len {
            let env = self.env_buffer[i];
            let env_db = 20.0 * env.max(1e-6).log10();

            let gain_reduction_db = if env_db > threshold_db {
                (threshold_db - env_db) * (1.0 - inv_ratio)
            } else {
                0.0
            };

            // dB -> linear is 10^(dB/20). This previously read
            // `(gain_reduction_db / 20.0).powf(10.0)` — base and exponent
            // swapped ((dB/20)^10), which drove the gain to ~1e-7 and
            // effectively muted the signal the instant it compressed.
            let gain = 10.0f32.powf(gain_reduction_db / 20.0) * self.makeup_gain;
            output[i] = input[i] * gain;
        }
    }
fn reset(&mut self) {
        self.envelope_follower.reset();
    }
}

impl nullherz_traits::MidiResponder for CompressorProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for CompressorProcessor { }

impl AudioProcessor for CompressorProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) = command
            && *target_id == self.id {
                self.set_parameter(*param_id, *value, 0);
            }
    }
fn set_parameter(&mut self, param_id: u32, mut value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() { value = 0.0; }
        match param_id {
            0 => self.threshold = value.clamp(0.001, 1.0),
            1 => self.ratio = value.clamp(1.0, 20.0),
            2 => self.makeup_gain = value.clamp(0.0, 4.0),
            3 => {
                self.attack_ms = value.max(0.1);
                self.envelope_follower.set_times(self.attack_ms, self.release_ms);
            }
            4 => {
                self.release_ms = value.max(0.1);
                self.envelope_follower.set_times(self.attack_ms, self.release_ms);
            }
            _ => {}
        }
    }
fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.threshold,
            1 => self.ratio,
            2 => self.makeup_gain,
            3 => self.attack_ms,
            4 => self.release_ms,
            _ => 0.0,
        }
    }

fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 1.0,
            default: 0.0,
        }; 16];

        let names: &[&[u8]] = &[b"Threshold", b"Ratio", b"Makeup", b"Attack", b"Release"];
        let mins = [0.001, 1.0, 0.0, 0.1, 0.1];
        let maxs = [1.0, 20.0, 4.0, 1000.0, 5000.0];
        let defs = [0.5, 4.0, 1.0, 10.0, 100.0];

        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
            parameters[i].min = mins[i];
            parameters[i].max = maxs[i];
            parameters[i].default = defs[i];
        }

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 5,
            parameters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::SignalProcessor;

    /// A signal above threshold must be gently ATTENUATED, not muted. The dB→
    /// linear conversion bug drove the gain to ~1e-7, so a compressing signal
    /// vanished. Feed a steady tone above threshold, settle the envelope, and
    /// assert the steady-state output matches the expected compressor gain.
    #[test]
    fn test_compressor_attenuates_not_mutes() {
        let sr = 44_100.0;
        let mut comp = CompressorProcessor::new(1, sr);
        // Defaults: threshold 0.5, ratio 4.0, makeup 1.0.
        let amp = 1.0f32; // above threshold

        let block = nullherz_traits::MAX_BLOCK_SIZE;
        let input = vec![amp; block];
        let mut out = vec![0.0f32; block];
        let mut ctx = ProcessContext { transport: None, host: None, sub_block_offset: 0, is_last_sub_block: false };

        // Settle the envelope (~0.5 s of steady input).
        for _ in 0..100 {
            let inputs: &[&[f32]] = &[&input];
            let mut o = &mut out[..];
            let outputs: &mut [&mut [f32]] = &mut [&mut o];
            comp.process(inputs, outputs, &mut ctx);
        }

        // Expected: env ~= amp (1.0). env_db = 0; threshold_db = 20*log10(0.5).
        // reduction = (threshold_db - env_db)*(1 - 1/ratio); gain = 10^(red/20).
        let threshold_db = 20.0 * 0.5f32.log10();
        let reduction_db = (threshold_db - 0.0) * (1.0 - 1.0 / 4.0);
        let expected_gain = 10.0f32.powf(reduction_db / 20.0);
        let expected = amp * expected_gain; // ~0.594

        let steady = out[block - 1];
        assert!(
            (steady - expected).abs() < 0.02,
            "compressor output {:.5} != expected {:.5} (gain {:.4}) — dB->linear likely wrong",
            steady, expected, expected_gain
        );
        // And it is genuinely attenuating, not passing through or muting.
        assert!(steady > 0.4 && steady < amp, "output {:.5} not in the compressing range", steady);
    }
}
