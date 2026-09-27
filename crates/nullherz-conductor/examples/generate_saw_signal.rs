fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_path = "tracks/examples/saw_test_signal.wav";
    println!("=== Generating Sawtooth Test Signal Audio File ===");

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(output_path, spec)?;

    let sample_rate = 44100.0f32;
    let duration_secs = 5.0f32;
    let total_samples = (sample_rate * duration_secs) as u32;

    // Fundamental frequency C2 (130.81 Hz)
    let base_freq = 130.81f32;

    for i in 0..total_samples {
        let t = i as f32 / sample_rate;

        // Bandlimited/Naive Sawtooth generator with slight L/R detune for stereo width
        let phase_l = (t * base_freq).fract();
        let phase_r = (t * base_freq * 1.002).fract();

        // Normalized naive saw: 2 * phase - 1
        let saw_l = 2.0 * phase_l - 1.0;
        let saw_r = 2.0 * phase_r - 1.0;

        // Amplitude envelope (decaying transient pulses every 1 second)
        let pulse_env = (1.0 - (t % 1.0) * 0.8).max(0.2);

        let out_l = (saw_l * pulse_env * 0.5 * 32767.0).clamp(-32768.0, 32767.0) as i16;
        let out_r = (saw_r * pulse_env * 0.5 * 32767.0).clamp(-32768.0, 32767.0) as i16;

        writer.write_sample(out_l)?;
        writer.write_sample(out_r)?;
    }

    writer.finalize()?;

    println!("Successfully generated: {}", output_path);
    println!("Format: 44.1 kHz, 16-bit stereo, 5.0 seconds, Sawtooth waveform (130.81 Hz)");

    Ok(())
}
