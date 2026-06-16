use nullherz_conductor::Conductor;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("nullherz-conductor starting...");

    let mut conductor = Conductor::new();
    let (cmd_buffer, tel_cons) = conductor.setup_engine();

    // Start the backend (defaulting to threaded for safety in sandbox)
    conductor.start_backend(nullherz_backends::AudioBackendType::Threaded)?;
    println!("Audio engine started.");

    let cmd_prod_gateway = ipc_layer::NonRtProducer::from_boxed(cmd_buffer);

    let _gateway_task = tokio::spawn(async move {
        let _ = nullherz_gateway::run_gateway("127.0.0.1:9001", cmd_prod_gateway, tel_cons).await;
        println!("Gateway bridge closed.");
    });

    // Main orchestration loop
    loop {
        conductor.tick();
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
