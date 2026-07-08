use nullherz_traits::{MidiEvent, Command, MidiMap, MidiTarget};
use std::cell::UnsafeCell;

pub struct MidiMapper {
    pub active_map: Option<MidiMap>,
    /// Cache for Most Significant Byte (MSB) of 14-bit CC messages (CC 0-31).
    pub pending_14bit_msb: UnsafeCell<std::collections::HashMap<u8, u8>>,
}

impl Default for MidiMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiMapper {
    pub fn new() -> Self {
        Self { active_map: None, pending_14bit_msb: UnsafeCell::new(std::collections::HashMap::new()) }
    }

    pub fn load_from_json(&mut self, json: &str) -> Result<(), Box<dyn std::error::Error>> {
        let map: MidiMap = serde_json::from_str(json)?;
        self.active_map = Some(map);
        Ok(())
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
                let mut target_val = 0.0;
                let mut found = false;

                // 1. Handle 14-bit CC Pairing (Stage 2 High-Res)
                if event.data1 < 32 {
                    // MSB received, cache it and wait for LSB (or use as coarse if no LSB follows)
                    unsafe { (*self.pending_14bit_msb.get()).insert(event.data1, event.data2); }
                }

                for ctrl in &map.controls {
                    if ctrl.cc_number == event.data1 {
                        found = true;

                        // Check if this control is 14-bit (LSB CC 32-63)
                        if event.data1 >= 32 && event.data1 < 64 {
                            let msb_cc = event.data1 - 32;
                            let msb_val_opt = unsafe { (*self.pending_14bit_msb.get()).get(&msb_cc).copied() };
                            if let Some(msb_val) = msb_val_opt {
                                let combined = ((msb_val as u16) << 7) | (event.data2 as u16);
                                let val_norm = combined as f32 / 16383.0;
                                target_val = ctrl.min_val + val_norm * (ctrl.max_val - ctrl.min_val);
                            } else {
                                // Fallback to 7-bit LSB only (rare)
                                let val_norm = event.data2 as f32 / 127.0;
                                target_val = ctrl.min_val + val_norm * (ctrl.max_val - ctrl.min_val);
                            }
                        } else {
                            // Standard 7-bit CC
                            let val_norm = event.data2 as f32 / 127.0;
                            target_val = ctrl.min_val + val_norm * (ctrl.max_val - ctrl.min_val);
                        }

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
