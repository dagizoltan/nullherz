use nullherz_conductor::Conductor;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("nullherz-conductor starting...");

    let mut conductor = Conductor::new();
    let _ = conductor.load_system_config();
    let context = conductor.setup_engine();

    // --- MIDI SIDECAR BRIDGE SETUP ---
    // This allows the nullherz-midi sidecar to talk to the conductor's mapping engine.
    let (_midi_sidecar_prod, midi_sidecar_cons) = ipc_layer::RingBuffer::new(256).split();
    conductor.set_midi_consumer(midi_sidecar_cons);

    // --- AUDIO BACKEND RESOLUTION AND FALLBACK ---
    // Load config to select the configured audio backend (falling back to ALSA by default)
    let mut backend_type = nullherz_backends::AudioBackendType::Alsa;
    let config_path = "system_config.json";
    if std::path::Path::new(config_path).exists()
        && let Ok(content) = std::fs::read_to_string(config_path)
            && let Ok(config) = serde_json::from_str::<nullherz_conductor::persistence::SystemConfig>(&content) {
                backend_type = match config.audio_backend.to_lowercase().as_str() {
                    "alsa" => nullherz_backends::AudioBackendType::Alsa,
                    "pipewire" => nullherz_backends::AudioBackendType::Pipewire,
                    "jack" => nullherz_backends::AudioBackendType::Jack,
                    "threaded" => nullherz_backends::AudioBackendType::Threaded,
                    "mock" => nullherz_backends::AudioBackendType::Mock,
                    _ => nullherz_backends::AudioBackendType::Alsa,
                };
                println!("System config loaded audio backend: {:?}", backend_type);
            }

    println!("Starting audio backend: {:?}", backend_type);
    if let Err(e) = conductor.start_backend(backend_type) {
        eprintln!("Failed to start audio backend {:?}: {}. Attempting fallback to Threaded backend...", backend_type, e);
        if let Err(fallback_err) = conductor.start_backend(nullherz_backends::AudioBackendType::Threaded) {
            eprintln!("CRITICAL: Failed to start fallback Threaded backend: {}", fallback_err);
            return Err(fallback_err.into());
        }
        println!("Fallback Threaded audio backend successfully started.");
    } else {
        println!("Audio engine started.");
    }

    let cmd_prod_gateway = ipc_layer::NonRtProducer::from_boxed(context.command_producer);
    let lib_db = conductor.library.clone();

    let _gateway_task = tokio::spawn(async move {
        let _ = nullherz_gateway::run_gateway("127.0.0.1:9001", cmd_prod_gateway, context.telemetry_consumer, Some(lib_db)).await;
        println!("Gateway bridge closed.");
    });

    // --- DJ PLATFORM BOOTSTRAP ---
    println!("Bootstrapping 4-Channel DJ Mixer...");
    let mut mixer = nullherz_mixer::MixerManager::new();
    let bootstrap_commands = mixer.create_4channel_mixer();
    conductor.apply_mixer_commands(bootstrap_commands);

    if let Some(worker) = conductor.analysis_worker.take() {
        worker.start();
    }

    if let Some(monitor) = conductor.folder_monitor.take() {
        monitor.start_auto_scan("tracks".to_string());
    }

    conductor.sidecar_discovery.start_watcher();

    // Main orchestration loop
    let mut ticker = tokio::time::interval(Duration::from_millis(100));
    loop {
        ticker.tick().await;
        conductor.tick();
    }
}
