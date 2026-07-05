use nullherz_traits::{Command, PerformanceCommand, MixerCommand, DeckParamType};
use nullherz_mixer::MixerManager;

pub struct MixerOrchestrator;

impl MixerOrchestrator {
    pub fn translate_command(cmd: &Command, mixer_manager: &MixerManager) -> Vec<Command> {
        let mut translated = Vec::new();
        match cmd {
            Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id, sample_id }) => {
                if let Some(nodes) = mixer_manager.deck_mappings.get(deck_id) {
                    translated.push(Command::Resource(nullherz_traits::ResourceCommand::AddSourceFromRegistry {
                        granular_node_idx: nodes.sampler_id,
                        sample_id: *sample_id,
                    }));
                }
            }
            Command::Mixer(MixerCommand::SetDeckParam { deck_id, param_type, value }) => {
                if let Some(nodes) = mixer_manager.deck_mappings.get(deck_id) {
                    match param_type {
                        DeckParamType::Gain => {
                            translated.push(Command::Mixer(MixerCommand::SetParam {
                                target_id: nodes.gain_id as u64,
                                param_id: 0,
                                value: *value,
                                ramp_duration_samples: 128,
                            }));
                        }
                        DeckParamType::EqLow => {
                            translated.push(Command::Mixer(MixerCommand::SetParam {
                                target_id: nodes.isolator_id as u64,
                                param_id: 0,
                                value: *value,
                                ramp_duration_samples: 0,
                            }));
                        }
                        DeckParamType::EqMid => {
                            translated.push(Command::Mixer(MixerCommand::SetParam {
                                target_id: nodes.isolator_id as u64,
                                param_id: 1,
                                value: *value,
                                ramp_duration_samples: 0,
                            }));
                        }
                        DeckParamType::EqHigh => {
                            translated.push(Command::Mixer(MixerCommand::SetParam {
                                target_id: nodes.isolator_id as u64,
                                param_id: 2,
                                value: *value,
                                ramp_duration_samples: 0,
                            }));
                        }
                        DeckParamType::Filter => {
                            translated.push(Command::Mixer(MixerCommand::SetParam {
                                target_id: nodes.filter_id as u64,
                                param_id: 0,
                                value: *value,
                                ramp_duration_samples: 128,
                            }));
                        }
                    }
                }
            }
            Command::Performance(PerformanceCommand::SyncDecks { source_deck: _, target_deck: _ }) => {
                // Future: implementation for BPM/Phase sync logic
            }
            Command::Performance(PerformanceCommand::PlayDeck { deck_id: _ }) => {
                // Future: individual deck play trigger
            }
            Command::Performance(PerformanceCommand::StopDeck { deck_id: _ }) => {
                // Future: individual deck stop trigger
            }
            _ => translated.push(*cmd),
        }
        translated
    }
}
