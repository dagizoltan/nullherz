use nullherz_traits::AudioProcessor;

const MODULATION_THRESHOLD: f32 = 0.001;

pub struct ModulationProcessor {
    pub id: u64,
    pub target_id: u64,
    pub param_id: u32,
    pub scale: f32,
    pub offset: f32,
    last_sent_value: f32,
}

impl ModulationProcessor {
    pub fn new(id: u64, target_id: u64, param_id: u32, scale: f32, offset: f32) -> Self {
        Self {
            id,
            target_id,
            param_id,
            scale,
            offset,
            last_sent_value: f32::NAN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::{Command, MixerCommand, AudioProcessor};

    #[test]
    fn test_modulation_processor_retargeting() {
        let mut proc = ModulationProcessor::new(100, 200, 1, 0.5, 0.0);

        // Retarget to scale 2.0
        let cmd = Command::Mixer(MixerCommand::SetParam {
            target_id: 100,
            param_id: 2, // scale
            value: 2.0,
            ramp_duration_samples: 0,
        });

        proc.apply_command(&cmd);
        assert_eq!(proc.scale, 2.0);

        // Retarget target_id to 300
        let cmd2 = Command::Mixer(MixerCommand::SetParam {
            target_id: 100,
            param_id: 0, // target_id
            value: 300.0,
            ramp_duration_samples: 0,
        });

        proc.apply_command(&cmd2);
        assert_eq!(proc.target_id, 300);
    }
}

impl nullherz_traits::SignalProcessor for ModulationProcessor {
fn reset(&mut self) {
        self.last_sent_value = f32::NAN;
    }
fn process(&mut self, inputs: &[&[f32]], _outputs: &mut [&mut [f32]], context: &mut nullherz_traits::ProcessContext) {
        if inputs.is_empty() { return; }
        let cv = inputs[0];
        if cv.is_empty() { return; }

        let sum: f32 = cv.iter().sum();
        let avg_cv = sum / cv.len() as f32;
        let val = avg_cv * self.scale + self.offset;

        let is_mod_needed = (val - self.last_sent_value).abs() > MODULATION_THRESHOLD || self.last_sent_value.is_nan();
        if is_mod_needed
            && let Some(host) = context.host {
                host.push_command(0, nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                    target_id: self.target_id,
                    param_id: self.param_id,
                    value: val,
                    ramp_duration_samples: 32,
                }));
                self.last_sent_value = val;
            }
    }
}

impl nullherz_traits::MidiResponder for ModulationProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for ModulationProcessor { }

impl AudioProcessor for ModulationProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &nullherz_traits::Command) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) = command {
            if *target_id == self.id {
                self.set_parameter(*param_id, *value, 0);
            }
        }
    }
fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        match param_id {
            0 => self.target_id = value as u64,
            1 => self.param_id = value as u32,
            2 => self.scale = value,
            3 => self.offset = value,
            _ => {}
        }
    }
fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.target_id as f32,
            1 => self.param_id as f32,
            2 => self.scale,
            3 => self.offset,
            _ => 0.0,
        }
    }
}
