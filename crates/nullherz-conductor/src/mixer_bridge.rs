use crate::timeline::Timeline;
use nullherz_traits::Command;
use ipc_layer::Producer;
use crate::topology_manager::TopologyManager;

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

    pub fn apply_mixer_commands(&mut self, commands: Vec<Command>, topology_manager: &mut TopologyManager) {
        let mut bundle = Vec::with_capacity(commands.len());

        for cmd in commands {
            if topology_manager.handle_topology_command(&cmd) {
                continue;
            }
            bundle.push(cmd);
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
