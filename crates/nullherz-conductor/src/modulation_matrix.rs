use std::collections::HashMap;
use nullherz_traits::Command;

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Archive, RkyvDeserialize, RkyvSerialize)]
#[archive(check_bytes)]
pub struct ModMapping {
    pub target_id: u64,
    pub param_id: u32,
    pub scaling: f32,
    pub ramp_duration_samples: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default, Archive, RkyvDeserialize, RkyvSerialize)]
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
        let entry = self.mappings.entry(macro_id).or_insert_with(Vec::new);
        entry.push(ModMapping {
            target_id,
            param_id,
            scaling,
            ramp_duration_samples,
        });
    }

    pub fn remove_mapping(&mut self, macro_id: u32, target_id: u64, param_id: u32) {
        if let Some(mappings) = self.mappings.get_mut(&macro_id) {
            mappings.retain(|m| m.target_id != target_id || m.param_id != param_id);
        }
    }

    pub fn expand_macro(&self, macro_id: u32, value: f32) -> Vec<Command> {
        let mut expanded = Vec::new();
        if let Some(mappings) = self.mappings.get(&macro_id) {
            for mapping in mappings {
                expanded.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                    target_id: mapping.target_id,
                    param_id: mapping.param_id,
                    value: value * mapping.scaling,
                    ramp_duration_samples: mapping.ramp_duration_samples,
                }));
            }
        }
        expanded
    }
}
