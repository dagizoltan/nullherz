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
        telemetry.active_master_deck = conductor.mixer_manager.active_master_deck;

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

        // Underruns, from the only party that observes them.
        //
        // The engine hands us `xrun_count` read from an atomic that nothing
        // increments, so it is always 0. The backend keeps the real count
        // (ALSA on `snd_pcm_writei` failure, Threaded on a missed deadline).
        // `None` means the running backend does not measure them at all, which
        // must stay distinguishable from a clean run — reporting both as `0` is
        // how a "zero xruns" gate came to be unfailable.
        match conductor.engine_coordinator.backend_manager.xruns() {
            Some(n) => {
                telemetry.xrun_count = n.min(u32::MAX as u64) as u32;
                telemetry.xruns_reported = true;
            }
            None => {
                telemetry.xrun_count = 0;
                telemetry.xruns_reported = false;
            }
        }

        // Output latency depends on the ring buffer, which only the backend
        // knows. 0 reads as "not reported" in the view.
        telemetry.device_buffer_frames =
            conductor.engine_coordinator.backend_manager.buffer_frames().unwrap_or(0);

        // Resident decoded audio, FROM THE CACHE (refreshed in tick()).
        // Walking the registry here would be the third time this function has
        // been given a per-frame job that scales with the session.
        let (samples, bytes) = conductor.residency();
        telemetry.registry_samples = samples;
        telemetry.registry_bytes = bytes;

        // Clock jitter, when a clock provider is actually installed. Without
        // one this stayed at the engine's hardcoded 0, which the calibration
        // view rendered as perfect synchronisation.
        use nullherz_traits::ClockProvider as _;
        match conductor.ptp_clock.as_ref() {
            Some(clock) => {
                telemetry.clock_jitter_ns = clock.get_estimated_jitter_ns();
                telemetry.clock_jitter_available = true;
            }
            None => {
                telemetry.clock_jitter_ns = 0;
                telemetry.clock_jitter_available = false;
            }
        }

        // Sync node name registry to telemetry.
        //
        // Sorted, so that if the map ever does overflow its slots the names
        // that survive are the same on every run. Truncating a `HashMap` in
        // iteration order drops an arbitrary subset, which would make a control
        // (a hot cue, the sync toggle, scroll-to-scrub) work or not depending on
        // hash ordering — the worst kind of bug to be handed.
        let slots = nullherz_traits::telemetry::NODE_MAP_SLOTS;
        let mut names: Vec<(&String, u32)> = conductor
            .mixer_manager
            .node_names
            .iter()
            .map(|(n, &i)| (n, i))
            .collect();
        names.sort_by(|a, b| a.0.cmp(b.0));
        if names.len() > slots {
            static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "WARN: {} named nodes exceed the {} telemetry slots; the UI cannot resolve                      the rest and any control bound to them will silently do nothing.                      Raise NODE_MAP_SLOTS.",
                    names.len(), slots
                );
            }
        }
        for (i, (name, idx)) in names.into_iter().take(slots).enumerate() {
            let bytes = name.as_bytes();
            let len = bytes.len().min(32);
            telemetry.node_map_keys[i] = [0u8; 32];
            telemetry.node_map_keys[i][..len].copy_from_slice(&bytes[..len]);
            telemetry.node_map_values[i] = idx;
        }

        // Sync audio devices to telemetry FROM THE CACHE.
        //
        // This used to call `backend.enumerate_devices()` directly. That is a
        // driver query (dlopen + snd_device_name_hint + an ALSA config parse),
        // measured at 74.6 ms — against a 5.3 ms telemetry budget, ~187 times a
        // second. The conductor thread could not drain its telemetry queue, so
        // LoadTrackToDeck and PlayDeck never executed and the decks appeared
        // dead. Identical in shape to the `get_track()` call this same function
        // used to make for `waveform_peaks` (see the note at the end).
        for (i, dev) in conductor.audio_device_names().iter().enumerate().take(16) {
            let bytes = dev.as_bytes();
            let len = bytes.len().min(64);
            telemetry.audio_devices[i].name = [0u8; 64];
            telemetry.audio_devices[i].name[..len].copy_from_slice(&bytes[..len]);
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
