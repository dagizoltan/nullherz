use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor};
use nullherz_processors::TapeSaturatorProcessor;

#[test]
fn test_tape_saturator_processing() {
    let sr = 48000.0;
    let mut tape = TapeSaturatorProcessor::new(20, sr);

    let input: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1).sin()).collect();
    let mut output = vec![0.0f32; 512];
    let mut ctx = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    tape.process(&[&input], &mut [&mut output], &mut ctx);

    assert!(output.iter().all(|s| s.is_finite()));
    let energy: f32 = output.iter().map(|s| s.abs()).sum();
    assert!(energy > 0.0, "Output energy must be non-zero");
}

#[test]
fn test_tape_saturator_parameter_updates() {
    let sr = 48000.0;
    let mut tape = TapeSaturatorProcessor::new(21, sr);

    tape.set_parameter(0, 3.0, 0); // Drive
    tape.set_parameter(1, 8000.0, 0); // HighCut
    tape.set_parameter(2, 0.2, 0); // WowDepth
    tape.set_parameter(3, 1.0, 0); // WowRate
    tape.set_parameter(4, 0.8, 0); // Gain

    assert_eq!(tape.get_parameter(0), 3.0);
    assert_eq!(tape.get_parameter(1), 8000.0);
    assert_eq!(tape.get_parameter(2), 0.2);
    assert_eq!(tape.get_parameter(3), 1.0);
    assert_eq!(tape.get_parameter(4), 0.8);
}
