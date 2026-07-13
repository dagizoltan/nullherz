use sidecar_sdk::SidecarHost;
use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider};
use std::env;

struct MySidecarProcessor {
    id: u64,
}

impl SignalProcessor for MySidecarProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        // Your DSP logic here
        for output in outputs {
            output.fill(0.0);
        }
    }
}

impl MidiResponder for MySidecarProcessor {}
impl SnapshotProvider for MySidecarProcessor {}

impl AudioProcessor for MySidecarProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 {
        eprintln!("Usage: {} <cmd_shm> <sig_shm> <in_shm_list> <out_shm_list> [event_fd]", args[0]);
        return;
    }

    let cmd_name = &args[1];
    let sig_name = &args[2];
    let in_names: Vec<String> = args[3].split(',').map(|s| s.to_string()).collect();
    let out_names: Vec<String> = args[4].split(',').map(|s| s.to_string()).collect();
    let efd = args.get(5).and_then(|s| s.parse::<i32>().ok()).unwrap_or(-1);

    unsafe {
        let sc_names = Vec::new();
        let mut host = SidecarHost::new(cmd_name, sig_name, &in_names, &sc_names, &out_names, efd);
        let processor = MySidecarProcessor { id: 0 };
        println!("Sidecar template starting...");
        host.run(processor);
    }
}
