use crate::orchestrator::Conductor;
use nullherz_traits::{Command, CoreCommand, PerformanceCommand, ResourceCommand, DnaCommand};
use std::sync::Arc;
use nullherz_dna::GeneticLibrary;

pub struct CommandHandler;

impl CommandHandler {
    pub fn apply_mixer_commands(conductor: &mut Conductor, commands: Vec<Command>) {
        let mut final_commands = Vec::new();

        // 1. Intercept DJ Deck Commands and Translate them
        let mut translated_commands = Vec::new();
        for cmd in &commands {
            translated_commands.extend(crate::mixer_orchestrator::MixerOrchestrator::translate_command(cmd, &conductor.mixer_manager, &conductor.library));
        }

        // Broadcast to remote nodes (Distributed Control Plane)
        for cmd in &commands {
            let ts_cmd = nullherz_traits::TimestampedCommand {
                timestamp_samples: 0,
                command: cmd.clone(),
            };
            let remote_manager = conductor.sidecar_supervisor.remote_manager.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let mut manager = remote_manager.lock().await;
                    manager.broadcast_command(ts_cmd).await;
                });
            }
        }

        for cmd in translated_commands {
            let handled = match cmd {
                Command::Core(core_cmd) => Self::handle_core_command(conductor, core_cmd),
                Command::Performance(perf_cmd) => Self::handle_performance_command(conductor, perf_cmd),
                Command::Resource(res_cmd) => Self::handle_resource_command(conductor, res_cmd),
                Command::Dna(dna_cmd) => Self::handle_dna_command(conductor, dna_cmd),
                _ => false,
            };

            if !handled {
                final_commands.push(cmd);
            }
        }
        if !final_commands.is_empty() {
            conductor.mixer_bridge.apply_mixer_commands(final_commands, &mut conductor.topology_manager, &mut conductor.modulation_matrix);
        }
    }

    fn handle_core_command(conductor: &mut Conductor, cmd: CoreCommand) -> bool {
        match cmd {
            CoreCommand::SwitchBackend(backend_type) => {
                let _ = conductor.switch_backend(backend_type);
                true
            }
            CoreCommand::Pause => {
                if let Some(ref prod) = conductor.engine_coordinator.command_producer {
                    let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                        timestamp_samples: 0,
                        command: Command::Core(CoreCommand::Stop),
                    });
                }
                true
            }
            CoreCommand::Resume => {
                if let Some(ref prod) = conductor.engine_coordinator.command_producer {
                    let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                        timestamp_samples: 0,
                        command: Command::Core(CoreCommand::Play),
                    });
                }
                true
            }
            CoreCommand::SetMasterDeck(deck_id) => {
                conductor.active_master_deck = deck_id;
                println!("Conductor: Master Deck set to {}", deck_id);
                conductor.trigger_matchmaking_suggestions(); // Trigger immediate update
                true
            }
            CoreCommand::LoadMidiMap(buffer) => {
                let name = String::from_utf8_lossy(&buffer).trim_matches(char::from(0)).to_string();
                let path = format!("mappings/{}.json", name);
                if let Ok(json) = std::fs::read_to_string(path) {
                    let _ = conductor.midi_mapper.load_from_json(&json);
                }
                true
            }
            CoreCommand::SetMidiPorts(buffer) => {
                let ports_str = String::from_utf8_lossy(&buffer).trim_matches(char::from(0)).to_string();
                let ports: Vec<String> = ports_str.split(',').filter(|s| !s.is_empty()).map(|s| s.trim().to_string()).collect();
                let _ = conductor.update_system_config(None, Some(ports), None);
                true
            }
            CoreCommand::CalibrateLatency => {
                let sample_rate = {
                    let engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
                    engine_lock.ok().and_then(|lock| lock.as_ref().map(|e| e.target_sample_rate())).unwrap_or(44100.0)
                };
                let samples = (sample_rate * 0.01) as u32;
                conductor.calibration_samples = samples;
                let _ = conductor.update_system_config(None, None, Some(samples));
                true
            }
            CoreCommand::HotLoadSidecar { name, node_idx } => {
                let plugin_name = String::from_utf8_lossy(&name).trim_matches(char::from(0)).to_string();
                let manifest = {
                    let known = conductor.sidecar_discovery.known_plugins.lock();
                    known.ok().and_then(|lock| lock.get(&plugin_name).cloned())
                };
                if let Some(m) = manifest {
                    let binary_path = format!("plugins/{}", m.binary_name);
                    match conductor.sidecar_supervisor.manager.spawn_sidecar(&plugin_name, &binary_path, node_idx, 2, fx_runtime::FailurePolicy::AutoRestart) {
                        Ok(processor) => {
                            if let Some(ref mut prod) = conductor.topology_manager.topo_producer {
                                let _ = prod.push(nullherz_traits::TopologyMutation::SwapProcessor { node_idx, processor });
                            }
                        }
                        Err(e) => eprintln!("Failed to hot-load sidecar {}: {}", plugin_name, e),
                    }
                } else {
                    eprintln!("Hot-load failed: plugin manifest for {} not found.", plugin_name);
                }
                true
            }
            CoreCommand::ExportAudio { filename, duration_seconds } => {
                let name = String::from_utf8_lossy(&filename).trim_matches(char::from(0)).to_string();
                eprintln!("Bounce: Offline Export requested for {}. Initializing bounce engine...", name);
                let state = conductor.capture_state();
                let mut renderer = crate::bounce::OfflineRenderer::new(state);
                let filename_clone = name.clone();
                tokio::task::spawn_blocking(move || {
                    let _ = renderer.bounce_to_wav(&filename_clone, duration_seconds);
                });
                true
            }
            CoreCommand::SetSafeMode(enabled) => {
                if let Some(ref mut prod) = conductor.engine_coordinator.command_producer {
                    let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                        timestamp_samples: 0,
                        command: Command::Core(CoreCommand::SetSafeMode(enabled)),
                    });
                }
                true
            }
            _ => false,
        }
    }

    fn handle_performance_command(conductor: &mut Conductor, cmd: PerformanceCommand) -> bool {
        match cmd {
            PerformanceCommand::LaunchClip { row, col } => {
                if row == 0xFF {
                    for r in 0..8 {
                        conductor.clip_orchestrator.launch_clip(r, col as usize);
                    }
                } else {
                    conductor.clip_orchestrator.launch_clip(row as usize, col as usize);
                }
                true
            }
            PerformanceCommand::TransfuseRow { row } => {
                let mutations = conductor.clip_orchestrator.transfuse_row(row as usize);
                for m in mutations {
                    if let Some(ref mut prod) = conductor.topology_manager.topo_producer {
                        let _ = prod.push(m);
                    }
                }
                true
            }
            PerformanceCommand::EvolvePattern { node_idx, track_idx, mutation_strength } => {
                let mut dna = nullherz_traits::RhythmicDNA::default();
                let mut sample_id = None;
                {
                    if let Ok(engine_lock) = conductor.engine_coordinator.backend_manager.engine_handle.lock() {
                        if let Some(ref engine) = *engine_lock {
                            let resource_id = engine.list_children().iter()
                                .find(|c| c.metadata().map(|m| m.processor_id as u32) == Some(node_idx))
                                .and_then(|c| c.resource_id());
                            if let Some(rid) = resource_id {
                                sample_id = Some(rid);
                                if let Some(s) = conductor.transfusion_manager.sample_registry.get(rid) {
                                    dna = (*s.metadata).dna.rhythmic.clone();
                                }
                            }
                        }
                    }
                }
                if let Some(rid) = sample_id {
                    if let Ok(lib) = conductor.library.lock() {
                        if let Ok(Some(track)) = lib.get_track(rid) {
                            dna = track.metadata.dna.rhythmic.clone();
                        }
                    }
                }
                let commands = crate::genetic_sequencer::GeneticSequencer::evolve_pattern(&dna, node_idx, track_idx, mutation_strength);
                Self::apply_mixer_commands(conductor, commands);
                true
            }
            PerformanceCommand::SetTrackMute { track_idx, muted, .. } => {
                println!("Conductor: Track {} Mute set to {}", track_idx, muted);
                true
            }
            PerformanceCommand::SetTrackSolo { track_idx, soloed, .. } => {
                println!("Conductor: Track {} Solo set to {}", track_idx, soloed);
                true
            }
            PerformanceCommand::ClearTrackPattern { track_idx, .. } => {
                println!("Conductor: Clearing Pattern for Track {}", track_idx);
                true
            }
            PerformanceCommand::Preview { sample_id } => {
                if let Some(ref mut prod) = conductor.topology_manager.topo_producer {
                    let preview_node_idx = conductor.mixer_manager.node_names.get("preview_node").cloned().unwrap_or(111);
                    if let Some(sample) = conductor.transfusion_manager.sample_registry.get(sample_id) {
                         let _ = prod.push(nullherz_traits::TopologyMutation::AddSource {
                            node_idx: preview_node_idx,
                            buffer: sample.buffer,
                            sample_id,
                            metadata: Some(sample.metadata.clone()),
                        });
                        let _ = Self::handle_performance_command(conductor, PerformanceCommand::PlayNode { node_idx: preview_node_idx });
                    }
                }
                true
            }
            PerformanceCommand::SyncDecks { source_deck, target_deck } => {
                let src_sampler_id = conductor.mixer_manager.deck_mappings.get(&source_deck).map(|d| d.sampler_id);
                let dst_sampler_id = conductor.mixer_manager.deck_mappings.get(&target_deck).map(|d| d.sampler_id);

                if let (Some(src_idx), Some(dst_idx)) = (src_sampler_id, dst_sampler_id) {
                    let mut src_bpm = 0.0;
                    if let Ok(engine_lock) = conductor.engine_coordinator.backend_manager.engine_handle.lock() {
                        if let Some(ref engine) = *engine_lock {
                             if let Some(src_proc) = engine.list_children().iter().find(|c| c.metadata().map(|m| m.processor_id as u32) == Some(src_idx)) {
                                 if let Some(rid) = src_proc.resource_id() {
                                     if let Ok(lib) = conductor.library.lock() {
                                         if let Ok(Some(track)) = lib.get_track(rid) {
                                             src_bpm = (*track.metadata).bpm;
                                         }
                                     }
                                 }
                             }
                        }
                    }

                    if src_bpm > 0.0 {
                        Self::apply_mixer_commands(conductor, vec![
                            Command::Core(CoreCommand::SetBpm(src_bpm)),
                            Command::Performance(PerformanceCommand::JumpByBeats {
                                node_idx: dst_idx,
                                beats: 0.0,
                            })
                        ]);
                        println!("Conductor: Phase-Locked Sync: Deck {} -> {} @ {:.2} BPM", source_deck, target_deck, src_bpm);
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn handle_resource_command(conductor: &mut Conductor, cmd: ResourceCommand) -> bool {
        match cmd {
            ResourceCommand::ScanFolder { path } => {
                let folder_path = String::from_utf8_lossy(&path).trim_matches(char::from(0)).to_string();
                if let Some(ref monitor) = conductor.folder_monitor {
                    monitor.scan_folder(&folder_path);
                }
                true
            }
            ResourceCommand::Normalize { sample_id } => {
                if let Some(sample) = conductor.transfusion_manager.sample_registry.get(sample_id) {
                    let mut max_peak = 0.0f32;
                    for &s in sample.buffer.iter() { max_peak = max_peak.max(s.abs()); }
                    if max_peak > 0.0 {
                        let gain = 0.95 / max_peak;
                        let mut new_buf = (*sample.buffer).clone();
                        for s in new_buf.iter_mut() { *s *= gain; }
                        conductor.transfusion_manager.sample_registry.register_with_metadata(sample_id, Arc::new(new_buf), sample.metadata);
                    }
                }
                true
            }
            ResourceCommand::Crop { sample_id, start_samples, end_samples } => {
                 if let Some(sample) = conductor.transfusion_manager.sample_registry.get(sample_id) {
                     let start = start_samples as usize;
                     let end = (end_samples as usize).min(sample.buffer.len());
                     if start < end {
                         let cropped = sample.buffer[start..end].to_vec();
                         conductor.transfusion_manager.sample_registry.register_with_metadata(sample_id, Arc::new(cropped), sample.metadata);
                     }
                 }
                 true
            }
            ResourceCommand::ReAnalyze { sample_id } => {
                 if let Some(_) = conductor.transfusion_manager.sample_registry.get(sample_id) {
                     if let Some(ref mut worker) = conductor.analysis_worker {
                         worker.request_analysis(sample_id);
                     }
                 }
                 true
            }
            ResourceCommand::CommitBreeding { parent_a_id, parent_b_id, bias } => {
                let lib = conductor.library.lock().unwrap();
                conductor.transfusion_manager.commit_breeding(parent_a_id, parent_b_id, bias, &lib);
                true
            }
            ResourceCommand::CommitChaoticBreeding { parent_a_id, parent_b_id, bias, chaotic_strength } => {
                let lib = conductor.library.lock().unwrap();
                conductor.transfusion_manager.commit_chaotic_breeding(parent_a_id, parent_b_id, bias, chaotic_strength, &lib);
                true
            }
            ResourceCommand::RegisterCapture { .. } => {
                if let Ok(engine_lock) = conductor.engine_coordinator.backend_manager.engine_handle.lock() {
                   if let Some(ref engine) = *engine_lock {
                       conductor.transfusion_manager.poll_snapshots(engine.as_ref());
                   }
                }
                true
            }
            ResourceCommand::ApplyFeatureMutation { target_id, feature_name, strength } => {
                let name = String::from_utf8_lossy(&feature_name).trim_matches(char::from(0)).to_string();
                if let Some(sample) = conductor.transfusion_manager.sample_registry.get(target_id) {
                    let mut metadata_mut = (*sample.metadata).clone();
                    nullherz_dna::FeatureMutator::mutate(&mut metadata_mut.dna, &name, strength);
                    conductor.transfusion_manager.sample_registry.register_with_metadata(target_id, sample.buffer, Arc::new(metadata_mut));
                    println!("Applied Feature Mutation '{}' (strength={:.2}) to sample {}", name, strength, target_id);
                }
                true
            }
            _ => false,
        }
    }

    fn handle_dna_command(conductor: &mut Conductor, cmd: DnaCommand) -> bool {
        if let Some(ref mut prod) = conductor.engine_coordinator.command_producer {
            let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                timestamp_samples: 0,
                command: Command::Dna(cmd),
            });
            return true;
        }
        false
    }
}
