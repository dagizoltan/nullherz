use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorCommand, Command};

pub struct StereoUtilityProcessor {
    pub id: u64,
    pan: f32, // -1.0 to 1.0
    width: f32, // 0.0 to 2.0 (1.0 = normal, 0.0 = mono, 2.0 = extra wide)
}

impl StereoUtilityProcessor {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            pan: 0.0,
            width: 1.0,
        }
    }
}

impl nullherz_traits::RtSafe for StereoUtilityProcessor {}

impl nullherz_traits::SignalProcessor for StereoUtilityProcessor {
fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if inputs.len() < 2 || outputs.len() < 2 { return; }
        let len = inputs[0].len();

        let pan = self.pan;
        let width = self.width;

        // Constant Power Panning
        let pan_angle = (pan + 1.0) * (std::f32::consts::PI / 4.0);
        let pan_l = pan_angle.cos();
        let pan_r = pan_angle.sin();

        for i in 0..len {
            let mut l = inputs[0][i];
            let mut r = inputs[1][i];

            // 1. Width (M/S based)
            let mid = (l + r) * 0.5;
            let side = (r - l) * 0.5;

            let w_side = side * width;

            l = mid - w_side;
            r = mid + w_side;

            // 2. Pan
            outputs[0][i] = l * pan_l;
            outputs[1][i] = r * pan_r;
        }
    }
}

impl nullherz_traits::MidiResponder for StereoUtilityProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for StereoUtilityProcessor { }

impl AudioProcessor for StereoUtilityProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) = command
            && *target_id == self.id {
                self.set_parameter(*param_id, *value, 0);
            }
    }
fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        match param_id {
            0 => self.pan = value.clamp(-1.0, 1.0),
            1 => self.width = value.clamp(0.0, 2.0),
            _ => {}
        }
    }
fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.pan,
            1 => self.width,
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

        let names: &[&[u8]] = &[b"Pan", b"Width"];
        let mins = [-1.0, 0.0];
        let maxs = [1.0, 2.0];
        let defs = [0.0, 1.0];

        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
            parameters[i].min = mins[i];
            parameters[i].max = maxs[i];
            parameters[i].default = defs[i];
        }

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 2,
            parameters,
        })
    }
}
