#![no_main]
use libfuzzer_sys::fuzz_target;
use nullherz_processors::gain::GainProcessor;
use nullherz_traits::{SignalProcessor, ProcessContext};

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 { return; }
    let gain_val = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if !gain_val.is_finite() { return; }

    let mut processor = GainProcessor::new(1, gain_val);

    let mut input_l = [0.0f32; 256];
    let mut input_r = [0.0f32; 256];
    let mut output_l = [0.0f32; 256];
    let mut output_r = [0.0f32; 256];

    // Fill input with some data from fuzzer
    for i in 0..data.len().min(256) {
        input_l[i] = data[i] as f32 / 255.0;
        input_r[i] = data[i] as f32 / 255.0;
    }

    let inputs: &[&[f32]] = &[&input_l, &input_r];
    let mut outputs: &mut [&mut [f32]] = &mut [&mut output_l, &mut output_r];
    let mut context = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    processor.process(inputs, &mut outputs, &mut context);

    // Assert no NaNs in output
    for sample in output_l.iter() {
        assert!(sample.is_finite());
    }
});
