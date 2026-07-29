#![allow(clippy::collapsible_if)]
use nullherz_traits::{Command, PerformanceCommand, MixerCommand, DeckParamType};
use nullherz_mixer::MixerManager;
use nullherz_processors::sampler::PARAM_QUANTIZE as SAMPLER_PARAM_QUANTIZE;

pub struct MixerOrchestrator;

use nullherz_dna::{LibraryDatabase, GeneticLibrary};
use std::sync::Arc;
use parking_lot::Mutex;

/// The root key of whatever track is on the master deck, if it is knowable.
///
/// Takes an already-locked library: every caller here is inside a lock scope
/// already, and `parking_lot::Mutex` is not reentrant — re-locking would
/// deadlock the command path rather than fail visibly.
fn master_root_key(mixer_manager: &MixerManager, lib: &LibraryDatabase) -> Option<f32> {
    let sample_id = mixer_manager.deck_samples.get(&mixer_manager.active_master_deck)?;
    lib.get_track_facets(*sample_id).ok().flatten()?.root_key
}

/// Semitones to shift `deck_id` so it lands on the master deck's key.
///
/// **Returns 0.0 whenever the shift is unknowable** — the deck IS the master,
/// no track on the master, or either side has no analysed key. Playing the file
/// as recorded is the only safe answer to "I don't know what key to match".
///
/// This is the whole bug: the previous code substituted a hardcoded C for the
/// master key, so a track that had simply never been key-analysed against
/// anything still got transposed toward C — measured at −5 semitones. An unknown
/// key must mean "don't touch it", never "assume C".
fn harmonic_shift(
    deck_id: char,
    track_key: Option<f32>,
    mixer_manager: &MixerManager,
    lib: &LibraryDatabase,
) -> f32 {
    if deck_id == mixer_manager.active_master_deck {
        return 0.0;
    }
    let (Some(track_key), Some(master_key)) = (track_key, master_root_key(mixer_manager, lib)) else {
        return 0.0;
    };
    // Shortest path around the 12-semitone circle: never transpose more than a
    // tritone to reach the same pitch class.
    let mut diff = master_key - track_key;
    while diff > 6.0 { diff -= 12.0; }
    while diff < -6.0 { diff += 12.0; }
    diff
}

/// Re-shift every KEY-latched deck against the CURRENT master key.
///
/// Needed because the master key is no longer a constant: loading a new track on
/// the master deck, or moving master to another deck, silently invalidates the
/// shift every latched deck is holding. Without this the KEY light stays on
/// while the deck is matched to a track that is no longer the reference.
///
/// `except` skips a deck whose shift the caller has already emitted in this same
/// translation, so a load onto the master deck does not produce two `SetParam`s
/// for it. Note this deliberately does NOT skip the master deck in general: when
/// master MOVES, the new master is usually a deck that was following, and it
/// needs the re-centre to 0 that `harmonic_shift` returns for it.
fn realign_key_latched_decks(
    mixer_manager: &MixerManager,
    lib: &LibraryDatabase,
    except: Option<char>,
) -> Vec<Command> {
    let mut out = Vec::new();
    for deck_id in &mixer_manager.key_sync_decks {
        if Some(*deck_id) == except {
            continue;
        }
        let Some(nodes) = mixer_manager.deck_mappings.get(deck_id) else { continue };
        let track_key = mixer_manager
            .deck_samples
            .get(deck_id)
            .and_then(|id| lib.get_track_facets(*id).ok().flatten())
            .and_then(|f| f.root_key);
        out.push(Command::Mixer(MixerCommand::SetParam {
            target_id: nodes.keysync_id as u64,
            param_id: 0,
            value: harmonic_shift(*deck_id, track_key, mixer_manager, lib),
            ramp_duration_samples: 1024,
        }));
    }
    out
}

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

                    // RAW BY DEFAULT.
                    //
                    // Loading a track must not change how it sounds. Everything
                    // below is now gated on a per-deck latch the user set
                    // deliberately; with no latch engaged a load is exactly one
                    // thing — the audio arrives at its native tempo and pitch.
                    //
                    // What used to happen on EVERY load, unasked:
                    //   * `Core::SetBpm(track.bpm)` — global, so the last deck
                    //     loaded re-tempoed the console and stretched every other
                    //     playing deck to match it.
                    //   * harmonic key-sync toward a master key hardcoded to C,
                    //     measured at −5 semitones on real material.
                    //   * groove transfusion onto the deck's sequencer.
                    // Two of the three had no UI affordance at all, so there was
                    // no way to decline them or even to see that they had fired.
                    let sync_on = mixer_manager.sync_decks.contains(deck_id);
                    let key_on = mixer_manager.key_sync_decks.contains(deck_id);

                    // State the deck's tempo mode explicitly on every load rather
                    // than trusting the sampler's construction default: a node
                    // rebuilt by a topology change would otherwise silently revert.
                    translated.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                        target_id: nodes.sampler_id as u64,
                        param_id: SAMPLER_PARAM_QUANTIZE,
                        value: if sync_on { 1.0 } else { 0.0 },
                        ramp_duration_samples: 0,
                    }));

                    // FACETS, not the full row: everything below is bpm, key and
                    // DNA. `get_track` would parse the whole JSON row including
                    // peaks/MIPs/band waveform — 61 ms for a 6-minute track
                    // against 2.5 ms for a 17-second one, on the latency-critical
                    // command path, while holding the library mutex.
                    //
                    // Skipped entirely in RAW: with both latches off there is
                    // nothing here to read, so the lock is never taken.
                    if sync_on || key_on {
                        let lib = library.lock();
                        if let Ok(Some(track)) = lib.get_track_facets(*sample_id) {
                            if sync_on && track.bpm > 0.0 {
                                translated.push(Command::Core(nullherz_traits::CoreCommand::SetBpm(track.bpm)));
                            }

                            // Harmonic sync: align to the MASTER DECK's key,
                            // resolved from whatever track is actually on it.
                            // Unknowable (no master track, no analysed key,
                            // or this IS the master) means 0 semitones — see
                            // `harmonic_shift`.
                            if key_on {
                                let diff = harmonic_shift(*deck_id, track.root_key, mixer_manager, &lib);
                                translated.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                    target_id: nodes.keysync_id as u64,
                                    param_id: 0,
                                    value: diff,
                                    ramp_duration_samples: 1024,
                                }));
                                println!("MixerOrchestrator: KEY on: shifted deck {} by {} semitones", deck_id, diff);
                            }

                            // DNA-Aware Auto-Gain REMOVED — it had no correct input.
                            //
                            // It read `feature_vector[0]` as "average RMS energy"
                            // and set deck gain to `0.7 / that`. But that slot is a
                            // BAND ENERGY RATIO, not loudness, so the intent was
                            // wrong to begin with: a bass-light track would have
                            // been boosted and a bass-heavy one cut, which inverts
                            // tonal balance rather than matching level.
                            //
                            // In practice it never ran at all. The band layout gave
                            // slot 0 a zero-width bin range, so the value was
                            // structurally 0.0 for every track and the `> 0.0`
                            // guard always failed. (The layout is fixed now, so
                            // reviving this as-is would start silently applying
                            // that inverted gain to every load.)
                            //
                            // Loudness matching needs a real loudness measure —
                            // LUFS/K-weighting — which does not exist yet; it is
                            // specified for the SM DNA layer. It should also be
                            // OPT-IN when it returns, alongside the other
                            // automatic load-time processing (see RAW mode).

                            // Groove Transfusion: apply the track's rhythmic
                            // micro-timing to the deck's sequencer node. Rides the
                            // SYNC latch — it displaces step fire times, which is a
                            // timing decision and so belongs with tempo, not with
                            // plain playback. Resolve the REAL node index by name:
                            // NodeConventions sequencer ids (70-73) are logical
                            // sentinels >= MAX_NODES, and commands aimed at them are
                            // silently dropped by the engine.
                            if sync_on {
                                if let Some(&seq_node_idx) = mixer_manager.node_names.get(&format!("deck_{}_sequencer", deck_id.to_lowercase())) {
                                    translated.extend(crate::pattern_manager::DnaSequencer::apply_groove(&track.dna.rhythmic, seq_node_idx, 0));
                                } else {
                                    eprintln!("MixerOrchestrator: deck {} has no sequencer node; groove transfusion skipped.", deck_id);
                                }
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

                    // A new track on the MASTER deck moves the reference key, so
                    // every KEY-latched deck is now matched to a track that is no
                    // longer the master. Re-align them.
                    //
                    // Outside the latch check above on purpose: this fires on the
                    // master deck's own latch state being irrelevant — the master
                    // is usually RAW (it defines the key, it does not follow one),
                    // and gating this on the master's latches would mean the
                    // common case never re-aligned anything.
                    if *deck_id == mixer_manager.active_master_deck && !mixer_manager.key_sync_decks.is_empty() {
                        let lib = library.lock();
                        translated.extend(realign_key_latched_decks(mixer_manager, &lib, Some(*deck_id)));
                    }
                }
            }
            // Moving master moves the reference key for every latched deck.
            Command::Core(nullherz_traits::CoreCommand::SetMasterDeck(_)) => {
                translated.push(*cmd);
                if !mixer_manager.key_sync_decks.is_empty() {
                    let lib = library.lock();
                    translated.extend(realign_key_latched_decks(mixer_manager, &lib, None));
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
            // The latch state itself is updated in `CommandHandler` (this
            // function only has `&MixerManager`); what happens here is making the
            // engine agree with it immediately, so pressing SYNC or KEY acts on
            // the track already on the deck instead of only on the next load.
            Command::Performance(PerformanceCommand::SetDeckSync { deck_id, enabled }) => {
                if let Some(nodes) = mixer_manager.deck_mappings.get(deck_id) {
                    translated.push(Command::Mixer(MixerCommand::SetParam {
                        target_id: nodes.sampler_id as u64,
                        param_id: SAMPLER_PARAM_QUANTIZE,
                        value: if *enabled { 1.0 } else { 0.0 },
                        ramp_duration_samples: 0,
                    }));
                    // Engaging SYNC adopts the deck's own tempo as the transport
                    // tempo, which is what makes the button do something audible
                    // on the deck the user pressed it on. Disengaging deliberately
                    // leaves the transport where it is: silently re-tempoing the
                    // console on a release would yank every OTHER synced deck.
                    if *enabled && let Some(sample_id) = mixer_manager.deck_samples.get(deck_id) {
                        let lib = library.lock();
                        if let Ok(Some(track)) = lib.get_track_facets(*sample_id)
                            && track.bpm > 0.0 {
                                translated.push(Command::Core(nullherz_traits::CoreCommand::SetBpm(track.bpm)));
                            }
                    }
                }
            }
            Command::Performance(PerformanceCommand::SetDeckKeySync { deck_id, enabled }) => {
                if let Some(nodes) = mixer_manager.deck_mappings.get(deck_id) {
                    // Releasing KEY must re-centre the node at 0 semitones, not
                    // just stop updating it — otherwise the last shift stays
                    // applied and the deck is still transposed with the light off.
                    let mut semitones = 0.0f32;
                    if *enabled && let Some(sample_id) = mixer_manager.deck_samples.get(deck_id) {
                        let lib = library.lock();
                        let track_key = lib.get_track_facets(*sample_id).ok().flatten().and_then(|f| f.root_key);
                        semitones = harmonic_shift(*deck_id, track_key, mixer_manager, &lib);
                    }
                    translated.push(Command::Mixer(MixerCommand::SetParam {
                        target_id: nodes.keysync_id as u64,
                        param_id: 0,
                        value: semitones,
                        ramp_duration_samples: 1024,
                    }));
                }
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
