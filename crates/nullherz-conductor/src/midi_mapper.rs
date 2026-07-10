use nullherz_traits::{MidiEvent, Command, MidiMap, MidiTarget};
use std::sync::Mutex;

pub struct MidiMapper {
    pub active_map: Option<MidiMap>,
    /// Cache for Most Significant Byte (MSB) of 14-bit CC messages (CC 0-31).
    pub pending_14bit_msb: Mutex<std::collections::HashMap<u8, u8>>,
    /// Cache of last known parameter values to implement Soft Takeover.
    /// Key: (target_id, param_id), Value: f32
    pub parameter_cache: Mutex<std::collections::HashMap<(u64, u32), f32>>,
}

impl Default for MidiMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiMapper {
    pub fn new() -> Self {
        Self {
            active_map: None,
            pending_14bit_msb: Mutex::new(std::collections::HashMap::new()),
            parameter_cache: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn load_from_json(&mut self, json: &str) -> Result<(), Box<dyn std::error::Error>> {
        let map: MidiMap = serde_json::from_str(json)?;
        self.active_map = Some(map);
        Ok(())
    }

    fn check_soft_takeover(&self, target_id: u64, param_id: u32, new_value: f32) -> bool {
        let mut cache = self.parameter_cache.lock().unwrap();
        if let Some(&last_val) = cache.get(&(target_id, param_id)) {
            // Tolerance for "crossing" the value (approx 5% window)
            if (new_value - last_val).abs() < 0.05 {
                cache.insert((target_id, param_id), new_value);
                return true;
            }
            false
        } else {
            // Initialize cache on first move
            cache.insert((target_id, param_id), new_value);
            true
        }
    }

    pub fn update_parameter_cache(&self, target_id: u64, param_id: u32, value: f32) {
        let mut cache = self.parameter_cache.lock().unwrap();
        cache.insert((target_id, param_id), value);
    }

    pub fn translate(&self, event: &MidiEvent, node_names: &std::collections::HashMap<String, u32>, focused_node_idx: Option<u32>) -> Vec<Command> {
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
                let mut target_val;

                // 1. Handle 14-bit CC Pairing (Stage 2 High-Res)
                if event.data1 < 32 {
                    // MSB received, cache it and wait for LSB (or use as coarse if no LSB follows)
                    let mut msb_cache = self.pending_14bit_msb.lock().unwrap();
                    msb_cache.insert(event.data1, event.data2);
                }

                for ctrl in &map.controls {
                    if ctrl.cc_number == event.data1 {
                        // Check if this control is 14-bit (LSB CC 32-63)
                        if event.data1 >= 32 && event.data1 < 64 {
                            let msb_cc = event.data1 - 32;
                            let msb_val_opt = {
                                let msb_cache = self.pending_14bit_msb.lock().unwrap();
                                msb_cache.get(&msb_cc).copied()
                            };
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
                                if self.check_soft_takeover(*target_id, *param_id, target_val) {
                                    commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                        target_id: *target_id,
                                        param_id: *param_id,
                                        value: target_val,
                                        ramp_duration_samples: 128
                                    }));
                                }
                            }
                            MidiTarget::NamedParam { node_name, param_id } => {
                                if let Some(&target_id) = node_names.get(node_name) {
                                    if self.check_soft_takeover(target_id as u64, *param_id, target_val) {
                                        commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                            target_id: target_id as u64,
                                            param_id: *param_id,
                                            value: target_val,
                                            ramp_duration_samples: 128
                                        }));
                                    }
                                }
                            }
                            MidiTarget::FocusedParam { param_id } => {
                                if let Some(target_id) = focused_node_idx {
                                    if self.check_soft_takeover(target_id as u64, *param_id, target_val) {
                                        commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                            target_id: target_id as u64,
                                            param_id: *param_id,
                                            value: target_val,
                                            ramp_duration_samples: 128
                                        }));
                                    }
                                }
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
