use nullherz_traits::{Command, PerformanceCommand, MixerCommand, DeckParamType};
use nullherz_mixer::MixerManager;

pub struct MixerOrchestrator;

use nullherz_dna::{LibraryDatabase, GeneticLibrary};
use std::sync::{Arc, Mutex};

impl MixerOrchestrator {
    pub fn translate_command(cmd: &Command, mixer_manager: &MixerManager, library: &Arc<Mutex<LibraryDatabase>>) -> Vec<Command> {
        let mut translated = Vec::new();
        match cmd {
            Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id, sample_id }) => {
                if let Some(nodes) = mixer_manager.deck_mappings.get(deck_id) {
                    translated.push(Command::Resource(nullherz_traits::ResourceCommand::AddSourceFromRegistry {
                        granular_node_idx: nodes.sampler_id,
                        sample_id: *sample_id,
                    }));

                    // Intelligent Auto-Sync: Resolve track BPM and notify target deck
                    if let Ok(lib) = library.lock() {
                        if let Ok(Some(track)) = lib.get_track(*sample_id) {
                            if track.metadata.bpm > 0.0 {
                                translated.push(Command::Core(nullherz_traits::CoreCommand::SetBpm(track.metadata.bpm)));
                                // Future: also emit SyncDecks if global sync is enabled
                            }

                            // Harmonic Auto-Sync: Align to Master Deck Key
                            if let Some(track_key) = track.metadata.root_key {
                                // For now, assume master key is C (0.0). In production, this would resolve from active_master_deck.
                                let master_key = 0.0f32;
                                let mut diff = master_key - track_key;
                                while diff > 6.0 { diff -= 12.0; }
                                while diff < -6.0 { diff += 12.0; }

                                translated.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: nodes.keysync_id as u64,
                                    param_id: 0,
                                    value: diff,
                                    ramp_duration_samples: 1024,
                                }));
                                println!("MixerOrchestrator: Harmonic Sync: Shifted Deck {} by {} semitones", deck_id, diff);
                            }
                        }
                    }
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
                        DeckParamType::Pan => {
                            translated.push(Command::Mixer(MixerCommand::SetParam {
                                target_id: nodes.stereo_util_id as u64,
                                param_id: 0,
                                value: *value,
                                ramp_duration_samples: 128,
                            }));
                        }
                        DeckParamType::Width => {
                            translated.push(Command::Mixer(MixerCommand::SetParam {
                                target_id: nodes.stereo_util_id as u64,
                                param_id: 1,
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
            Command::Performance(PerformanceCommand::PlayDeck { deck_id }) => {
                if let Some(nodes) = mixer_manager.deck_mappings.get(deck_id) {
                    translated.push(Command::Performance(PerformanceCommand::PlayNode { node_idx: nodes.sampler_id }));
                }
            }
            Command::Performance(PerformanceCommand::StopDeck { deck_id }) => {
                if let Some(nodes) = mixer_manager.deck_mappings.get(deck_id) {
                    translated.push(Command::Performance(PerformanceCommand::StopNode { node_idx: nodes.sampler_id }));
                }
            }
            Command::Performance(PerformanceCommand::SetSequencerStep { .. }) |
            Command::Performance(PerformanceCommand::JumpToHotCue { .. }) |
            Command::Performance(PerformanceCommand::EvolvePattern { .. }) |
            Command::Performance(PerformanceCommand::ClearTrackPattern { .. }) => {
                translated.push(*cmd);
            }
            _ => translated.push(*cmd),
        }
        translated
    }
}
