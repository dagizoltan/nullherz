use nullherz_traits::Command;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModulationMapping {
    pub target_id: u64,
    pub param_id: u32,
    pub scaling: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModulationMatrix {
    pub mappings: HashMap<u32, Vec<ModulationMapping>>,
}

impl ModulationMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_mapping(&mut self, macro_id: u32, target_id: u64, param_id: u32, scaling: f32) {
        let mapping = ModulationMapping {
            target_id,
            param_id,
            scaling,
        };
        let mappings = self.mappings.entry(macro_id).or_default();
        // Avoid duplicate mappings for same target/param
        mappings.retain(|m| m.target_id != target_id || m.param_id != param_id);
        mappings.push(mapping);
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
                expanded.push(Command::SetParam {
                    target_id: mapping.target_id,
                    param_id: mapping.param_id,
                    value: value * mapping.scaling,
                    ramp_duration_samples: 0,
                });
            }
        }
        expanded
    }
}
