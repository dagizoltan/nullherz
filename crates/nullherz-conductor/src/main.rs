use nullherz_conductor::Conductor;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("nullherz-conductor starting...");

    let mut conductor = Conductor::new();
    let context = conductor.setup_engine();

    // --- MIDI SIDECAR BRIDGE SETUP ---
    // This allows the nullherz-midi sidecar to talk to the conductor's mapping engine.
    let (_midi_sidecar_prod, midi_sidecar_cons) = ipc_layer::RingBuffer::new(256).split();
    conductor.set_midi_consumer(midi_sidecar_cons);

    // Start the backend (defaulting to threaded for safety in sandbox)
    conductor.start_backend(nullherz_backends::AudioBackendType::Threaded)?;
    println!("Audio engine started.");

    let cmd_prod_gateway = ipc_layer::NonRtProducer::from_boxed(context.command_producer);

    let _gateway_task = tokio::spawn(async move {
        let _ = nullherz_gateway::run_gateway("127.0.0.1:9001", cmd_prod_gateway, context.telemetry_consumer).await;
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

    // Main orchestration loop
    let mut ticker = tokio::time::interval(Duration::from_millis(100));
    loop {
        ticker.tick().await;
        conductor.tick();
    }
}
