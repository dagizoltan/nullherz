use nullherz_traits::{MidiEvent, Command, MidiMap, MidiTarget};

pub struct MidiMapper {
    pub active_map: Option<MidiMap>,
}

impl Default for MidiMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiMapper {
    pub fn new() -> Self {
        Self { active_map: None }
    }

    pub fn translate(&self, event: &MidiEvent) -> Vec<Command> {
        let mut commands = Vec::new();
        let Some(ref map) = self.active_map else { return commands; };

        let status = event.status & 0xF0;

        match status {
            0x90 => { // Note On
                if event.data2 > 0 {
                    for trigger in &map.triggers {
                        if trigger.note_number == event.data1 {
                            match &trigger.target {
                                MidiTarget::Command(cmd) => commands.push(*cmd),
                                _ => {}
                            }
                        }
                    }
                }
            }
            0xB0 => { // Control Change
                for ctrl in &map.controls {
                    if ctrl.cc_number == event.data1 {
                        let val_norm = event.data2 as f32 / 127.0;
                        let target_val = ctrl.min_val + val_norm * (ctrl.max_val - ctrl.min_val);

                        match &ctrl.target {
                            MidiTarget::Param { target_id, param_id } => {
                                commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: *target_id,
                                    param_id: *param_id,
                                    value: target_val,
                                    ramp_duration_samples: 128
                                }));
                            }
                            MidiTarget::Macro { macro_id } => {
                                commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetMacro {
                                    macro_id: *macro_id,
                                    value: target_val
                                }));
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }

        commands
    }
}
