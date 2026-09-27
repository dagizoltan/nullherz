use nullherz_processors::MutatorProcessor;
use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input_path = "tracks/examples/track_a.wav";
    let output_path = "tracks/mutator_demo_output.wav";

    println!("=== MUTATOR / TRINYÓ Sound Deformation Demo ===");

    let mut reader = match hound::WavReader::open(input_path) {
        Ok(r) => r,
        Err(_) => {
            eprintln!("Input file {} not found, generating synthetic test audio...", input_path);
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = hound::WavWriter::create(input_path, spec)?;
            for t in 0..48000 * 5 {
                let time = t as f32 / 48000.0;
                let sample_l = (2.0 * std::f32::consts::PI * 110.0 * time).sin() * 0.5
                    + (2.0 * std::f32::consts::PI * 440.0 * time).sin() * 0.3;
                let sample_r = (2.0 * std::f32::consts::PI * 110.0 * time).sin() * 0.5
                    + (2.0 * std::f32::consts::PI * 880.0 * time).sin() * 0.3;
                writer.write_sample((sample_l * 32767.0) as i16)?;
                writer.write_sample((sample_r * 32767.0) as i16)?;
            }
            writer.finalize()?;
            hound::WavReader::open(input_path)?
        }
    };

    let spec = reader.spec();
    let sample_rate = spec.sample_rate as f32;
    let channels = spec.channels as usize;

    println!("Input Audio: {} (Channels: {}, Sample Rate: {} Hz)", input_path, channels, spec.sample_rate);

    let raw_samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / max_val).collect()
        }
    };

    let total_frames = raw_samples.len() / channels.max(1);
    let mut in_l = Vec::with_capacity(total_frames);
    let mut in_r = Vec::with_capacity(total_frames);

    if channels >= 2 {
        for frame in raw_samples.chunks(channels) {
            in_l.push(frame[0]);
            in_r.push(frame[1]);
        }
    } else {
        for &sample in &raw_samples {
            in_l.push(sample);
            in_r.push(sample);
        }
    }

    // Initialize MUTATOR Processor
    let mut mutator = MutatorProcessor::new(1, sample_rate);

    // Apply Active Sound Deformation Parameters
    mutator.set_parameter(0, 0.70, 0); // FLESH (Formant body warping)
    mutator.set_parameter(1, 0.60, 0); // BONE (Inharmonic resonators)
    mutator.set_parameter(2, 0.75, 0); // TEETH (Transient Padé saturation)
    mutator.set_parameter(3, 0.50, 0); // PARASITE (Gated filtered noise)
    mutator.set_parameter(4, 0.40, 0); // ASYMMETRY (Stereo micro-delay)
    mutator.set_parameter(5, 0.35, 0); // ABOMINATION (Meta-macro coupling)
    mutator.set_parameter(6, 1337.0, 0); // Seed S
    mutator.set_parameter(7, 0.80, 0); // Dry/Wet (80% Wet)
    mutator.set_parameter(10, 1.0, 0); // 200 Hz Sub-mono safety ON

    let mut out_l = vec![0.0f32; total_frames];
    let mut out_r = vec![0.0f32; total_frames];

    let block_size = 256;
    let mut ctx = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    let mut offset = 0;
    while offset < total_frames {
        let chunk_len = (total_frames - offset).min(block_size);

        let slice_in_l = &in_l[offset..offset + chunk_len];
        let slice_in_r = &in_r[offset..offset + chunk_len];

        let slice_out_l = &mut out_l[offset..offset + chunk_len];
        let slice_out_r = &mut out_r[offset..offset + chunk_len];

        mutator.process(
            &[slice_in_l, slice_in_r],
            &mut [slice_out_l, slice_out_r],
            &mut ctx,
        );

        offset += chunk_len;
    }

    // Write output WAV file
    let out_spec = hound::WavSpec {
        channels: 2,
        sample_rate: spec.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(output_path, out_spec)?;
    for i in 0..total_frames {
        let sample_l = (out_l[i].clamp(-1.0, 1.0) * 32767.0) as i16;
        let sample_r = (out_r[i].clamp(-1.0, 1.0) * 32767.0) as i16;
        writer.write_sample(sample_l)?;
        writer.write_sample(sample_r)?;
    }
    writer.finalize()?;

    println!("MUTATOR processing complete!");
    println!("Output written to: {}", output_path);
    println!("Duration: {:.2} seconds", total_frames as f32 / sample_rate);

    Ok(())
}
