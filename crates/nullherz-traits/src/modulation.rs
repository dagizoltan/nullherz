use crate::*;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
#[repr(u32)]
pub enum TemporalShape {
    Sine,
    Saw,
    Square,
    Triangle,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ModMapping {
    pub macro_id: u32,
    pub target_id: u64,
    pub param_id: u32,
    pub scaling: f32,
    pub ramp_duration_samples: u32,
    pub temporal_shape: Option<TemporalShape>,
    pub active: bool,
}

impl Default for ModMapping {
    fn default() -> Self {
        Self {
            macro_id: 0,
            target_id: 0,
            param_id: 0,
            scaling: 1.0,
            ramp_duration_samples: 0,
            temporal_shape: None,
            active: false,
        }
    }
}

pub const MAX_MOD_MAPPINGS: usize = 128;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ModulationMatrix {
    #[serde(with = "BigArray")]
    pub mappings: [ModMapping; MAX_MOD_MAPPINGS],
}

impl Default for ModulationMatrix {
    fn default() -> Self {
        Self {
            mappings: [ModMapping::default(); MAX_MOD_MAPPINGS],
        }
    }
}

impl ModulationMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_mapping(&mut self, macro_id: u32, target_id: u64, param_id: u32, scaling: f32, ramp_duration_samples: u32, shape: Option<TemporalShape>) {
        // First try to find an existing mapping to update
        for mapping in self.mappings.iter_mut() {
            if mapping.active && mapping.macro_id == macro_id && mapping.target_id == target_id && mapping.param_id == param_id {
                mapping.scaling = scaling;
                mapping.ramp_duration_samples = ramp_duration_samples;
                mapping.temporal_shape = shape;
                return;
            }
        }

        // Otherwise find a free slot
        for mapping in self.mappings.iter_mut() {
            if !mapping.active {
                mapping.macro_id = macro_id;
                mapping.target_id = target_id;
                mapping.param_id = param_id;
                mapping.scaling = scaling;
                mapping.ramp_duration_samples = ramp_duration_samples;
                mapping.temporal_shape = shape;
                mapping.active = true;
                return;
            }
        }
    }

    pub fn remove_mapping(&mut self, macro_id: u32, target_id: u64, param_id: u32) {
        for mapping in self.mappings.iter_mut() {
            if mapping.active && mapping.macro_id == macro_id && mapping.target_id == target_id && mapping.param_id == param_id {
                mapping.active = false;
            }
        }
    }

    pub fn expand_macro<F>(&self, macro_id: u32, value: f32, beat_pos: f64, mut f: F)
    where
        F: FnMut(u64, u32, f32, u32),
    {
        for mapping in self.mappings.iter() {
            if mapping.active && mapping.macro_id == macro_id {
                let mut val = value * mapping.scaling;

                if let Some(shape) = mapping.temporal_shape {
                    let phase = (beat_pos % 1.0) as f32; // 1-beat cycle
                    let modifier = match shape {
                        TemporalShape::Sine => (phase * 2.0 * std::f32::consts::PI).sin(),
                        TemporalShape::Saw => phase * 2.0 - 1.0,
                        TemporalShape::Square => {
                            if phase < 0.5 {
                                1.0
                            } else {
                                -1.0
                            }
                        }
                        TemporalShape::Triangle => {
                            if phase < 0.5 {
                                phase * 4.0 - 1.0
                            } else {
                                1.0 - (phase - 0.5) * 4.0
                            }
                        }
                    };
                    val *= modifier;
                }
                f(mapping.target_id, mapping.param_id, val, mapping.ramp_duration_samples);
            }
        }
    }
}

#[cfg(test)]
mod modulation_matrix_tests {
    use super::*;

    fn collect(matrix: &ModulationMatrix, macro_id: u32, value: f32, beat: f64) -> Vec<(u64, u32, f32, u32)> {
        let mut out = Vec::new();
        matrix.expand_macro(macro_id, value, beat, |t, p, v, r| out.push((t, p, v, r)));
        out
    }

    #[test]
    fn test_expand_macro_scales_and_fans_out() {
        let mut m = ModulationMatrix::new();
        m.add_mapping(1, 10, 0, 0.5, 64, None);
        m.add_mapping(1, 20, 3, -1.0, 0, None);
        m.add_mapping(2, 30, 0, 1.0, 0, None); // different macro, must not fire

        let fired = collect(&m, 1, 0.8, 0.0);
        assert_eq!(fired.len(), 2, "macro 1 fans out to its two targets only");
        assert_eq!(fired[0], (10, 0, 0.4, 64));
        assert_eq!(fired[1], (20, 3, -0.8, 0));
    }

    #[test]
    fn test_add_mapping_updates_existing_slot_in_place() {
        let mut m = ModulationMatrix::new();
        m.add_mapping(1, 10, 0, 0.5, 0, None);
        m.add_mapping(1, 10, 0, 2.0, 32, None); // same triple: update, not duplicate

        let fired = collect(&m, 1, 1.0, 0.0);
        assert_eq!(fired.len(), 1, "re-adding the same mapping must not duplicate it");
        assert_eq!(fired[0], (10, 0, 2.0, 32));
    }

    #[test]
    fn test_remove_mapping_frees_slot() {
        let mut m = ModulationMatrix::new();
        m.add_mapping(1, 10, 0, 1.0, 0, None);
        m.remove_mapping(1, 10, 0);
        assert!(collect(&m, 1, 1.0, 0.0).is_empty());

        // The freed slot is reusable
        m.add_mapping(3, 99, 7, 1.0, 0, None);
        assert_eq!(collect(&m, 3, 0.5, 0.0), vec![(99, 7, 0.5, 0)]);
    }

    #[test]
    fn test_temporal_shapes_modulate_over_the_beat() {
        let mut m = ModulationMatrix::new();
        m.add_mapping(1, 10, 0, 1.0, 0, Some(TemporalShape::Square));

        // Square: +1 in the first half of the beat, -1 in the second.
        assert_eq!(collect(&m, 1, 0.7, 0.25)[0].2, 0.7);
        assert_eq!(collect(&m, 1, 0.7, 0.75)[0].2, -0.7);

        // Sine: zero crossing at phase 0, positive peak at phase 0.25.
        let mut m = ModulationMatrix::new();
        m.add_mapping(1, 10, 0, 1.0, 0, Some(TemporalShape::Sine));
        assert!(collect(&m, 1, 1.0, 0.0)[0].2.abs() < 1e-6);
        assert!((collect(&m, 1, 1.0, 0.25)[0].2 - 1.0).abs() < 1e-6);

        // Triangle: -1 at phase 0, +1 at phase 0.5, back to -1 at 1.0-eps.
        let mut m = ModulationMatrix::new();
        m.add_mapping(1, 10, 0, 1.0, 0, Some(TemporalShape::Triangle));
        assert!((collect(&m, 1, 1.0, 0.0)[0].2 - -1.0).abs() < 1e-6);
        assert!((collect(&m, 1, 1.0, 0.5)[0].2 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_matrix_capacity_is_bounded_and_silent_on_overflow() {
        let mut m = ModulationMatrix::new();
        for i in 0..(MAX_MOD_MAPPINGS as u64 + 10) {
            m.add_mapping(1, i, 0, 1.0, 0, None);
        }
        let fired = collect(&m, 1, 1.0, 0.0);
        assert_eq!(fired.len(), MAX_MOD_MAPPINGS, "overflow must be dropped, not panic or overwrite");
    }
}

