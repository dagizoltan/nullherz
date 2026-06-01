use nullherz_bridge::Bridge;
use ipc_layer::RingBuffer;
use audio_core::Telemetry;
use control_plane::TimestampedCommand;

fn main() {
    let (tel_prod, tel_cons) = RingBuffer::<Telemetry>::new(1024).split();
    let (cmd_prod, _cmd_cons) = RingBuffer::<TimestampedCommand>::new(1024).split();

    let mut bridge = Bridge::new(tel_cons, cmd_prod);

    // For demonstration in the bench/test environment, we'd normally pass the actual producers/consumers
    // from the AudioEngine setup.

    if let Err(e) = bridge.run("127.0.0.1:8080") {
        eprintln!("Bridge error: {}", e);
    }
}
