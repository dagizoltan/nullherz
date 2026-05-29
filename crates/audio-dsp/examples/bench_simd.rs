use audio_dsp::{BiquadCoefficients, BiquadFilter, SimdBiquad, Filter};
use std::time::Instant;

fn main() {
    let coeffs = BiquadCoefficients { b0: 0.1, b1: 0.2, b2: 0.1, a1: -0.5, a2: 0.3 };
    let iterations = 100_000;
    let block_size = 128;

    // 1. Scalar Benchmark
    let mut scalar_filter = BiquadFilter::new(coeffs);
    let input = vec![1.0f32; block_size];
    let mut output = vec![0.0f32; block_size];

    let start = Instant::now();
    for _ in 0..iterations {
        for i in 0..block_size {
            output[i] = scalar_filter.process_sample(input[i]);
        }
    }
    println!("Scalar: {:?}", start.elapsed());

    // 2. SIMD Multi-channel Benchmark (8 channels)
    let mut simd_filter = SimdBiquad::new(coeffs);
    let in_ptrs: [*const f32; 8] = [input.as_ptr(); 8];
    let mut out_ptrs: [*mut f32; 8] = [output.as_mut_ptr(); 8];

    let start = Instant::now();
    for _ in 0..iterations {
        unsafe { simd_filter.process_8_channels(in_ptrs, out_ptrs, block_size); }
    }
    println!("SIMD (8-channel): {:?}", start.elapsed());
}
