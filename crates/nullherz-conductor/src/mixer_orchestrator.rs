#![allow(clippy::collapsible_if)]
use nullherz_traits::{Command, PerformanceCommand, MixerCommand, DeckParamType};
use nullherz_mixer::MixerManager;

pub struct MixerOrchestrator;

use nullherz_dna::{LibraryDatabase, GeneticLibrary};
use std::sync::Arc;
use parking_lot::Mutex;

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
                    { let lib = library.lock();
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

                            // DNA-Aware Auto-Gain: Adjust target gain based on track energy
                            // Feature vector index 0 is assumed to be average RMS energy
                            let track_energy = track.metadata.dna.feature_vector[0];
                            if track_energy > 0.0 {
                                // Target normalization to 0.7 energy level
                                let energy_compensation = 0.7 / track_energy.max(0.1);
                                translated.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: nodes.gain_id as u64,
                                    param_id: 0,
                                    value: energy_compensation.clamp(0.5, 1.5),
                                    ramp_duration_samples: 44100, // 1s smooth transition
                                }));
                                println!("MixerOrchestrator: DNA-Aware Gain: Compensing by factor {}", energy_compensation);
                            }

                            // Groove Transfusion: Apply rhythmic micro-timing to the
                            // deck's sequencer node. Resolve the REAL node index by
                            // name — NodeConventions sequencer ids (70-73) are logical
                            // sentinels >= MAX_NODES, and commands aimed at them are
                            // silently dropped by the engine.
                            if let Some(&seq_node_idx) = mixer_manager.node_names.get(&format!("deck_{}_sequencer", deck_id.to_lowercase())) {
                                translated.extend(crate::pattern_manager::DnaSequencer::apply_groove(&track.metadata.dna.rhythmic, seq_node_idx, 0));
                            } else {
                                eprintln!("MixerOrchestrator: deck {} has no sequencer node; groove transfusion skipped.", deck_id);
                            }

                            // Formant-Driven EQ removed: it addressed nodes.filter_id
                            // as if it were a (freq, Q, gain) BiquadEQ, but that node
                            // is a RAW BiquadProcessor whose params 0..4 are the
                            // coefficients b0,b1,b2,a1,a2. The mapping wrote the DNA
                            // Q (hardcoded to 100 by analysis -> 100/100 = 1.0)
                            // straight into the pole coefficient a2, and the formant
                            // frequency (always > 10, so rejected) never landed at all.
                            // Result: poles at z = +/-j, exactly on the unit circle -
                            // an undamped ~fs/4 (11 kHz) oscillator that rang forever
                            // and did not stop when the deck stopped (the filter's IIR
                            // feedback sustains with no input). The feature never
                            // produced a real formant EQ; a correct version needs a
                            // dedicated peaking-EQ node with RBJ coefficients, not raw
                            // coefficients written onto the deck's DJ filter.
                        }
                    }
                }
            }
            Command::Mixer(MixerCommand::SetMacro { macro_id, value }) => {
                // STAGE 8: Semantic DNA-Macro Performance Links
                // If a macro is set, we check if it's bound to a Deck's timbral trajectory.
                // Convention: Macro IDs 100-103 map to DnaMorpher position of Decks A-D.
                if *macro_id >= 100 && *macro_id <= 103 {
                    let deck_id = match *macro_id {
                        100 => 'A',
                        101 => 'B',
                        102 => 'C',
                        103 => 'D',
                        _ => 'A',
                    };
                    if let Some(nodes) = mixer_manager.deck_mappings.get(&deck_id) {
                        if let Some(morph_id) = nodes.dna_morph_id {
                            translated.push(Command::Mixer(MixerCommand::SetParam {
                                target_id: morph_id as u64,
                                param_id: 0, // Morph Position
                                value: *value,
                                ramp_duration_samples: 1024,
                            }));
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
                    // Pressing play starts the clock: with the transport
                    // stopped, beat_position freezes at 0 and the quantize
                    // phase-lock drags every voice back to the track start.
                    translated.push(Command::Core(nullherz_traits::CoreCommand::Play));
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
