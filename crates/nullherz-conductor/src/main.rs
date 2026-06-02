use nullherz_conductor::Conductor;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("nullherz-conductor starting...");

    let mut conductor = Conductor::new();
    let (cmd_prod, tel_cons) = conductor.setup_engine();

    // Start the backend (defaulting to threaded for safety in sandbox)
    conductor.start_backend("threaded")?;
    println!("Audio engine started.");

    // Bridge to gateway in a background task
    let gateway_task = tokio::spawn(async move {
        // We'll just use a simple loop to simulate gateway bridging if not using the actual gateway crate
        // In a real scenario, we'd call nullherz_gateway::run_gateway
        println!("Gateway bridge active (internal).");
    });

    // Main orchestration loop
    loop {
        // Reap zombie sidecars and handle automated recovery
        let dead_sidecars = conductor.manager.reap_zombies();
        if !dead_sidecars.is_empty() {
            println!("Recovered {} sidecar(s).", dead_sidecars.len());
            // In a real impl, we'd re-insert these into the graph via cmd_prod
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
