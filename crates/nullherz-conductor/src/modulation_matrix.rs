use std::collections::HashMap;
use nullherz_traits::Command;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ModMapping {
    pub target_id: u64,
    pub param_id: u32,
    pub scaling: f32,
    pub ramp_duration_samples: u32,
    pub temporal_shape: Option<TemporalShape>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub enum TemporalShape {
    Sine,
    Saw,
    Square,
    Triangle,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ModulationMatrix {
    pub mappings: HashMap<u32, Vec<ModMapping>>,
}

impl ModulationMatrix {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, macro_id: u32, target_id: u64, param_id: u32, scaling: f32, ramp_duration_samples: u32) {
        self.add_temporal_mapping(macro_id, target_id, param_id, scaling, ramp_duration_samples, None);
    }

    pub fn add_temporal_mapping(&mut self, macro_id: u32, target_id: u64, param_id: u32, scaling: f32, ramp_duration_samples: u32, shape: Option<TemporalShape>) {
        let entry = self.mappings.entry(macro_id).or_default();
        entry.push(ModMapping {
            target_id,
            param_id,
            scaling,
            ramp_duration_samples,
            temporal_shape: shape,
        });
    }

    pub fn remove_mapping(&mut self, macro_id: u32, target_id: u64, param_id: u32) {
        if let Some(mappings) = self.mappings.get_mut(&macro_id) {
            mappings.retain(|m| m.target_id != target_id || m.param_id != param_id);
        }
    }

    pub fn update_macro_value(&mut self, _macro_id: u32, _value: f32) {
        // Conductor no longer performs real-time expansion.
        // This method can be used for UI state persistence or telemetry.
    }

    pub fn expand_analysis_measurement(&self, meas: &nullherz_traits::MeasurementBlock, beat_pos: f64) -> Vec<Command> {
        let mut commands = Vec::new();

        // 100: Spectral Centroid [0.0, 1.0] (0..20000 Hz)
        let centroid_norm = (meas.spectral_centroid_hz / 20000.0).clamp(0.0, 1.0);
        commands.extend(self.expand_macro(100, centroid_norm, beat_pos));

        // 101: RMS Envelope [0.0, 1.0] (-96..0 dB)
        let rms_avg = (meas.rms_db[0] + meas.rms_db[1]) * 0.5;
        let rms_norm = ((rms_avg + 96.0) / 96.0).clamp(0.0, 1.0);
        commands.extend(self.expand_macro(101, rms_norm, beat_pos));

        // 103: Spectral Flux
        let flux_norm = (meas.spectral_flux / 100.0).clamp(0.0, 1.0);
        commands.extend(self.expand_macro(103, flux_norm, beat_pos));

        // 105: Spectral Flatness
        commands.extend(self.expand_macro(105, meas.spectral_flatness, beat_pos));

        // 106: Zero Crossing Rate
        commands.extend(self.expand_macro(106, meas.zero_crossing_rate, beat_pos));

        // 107: Stereo Width
        commands.extend(self.expand_macro(107, (meas.stereo_width * 0.5).clamp(0.0, 1.0), beat_pos));

        commands
    }

    pub fn expand_perception_frame(&self, frame: &nullherz_traits::PerceptionFrame, beat_pos: f64) -> Vec<Command> {
        let mut commands = Vec::new();

        // 104: Perceptual Energy
        commands.extend(self.expand_macro(104, frame.perceptual_energy, beat_pos));

        // 102: Transient Onset (if confidence > 0.5)
        if frame.pitch_confidence > 0.5 {
            commands.extend(self.expand_macro(102, frame.brightness, beat_pos));
        }

        commands
    }

    pub fn expand_macro(&self, macro_id: u32, value: f32, beat_pos: f64) -> Vec<Command> {
        let mut expanded = Vec::new();
        if let Some(mappings) = self.mappings.get(&macro_id) {
            // Pack into bundles of 8 commands each for atomic execution
            for chunk in mappings.chunks(8) {
                let mut data = [0u8; 128];
                let count = chunk.len();
                for (i, mapping) in chunk.iter().enumerate() {
                    let offset = i * 16;

                    let mut val = value * mapping.scaling;

                    if let Some(shape) = mapping.temporal_shape {
                        let phase = (beat_pos % 1.0) as f32; // 1-beat cycle
                        let modifier = match shape {
                            TemporalShape::Sine => (phase * 2.0 * std::f32::consts::PI).sin(),
                            TemporalShape::Saw => phase * 2.0 - 1.0,
                            TemporalShape::Square => if phase < 0.5 { 1.0 } else { -1.0 },
                            TemporalShape::Triangle => if phase < 0.5 { phase * 4.0 - 1.0 } else { 1.0 - (phase - 0.5) * 4.0 },
                        };
                        val *= modifier;
                    }

                    data[offset..offset+8].copy_from_slice(&mapping.target_id.to_le_bytes());
                    data[offset+8..offset+12].copy_from_slice(&mapping.param_id.to_le_bytes());
                    data[offset+12..offset+16].copy_from_slice(&val.to_le_bytes());
                }
                expanded.push(Command::Mixer(nullherz_traits::MixerCommand::Bundle {
                    count: count as u32,
                    data,
                }));
            }
        }
        expanded
    }
}
