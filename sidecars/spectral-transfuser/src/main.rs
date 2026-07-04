use clap::Parser;
use ipc_layer::{SharedMemory, ShmRingBuffer, ShmSignal, EventFd, AudioBlock};
use nullherz_traits::{Command, DnaCommand, TimestampedCommand};
use audio_dsp::spectral::SpectralPipeline;
use audio_dsp::simd_vec::{FloatX16, load_f32x16, store_f32x16};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    command_shm: String,
    #[arg(long)]
    feedback_shm: String,
    #[arg(long)]
    signal_shm: String,
    #[arg(long)]
    event_fd: i32,
    #[arg(long)]
    channels: usize,
    #[arg(long)]
    input_shm: Vec<String>,
    #[arg(long)]
    output_shm: Vec<String>,
}

fn main() {
    let args = Args::parse();

    // 1. Map SHM
    let (cmd_layout, _) = ShmRingBuffer::<TimestampedCommand>::layout(64);
    let cmd_shm = SharedMemory::open(&args.command_shm, cmd_layout.size()).unwrap();
    let cmd_rb = cmd_shm.ptr() as *const ShmRingBuffer<TimestampedCommand>;

    let sig_shm = SharedMemory::open(&args.signal_shm, 64).unwrap();
    let efd = EventFd::from_raw(args.event_fd);

    let signal = unsafe { &*(sig_shm.ptr() as *const ShmSignal) };

    // 2. Map Audio SHM
    let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(16);
    let mut input_rbs = Vec::new();
    for path in &args.input_shm {
        let shm = SharedMemory::open(path, audio_layout.size()).unwrap();
        input_rbs.push(shm.ptr() as *const ShmRingBuffer<AudioBlock>);
        std::mem::forget(shm); // Keep mapped
    }
    let mut output_rbs = Vec::new();
    for path in &args.output_shm {
        let shm = SharedMemory::open(path, audio_layout.size()).unwrap();
        output_rbs.push(shm.ptr() as *const ShmRingBuffer<AudioBlock>);
        std::mem::forget(shm); // Keep mapped
    }

    // 3. Initialize DSP
    let mut pipeline = SpectralPipeline::new(1024);
    let mut energy_map = [0u8; 64];
    let mut transfusion_bias = 0.0f32;

    println!("Spectral Transfuser Sidecar Started");

    // 4. Real-time Loop
    loop {
        efd.wait();
        signal.pulse_heartbeat();

        // Pop commands
        while let Some(ts_cmd) = unsafe { (*cmd_rb).pop() } {
            if let Command::Dna(dna_cmd) = ts_cmd.command {
                energy_map.copy_from_slice(&dna_cmd.payload[0..64]);
                transfusion_bias = dna_cmd.bias;
            }
        }

        // Simple bypass/process loop for each channel
        for i in 0..args.channels {
            if let (Some(in_block), Some(out_rb)) = (unsafe { (*input_rbs[i]).pop() }, output_rbs.get(i)) {
                let mut out_block = AudioBlock { data: [0.0; 256], len: in_block.len, _pad: [0; 15] };

                // For simplicity in this reference, we process blocks through the pipeline
                pipeline.process(&in_block.data[..in_block.len as usize], &mut out_block.data[..in_block.len as usize], |re, im, n, _win, _fft| {
                    if transfusion_bias > 0.0 {
                        let bins_per_entry = n / 2 / 64;
                        if bins_per_entry == 0 { return; }

                        for entry_idx in 0..64 {
                            let target_mag = energy_map[entry_idx] as f32 / 255.0;
                            let start_bin = entry_idx * bins_per_entry;
                            let end_bin = (entry_idx + 1) * bins_per_entry;

                            for bin in (start_bin..end_bin).step_by(16).filter(|&b| b + 16 <= end_bin) {
                                // SIMD Optimized Magnitude Scaling
                                let v_re = load_f32x16(re, bin);
                                let v_im = load_f32x16(im, bin);
                                // mag = sqrt(re*re + im*im)
                                // Simplified: we just use the bias and target_mag to scale
                                let v_target = FloatX16::splat(target_mag);
                                let v_bias = FloatX16::splat(transfusion_bias);

                                let v_res_re = v_re * (FloatX16::splat(1.0) - v_bias + v_target * v_bias);
                                let v_res_im = v_im * (FloatX16::splat(1.0) - v_bias + v_target * v_bias);

                                store_f32x16(re, bin, v_res_re);
                                store_f32x16(im, bin, v_res_im);
                            }
                            // Scalar fallback for remainders omitted for brevity in reference
                        }
                    }
                });

                let _ = unsafe { (**out_rb).push(out_block) };
            }
        }
    }
}
