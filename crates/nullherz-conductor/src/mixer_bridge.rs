use crate::timeline::Timeline;
use nullherz_traits::Command;
use ipc_layer::{Producer, Consumer};
use crate::topology_manager::TopologyManager;
use crate::modulation_matrix::ModulationMatrix;

pub struct MixerBridge {
    pub timeline: Timeline,
    pub bundle_producer: Option<Producer<Vec<Command>>>,
    pub bundle_pool: Option<Consumer<Vec<Command>>>,
    /// Local stash for unused bundles to prevent global pool depletion.
    local_stash: Vec<Vec<Command>>,
}

impl Default for MixerBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl MixerBridge {
    pub fn new() -> Self {
        Self {
            timeline: Timeline::default(),
            bundle_producer: None,
            bundle_pool: None,
            local_stash: Vec::with_capacity(4),
        }
    }

    pub fn apply_mixer_commands(&mut self, commands: Vec<Command>, topology_manager: &mut TopologyManager, modulation_matrix: &mut ModulationMatrix) {
        if commands.is_empty() { return; }

        let mut bundle = self.local_stash.pop()
            .or_else(|| self.bundle_pool.as_mut().and_then(|c| c.pop()))
            .unwrap_or_else(|| Vec::with_capacity(16));
        bundle.clear();

        for cmd in commands {
            if topology_manager.handle_topology_command(&cmd) {
                continue;
            }

            match &cmd {
                Command::Mixer(nullherz_traits::MixerCommand::SetMacro { macro_id, value }) => {
                    modulation_matrix.update_macro_value(*macro_id, *value);
                    bundle.push(cmd);
                }
                Command::Mixer(nullherz_traits::MixerCommand::AddModMapping { macro_id, target_id, param_id, scaling, ramp_duration_samples }) => {
                    modulation_matrix.add_mapping(*macro_id, *target_id, *param_id, *scaling, *ramp_duration_samples);
                    bundle.push(cmd);
                }
                Command::Mixer(nullherz_traits::MixerCommand::RemoveModMapping { macro_id, target_id, param_id }) => {
                    modulation_matrix.remove_mapping(*macro_id, *target_id, *param_id);
                    bundle.push(cmd);
                }
                _ => {
                    bundle.push(cmd);
                }
            }
        }

        if !bundle.is_empty() {
            if let Some(ref mut prod) = self.bundle_producer {
                if let Err(returned_bundle) = prod.push(bundle) {
                    // Producer full, return to local stash
                    if self.local_stash.len() < 8 {
                        self.local_stash.push(returned_bundle);
                    }
                }
            } else {
                // No producer, return to local stash
                if self.local_stash.len() < 8 {
                    self.local_stash.push(bundle);
                }
            }
        } else {
            // Bundle was empty (all commands were topology/mod-matrix), return to stash
            if self.local_stash.len() < 8 {
                self.local_stash.push(bundle);
            }
        }
    }

    pub fn update_timeline(&mut self, telemetry: &audio_core::Telemetry) {
        self.timeline.update(telemetry);
    }
}
