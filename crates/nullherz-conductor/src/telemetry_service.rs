#![allow(clippy::collapsible_if)]
use crate::orchestrator::Conductor;
use nullherz_traits::telemetry::Telemetry;

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

        // NOTE: `telemetry.waveform_peaks` is deliberately NOT produced here.
        //
        // This function runs once per TELEMETRY FRAME — one per audio block, so
        // ~172 Hz at 44.1 kHz / 256. It used to fill that field by locking the
        // engine, scanning `list_children()` for each of the four decks, and then
        // calling `library.get_track()` per deck to keep 64 floats.
        //
        // `get_track()` deserializes the WHOLE row, and library rows are stored
        // as JSON — for a 6-minute track that is ~1.6 million floats of peaks,
        // MIP levels and band waveform parsed from text. Measured: 61 ms per
        // call, versus 2.5 ms for a 17-second demo WAV. With one real track on a
        // deck this loop alone cost 64 ms per telemetry frame against a 5.8 ms
        // budget — 11x realtime, and 22x with two decks loaded. The conductor
        // thread could never drain the telemetry queue, and because the read held
        // the library mutex it also starved every command that needs the library:
        // LoadTrackToDeck and PlayDeck queued behind it indefinitely. The console
        // played short WAVs fine and appeared unable to play full-length tracks
        // at all.
        //
        // Nothing ever read `waveform_peaks`: the deck lanes render from the
        // cached library row's `mip_waveform` / `band_waveform` on the UI side.
        // The field stays in `Telemetry` (fixed-size, ABI-stable protocol
        // struct); only the producer is gone. Reviving it must not put a library
        // read back on this path — cache per deck, keyed by the loaded sample id,
        // and refresh only when that id changes.
    }
}
