use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor};
use nullherz_processors::TransientShaperProcessor;

#[test]
fn test_transient_shaper_identity_at_unity() {
    let sr = 48000.0;
    let mut shaper = TransientShaperProcessor::new(10, sr);

    let input = vec![0.5f32; 256];
    let mut output = vec![0.0f32; 256];
    let mut ctx = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    shaper.process(&[&input], &mut [&mut output], &mut ctx);

    // With default attack=1.0, sustain=1.0, output=1.0, steady signal matches input
    assert!(output.iter().all(|s| s.is_finite()));
    assert!((output[200] - 0.5).abs() < 0.05);
}

#[test]
fn test_transient_shaper_attack_boost() {
    let sr = 48000.0;
    let mut shaper = TransientShaperProcessor::new(11, sr);
    shaper.set_parameter(0, 2.5, 0); // Attack gain boost

    let mut input = vec![0.0f32; 256];
    // Sharp transient spike
    input[10] = 1.0;
    input[11] = 0.8;

    let mut output = vec![0.0f32; 256];
    let mut ctx = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    shaper.process(&[&input], &mut [&mut output], &mut ctx);

    assert!(output.iter().all(|s| s.is_finite()));
    assert!(output[10] > 1.0, "Transient attack spike should be boosted above 1.0");
}
