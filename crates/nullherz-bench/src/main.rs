use control_plane::TimestampedCommand;
use nullherz_mixer::MixerManager;
use nullherz_conductor::Conductor;

fn main() {
    println!("Benchmarking Nullherz Engine with Complex Graph...");

    let mut conductor = Conductor::new();
    let (cmd_buffer, mut tel_cons) = conductor.setup_engine();

    let mut mixer = MixerManager::new();
    let commands = mixer.create_4channel_mixer();
    conductor.apply_mixer_commands(commands);

    // Commit topology
    let _ = cmd_buffer.push(TimestampedCommand {
        timestamp_samples: 0,
        command: control_plane::Command::CommitTopology,
    });

    conductor.start_backend(nullherz_backends::AudioBackendType::Threaded).unwrap();

    let mut samples_collected = 0;
    let mut total_ns = 0;
    let mut peak_ns = 0;

    let start_time = std::time::Instant::now();
    while start_time.elapsed() < std::time::Duration::from_secs(2) {
        while let Some(telemetry) = tel_cons.pop() {
            total_ns += telemetry.process_time_ns;
            peak_ns = peak_ns.max(telemetry.peak_process_time_ns);
            samples_collected += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    conductor.stop_backend();

    if samples_collected > 0 {
        let avg_ns = total_ns / samples_collected;
        println!("Benchmark results for 4-channel mixer:");
        println!("  Average processing time: {} ns", avg_ns);
        println!("  Peak processing time: {} ns", peak_ns);
        println!("  Samples collected: {}", samples_collected);
    } else {
        println!("No telemetry data collected.");
    }

    println!("Bench finished.");
}
