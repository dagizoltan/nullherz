use crate::orchestrator::Conductor;
use nullherz_traits::telemetry::Telemetry;
use nullherz_dna::GeneticLibrary;

pub struct TelemetryService;

impl TelemetryService {
    pub fn update_timeline(conductor: &mut Conductor, telemetry: &mut Telemetry) {
        conductor.mixer_bridge.update_timeline(telemetry);
        conductor.clip_orchestrator.collect_telemetry(&mut telemetry.active_clips, &mut telemetry.starting_clips_mask);

        // Update Matchmaking Suggestions
        if let Some(sugg) = conductor.matchmaking_suggestions.try_lock() {
            for (i, (id, score)) in sugg.iter().enumerate().take(4) {
                telemetry.suggestions[i] = (*id, *score);
            }
        }
        telemetry.active_master_deck = conductor.active_master_deck;

        // Update Remote Node Telemetry
        if let Ok(manager) = conductor.sidecar_supervisor.remote_manager.try_lock() {
            telemetry.remote_node_count = manager.remote_nodes.len() as u32;
            for (i, node) in manager.remote_nodes.iter().enumerate().take(8) {
                telemetry.remote_cpu_usage[i] = node.cpu_usage;
                telemetry.remote_latency_ms[i] = node.latency_ms;
            }
        }

        // Update Calibration Telemetry from cached state
        telemetry.calibration_samples = conductor.calibration_samples;

        // Sync node name registry to telemetry
        for (i, (name, &idx)) in conductor.mixer_manager.node_names.iter().enumerate().take(32) {
            let bytes = name.as_bytes();
            let len = bytes.len().min(32);
            telemetry.node_map_keys[i][..len].copy_from_slice(&bytes[..len]);
            telemetry.node_map_values[i] = idx;
        }

        // Sync audio devices to telemetry
        if let Some(ref backend) = conductor.engine_coordinator.backend_manager.backend {
            for (i, dev) in backend.enumerate_devices().iter().enumerate().take(16) {
                let bytes = dev.as_bytes();
                let len = bytes.len().min(64);
                telemetry.audio_devices[i].name[..len].copy_from_slice(&bytes[..len]);
            }
        }

        // Sync Live Streaming Telemetry
        telemetry.is_streaming = conductor.is_streaming;
        telemetry.stream_bitrate = conductor.stream_bitrate;
        telemetry.stream_uptime_sec = conductor.stream_start_time.map(|t| t.elapsed().as_secs() as u32).unwrap_or(0);
        telemetry.stream_dropped_frames = conductor.stream_dropped_frames;
        telemetry.stream_viewers = conductor.stream_viewers;

        // Sync Live Mesh Peer Templates from Discovery Service
        { let known = conductor.sidecar_discovery.known_plugins.lock();
            telemetry.mesh_peer_count = known.len() as u32;
            for (i, (name, _)) in known.iter().enumerate().take(8) {
                let bytes = name.as_bytes();
                let len = bytes.len().min(64);
                telemetry.mesh_peer_names[i].name = [0u8; 64];
                telemetry.mesh_peer_names[i].name[..len].copy_from_slice(&bytes[..len]);
            }
        }

        // Waveform Telemetry Extraction
        let decks = ['A', 'B', 'C', 'D'];
        for (i, &deck_id) in decks.iter().enumerate().take(4) {
            if let Some(nodes) = conductor.mixer_manager.deck_mappings.get(&deck_id) {
                { let engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
                    if let Some(ref engine) = *engine_lock {
                        let resource_id = engine.list_children().iter()
                            .find(|c| c.metadata().map(|m| m.processor_id as u32) == Some(nodes.sampler_id))
                            .and_then(|c| c.resource_id());

                        if let Some(rid) = resource_id {
                            { let lib = conductor.library.lock();
                                if let Ok(Some(track)) = lib.get_track(rid) {
                                    if let Some(level) = track.metadata.mip_waveform.levels.get(4) {
                                        let offset = i * 64;
                                        for (j, &peak) in level.iter().enumerate().take(64) {
                                            let p: f32 = peak;
                                            telemetry.waveform_peaks[offset + j] = p;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
