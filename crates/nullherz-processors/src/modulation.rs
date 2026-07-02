use nullherz_traits::AudioProcessor;

const MODULATION_THRESHOLD: f32 = 0.001;

pub struct ModulationProcessor {
    pub target_id: u64,
    pub param_id: u32,
    pub scale: f32,
    pub offset: f32,
    last_sent_value: f32,
}

impl ModulationProcessor {
    pub fn new(target_id: u64, param_id: u32, scale: f32, offset: f32) -> Self {
        Self {
            target_id,
            param_id,
            scale,
            offset,
            last_sent_value: f32::NAN,
        }
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
}
