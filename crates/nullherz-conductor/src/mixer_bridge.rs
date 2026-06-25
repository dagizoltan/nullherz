use crate::timeline::Timeline;
use nullherz_traits::Command;
use ipc_layer::Producer;
use crate::topology_manager::TopologyManager;
use crate::modulation_matrix::ModulationMatrix;

pub struct MixerBridge {
    pub timeline: Timeline,
    pub bundle_producer: Option<Producer<Vec<Command>>>,
}

impl MixerBridge {
    pub fn new() -> Self {
        Self {
            timeline: Timeline::default(),
            bundle_producer: None,
        }
    }

    pub fn apply_mixer_commands(&mut self, commands: Vec<Command>, topology_manager: &mut TopologyManager, modulation_matrix: &mut ModulationMatrix) {
        let mut bundle = Vec::with_capacity(commands.len());

        for cmd in commands {
            if topology_manager.handle_topology_command(&cmd) {
                continue;
            }

            match cmd {
                Command::SetMacro { macro_id, value } => {
                    let expanded = modulation_matrix.expand_macro(macro_id, value);
                    bundle.extend(expanded);
                }
                Command::AddModMapping { macro_id, target_id, param_id, scaling, ramp_duration_samples } => {
                    modulation_matrix.add_mapping(macro_id, target_id, param_id, scaling, ramp_duration_samples);
                }
                Command::RemoveModMapping { macro_id, target_id, param_id } => {
                    modulation_matrix.remove_mapping(macro_id, target_id, param_id);
                }
                _ => {
                    bundle.push(cmd);
                }
            }
        }

        if !bundle.is_empty() {
            if let Some(ref mut prod) = self.bundle_producer {
                let _ = prod.push(bundle);
            }
        }
    }

    pub fn update_timeline(&mut self, telemetry: &audio_core::Telemetry) {
        self.timeline.update(telemetry);
    }
}
