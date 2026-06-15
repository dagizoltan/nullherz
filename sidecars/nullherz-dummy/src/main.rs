use sidecar_sdk::SidecarHost;
use audio_core::AudioProcessor;

struct DummyProcessor;

impl AudioProcessor for DummyProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut audio_core::processors::ProcessContext) {
        for (i, input) in inputs.iter().enumerate() {
            if i < outputs.len() {
                outputs[i].copy_from_slice(input);
            }
        }
    }
    fn apply_command(&mut self, _cmd: &control_plane::Command) {}
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!("Usage: sidecar --command-shm <name> --channels <n> --signal-shm <name> --event-fd <fd> [--input-shm <name> ...]");
        return;
    }

    let mut cmd_shm = "";
    let mut sig_shm = "";
    let mut efd_val = 0;
    let mut _channels = 0;

    for i in 0..args.len() {
        match args[i].as_str() {
            "--command-shm" if i + 1 < args.len() => cmd_shm = &args[i+1],
            "--signal-shm" if i + 1 < args.len() => sig_shm = &args[i+1],
            "--event-fd" if i + 1 < args.len() => efd_val = args[i+1].parse().unwrap_or(0),
            "--channels" if i + 1 < args.len() => _channels = args[i+1].parse().unwrap_or(0),
            _ => {}
        }
    }

    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for i in 0..args.len() {
        if args[i] == "--input-shm" && i + 1 < args.len() { inputs.push(args[i+1].clone()); }
        if args[i] == "--output-shm" && i + 1 < args.len() { outputs.push(args[i+1].clone()); }
    }

    unsafe {
        let mut sidecar = SidecarHost::new(cmd_shm, sig_shm, &inputs, &outputs, efd_val);
        sidecar.run(DummyProcessor);
    }
}
