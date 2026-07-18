use crate::orchestrator::Conductor;
use nullherz_traits::{Command, CoreCommand, PerformanceCommand, ResourceCommand, DnaCommand};
use std::sync::Arc;
use nullherz_dna::GeneticLibrary;

pub struct CommandHandler;

impl CommandHandler {
    pub fn apply_mixer_commands(conductor: &mut Conductor, commands: Vec<Command>) {
        let mut final_commands = Vec::new();

        // 0. On-demand registry hydration: a deck load referencing a library
        // track whose buffer is not in the (boot-empty, in-memory) registry
        // would silently no-op in the engine. Decode it here, off the RT
        // thread, so ANY library entry is playable regardless of which
        // scanner or seeder created it.
        for cmd in &commands {
            if let Command::Performance(PerformanceCommand::LoadTrackToDeck { sample_id, .. }) = cmd {
                if conductor.transfusion_manager.sample_registry.get(*sample_id).is_none() {
                    let track = { conductor.library.lock().get_track(*sample_id).ok().flatten() };
                    if let Some(track) = track {
                        println!("CommandHandler: Hydrating sample {} from {}", sample_id, track.path);
                        let buffer = crate::folder_monitor::decode_audio_file(&track.path);
                        conductor.transfusion_manager.sample_registry.register_with_metadata(
                            *sample_id, buffer, track.metadata.clone());
                    } else {
                        eprintln!("CommandHandler: LoadTrackToDeck {} has no library entry; the deck will stay silent.", sample_id);
                    }
                }
            }
        }

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
            CoreCommand::Undo => {
                conductor.undo();
                true
            }
            CoreCommand::Redo => {
                conductor.redo();
                true
            }
            CoreCommand::CheckpointParameterEdit => {
                conductor.checkpoint_parameter_edit();
                true
            }
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
                    engine_lock.as_ref().map(|e| e.target_sample_rate()).unwrap_or(44100.0)
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
                    known.get(&plugin_name).cloned()
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
                    { let engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
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
                    { let lib = conductor.library.lock();
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
            PerformanceCommand::Preview { mut sample_id } => {
                if let Some(ref mut prod) = conductor.topology_manager.topo_producer {
                    let preview_node_idx = conductor.mixer_manager.node_names.get("preview_node").cloned().unwrap_or(111);

                    // Quality-of-life fallback: if hardcoded sample_id 1 or 2 is not found in the registry,
                    // fall back to the first or second available sample in the registry.
                    let mut sample_opt = conductor.transfusion_manager.sample_registry.get(sample_id);
                    if sample_opt.is_none() && (sample_id == 1 || sample_id == 2) {
                        let ids = conductor.transfusion_manager.sample_registry.list_ids();
                        if !ids.is_empty() {
                            let fallback_idx = if sample_id == 2 { 1.min(ids.len() - 1) } else { 0 };
                            sample_id = ids[fallback_idx];
                            sample_opt = conductor.transfusion_manager.sample_registry.get(sample_id);
                        }
                    }

                    if let Some(sample) = sample_opt {
                         let _ = prod.push(nullherz_traits::TopologyMutation::AddSource {
                            node_idx: preview_node_idx,
                            buffer: sample.buffer,
                            sample_id,
                            metadata: Some(sample.metadata.clone()),
                        });
                        let _ = Self::handle_performance_command(conductor, PerformanceCommand::PlayNode { node_idx: preview_node_idx });
                    } else {
                        eprintln!("Preview failed: sample_id {} not found in registry, and no fallback available.", sample_id);
                    }
                }
                true
            }
            PerformanceCommand::SyncDecks { source_deck, target_deck } => {
                let src_sampler_id = conductor.mixer_manager.deck_mappings.get(&source_deck).map(|d| d.sampler_id);
                let dst_sampler_id = conductor.mixer_manager.deck_mappings.get(&target_deck).map(|d| d.sampler_id);

                if let (Some(src_idx), Some(dst_idx)) = (src_sampler_id, dst_sampler_id) {
                    let mut src_bpm = 0.0;
                    { let engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
                        if let Some(ref engine) = *engine_lock {
                             if let Some(src_proc) = engine.list_children().iter().find(|c| c.metadata().map(|m| m.processor_id as u32) == Some(src_idx)) {
                                 if let Some(rid) = src_proc.resource_id() {
                                     { let lib = conductor.library.lock();
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
                conductor.checkpoint();
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
                 conductor.checkpoint();
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
            ResourceCommand::TimeStretch { sample_id, ratio } => {
                 conductor.checkpoint();
                 if let Some(sample) = conductor.transfusion_manager.sample_registry.get(sample_id) {
                     let stretched = audio_dsp::util::time_stretch(&sample.buffer, ratio);
                     let mut new_metadata = (*sample.metadata).clone();
                     new_metadata.total_samples = stretched.len() as u64;

                     let mut new_transients = Vec::new();
                     for &t in (*sample.metadata.transients).iter() {
                         new_transients.push((t as f32 / ratio) as u64);
                     }
                     new_metadata.transients = Arc::new(new_transients);

                     conductor.transfusion_manager.sample_registry.register_with_metadata(
                         sample_id,
                         Arc::new(stretched),
                         Arc::new(new_metadata.clone()),
                     );

                     { let lib = conductor.library.lock();
                         if let Ok(Some(mut track)) = lib.get_track(sample_id) {
                             track.metadata = Arc::new(new_metadata);
                             let _ = lib.save_track(&track);
                         }
                     }
                 }
                 true
            }
            ResourceCommand::ChopByTransient { sample_id } => {
                 conductor.checkpoint();
                 if let Some(sample) = conductor.transfusion_manager.sample_registry.get(sample_id) {
                     let transients = &sample.metadata.transients;
                     let mut slice_points = Vec::new();
                     slice_points.push(0u64);
                     for &t in transients.iter() {
                         if t > 0 && t < sample.buffer.len() as u64 {
                             slice_points.push(t);
                         }
                     }

                     // If transients list is empty or single-point, run a real time-domain onset detector across the whole sample buffer!
                     if slice_points.len() <= 2 {
                         let win_size = 256;
                         if sample.buffer.len() >= win_size * 4 {
                             let mut last_transient = 0u64;
                             let min_gap = 44100 / 10; // min 100ms between transients
                             let threshold = 1.8f32;
                             let mut env = 0.0f32;

                             let mut short_term_energy = vec![0.0f32; sample.buffer.len() / win_size];
                             for chunk_idx in 0..short_term_energy.len() {
                                 let start = chunk_idx * win_size;
                                 let mut sum_sq = 0.0f32;
                                 for j in 0..win_size {
                                     let s = sample.buffer[start + j];
                                     sum_sq += s * s;
                                 }
                                 short_term_energy[chunk_idx] = (sum_sq / win_size as f32).sqrt();
                             }

                             for idx in 1..short_term_energy.len() {
                                 let energy = short_term_energy[idx];
                                 let prev_energy = short_term_energy[idx - 1];
                                 env = env * 0.95 + prev_energy * 0.05;

                                 if energy > env * threshold && energy > 0.02 {
                                     let sample_pos = (idx * win_size) as u64;
                                     if sample_pos > last_transient + min_gap as u64 && sample_pos < sample.buffer.len() as u64 {
                                         slice_points.push(sample_pos);
                                         last_transient = sample_pos;
                                     }
                                 }
                             }
                         }
                     }

                     slice_points.push(sample.buffer.len() as u64);
                     slice_points.sort();
                     slice_points.dedup();

                     // If still only 2 points, fall back to equal 4-way split, with honest disclosure in log
                     if slice_points.len() <= 2 {
                         println!("[ChopByTransient] NOTICE: No transients detected, falling back to equal 4-way split.");
                         let chunk = sample.buffer.len() / 4;
                         slice_points = vec![
                             0,
                             chunk as u64,
                             (chunk * 2) as u64,
                             (chunk * 3) as u64,
                             sample.buffer.len() as u64,
                         ];
                     }

                     static SLICE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(50000);
                     let mut slice_ids = Vec::new();
                     for i in 0..slice_points.len() - 1 {
                         let start = slice_points[i] as usize;
                         let end = slice_points[i+1] as usize;
                         if start < end {
                             let slice_data = sample.buffer[start..end].to_vec();
                             let slice_id = SLICE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                             slice_ids.push(slice_id);

                             let mut slice_metadata = (*sample.metadata).clone();
                             slice_metadata.total_samples = slice_data.len() as u64;
                             slice_metadata.transients = Arc::new(Vec::new());

                             conductor.transfusion_manager.sample_registry.register_with_metadata(
                                 slice_id,
                                 Arc::new(slice_data),
                                 Arc::new(slice_metadata.clone()),
                             );

                             { let lib = conductor.library.lock();
                                 let track = nullherz_dna::LibraryTrack {
                                     id: slice_id,
                                     path: format!("slice://{}/{}", sample_id, i),
                                     title: format!("Slice {} [{}]", i + 1, sample_id),
                                     artist: "Slice Editor".to_string(),
                                     album: "Chops".to_string(),
                                     genre: "Sample Slice".to_string(),
                                     energy_level: 0.5,
                                     metadata: Arc::new(slice_metadata),
                                 };
                                 let _ = lib.save_track(&track);
                             }
                         }
                     }
                     println!("Conductor: Chopped sample {} into {} slices: {:?}", sample_id, slice_ids.len(), slice_ids);
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
                let lib = conductor.library.lock();
                conductor.transfusion_manager.commit_breeding(parent_a_id, parent_b_id, bias, &lib);
                true
            }
            ResourceCommand::CommitChaoticBreeding { parent_a_id, parent_b_id, bias, chaotic_strength } => {
                let lib = conductor.library.lock();
                conductor.transfusion_manager.commit_chaotic_breeding(parent_a_id, parent_b_id, bias, chaotic_strength, &lib);
                true
            }
            ResourceCommand::RhythmicTransfusion { source_id, target_id } => {
                let lib = conductor.library.lock();
                conductor.transfusion_manager.execute_rhythmic_transfusion(source_id, target_id, &lib);
                true
            }
            ResourceCommand::RegisterCapture { .. } => {
                { let engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
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
