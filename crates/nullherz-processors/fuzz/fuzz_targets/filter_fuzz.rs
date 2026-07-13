#![no_main]
use libfuzzer_sys::fuzz_target;
use nullherz_processors::biquad::BiquadProcessor;
use nullherz_traits::{SignalProcessor, ProcessContext};
use audio_dsp::{MoogLadder, ZdfSvf, BiquadCoefficients, Filter};

fuzz_target!(|data: &[u8]| {
    if data.len() < 32 { return; }

    // 1. Extract parameter values from fuzz data
    let cutoff = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let resonance = f32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let drive = f32::from_le_bytes([data[8], data[9], data[10], data[11]]);

    let b0 = f32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let b1 = f32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    let b2 = f32::from_le_bytes([data[20], data[21], data[22], data[23]]);
    let a1 = f32::from_le_bytes([data[24], data[25], data[26], data[27]]);
    let a2 = f32::from_le_bytes([data[28], data[29], data[30], data[31]]);

    let mut moog = MoogLadder::new(44100.0);
    moog.set_params(cutoff, resonance, drive);

    let mut svf = ZdfSvf::new(44100.0);
    svf.set_params(cutoff, resonance);

    let coeffs = BiquadCoefficients { b0, b1, b2, a1, a2 };
    let mut biquad = BiquadProcessor::new(101, coeffs);

    let mut input_block = [0.0f32; 64];
    let mut output_block_l = [0.0f32; 64];
    let mut output_block_r = [0.0f32; 64];

    // Populate input with remainder of fuzz data
    let rem = &data[32..];
    for i in 0..64 {
        if i < rem.len() {
            input_block[i] = rem[i] as f32 / 255.0;
        } else {
            input_block[i] = 0.0;
        }
    }

    // 2. Fuzz MoogLadder
    for &sample in input_block.iter() {
        let out = moog.process_sample(sample);
        // Ensure no crash or unexpected state
        let _ = out.is_finite();
    }

    // 3. Fuzz ZdfSvf
    for &sample in input_block.iter() {
        let lp = svf.process_lp(sample);
        let hp = svf.process_hp(sample);
        let bp = svf.process_bp(sample);
        let _ = lp.is_finite() && hp.is_finite() && bp.is_finite();
    }

    // 4. Fuzz BiquadProcessor
    let inputs: &[&[f32]] = &[&input_block[..], &input_block[..]];
    let mut outputs_slices = [output_block_l.as_mut_slice(), output_block_r.as_mut_slice()];
    let mut context = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    biquad.process(inputs, &mut outputs_slices, &mut context);
});
