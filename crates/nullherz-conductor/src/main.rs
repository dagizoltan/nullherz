use nullherz_conductor::Conductor;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("nullherz-conductor starting...");

    let mut conductor = Conductor::new();
    let (mut cmd_prod, tel_cons) = conductor.setup_engine();

    // Start the backend (defaulting to threaded for safety in sandbox)
    conductor.start_backend("threaded")?;
    println!("Audio engine started.");

    // We need a way to share the command producer between the gateway and the local orchestration loop.
    // For now, we'll clone it or use a proxy if ipc-layer supports it.
    // Actually, ipc_layer::Producer is just an Arc wrapper around the RingBuffer.
    let cmd_prod_gateway = cmd_prod.clone();

    let _gateway_task = tokio::spawn(async move {
        let _ = nullherz_gateway::run_gateway("127.0.0.1:9001", cmd_prod_gateway, tel_cons).await;
        println!("Gateway bridge closed.");
    });

    // Main orchestration loop
    loop {
        // Reap zombie sidecars and handle automated recovery
        let new_processors = conductor.manager.reap_zombies();
        for _processor in new_processors {
            println!("Recovered sidecar process. Re-inserting into audio graph...");
            // Swap the old (dead) processor in the engine with the new one
            // We assume node_idx 0 for this prototype recovery
            let _ = cmd_prod.push(control_plane::TimestampedCommand {
                timestamp_samples: 0,
                command: control_plane::Command::SwapProcessor {
                    node_idx: 0,
                    processor_type_id: 100 // Marker for custom/re-injected sidecar
                },
            });
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
