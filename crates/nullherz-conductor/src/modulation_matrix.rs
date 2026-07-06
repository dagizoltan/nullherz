use std::collections::HashMap;
use nullherz_traits::Command;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ModMapping {
    pub target_id: u64,
    pub param_id: u32,
    pub scaling: f32,
    pub ramp_duration_samples: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default)]
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
            // Pack into bundles of 8 commands each for atomic execution
            for chunk in mappings.chunks(8) {
                let mut data = [0u8; 128];
                let count = chunk.len();
                for (i, mapping) in chunk.iter().enumerate() {
                    let offset = i * 16;
                    let val = value * mapping.scaling;
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
