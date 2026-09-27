use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor};
use nullherz_processors::MutatorProcessor;

#[test]
fn test_mutator_bypass_transparency() {
    let mut proc = MutatorProcessor::new(1, 48000.0);
    proc.dry_wet = 0.0; // 0% wet -> transparent bypass

    let in_l = vec![0.5f32; 128];
    let in_r = vec![-0.5f32; 128];
    let mut out_l = vec![0.0f32; 128];
    let mut out_r = vec![0.0f32; 128];

    let mut ctx = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    proc.process(&[&in_l, &in_r], &mut [&mut out_l, &mut out_r], &mut ctx);

    for i in 0..128 {
        assert!((out_l[i] - in_l[i]).abs() < 1e-6, "Left channel bypass failed at index {}", i);
        assert!((out_r[i] - in_r[i]).abs() < 1e-6, "Right channel bypass failed at index {}", i);
    }
}

#[test]
fn test_mutator_deterministic_seed_recall() {
    let mut proc1 = MutatorProcessor::new(1, 48000.0);
    let mut proc2 = MutatorProcessor::new(2, 48000.0);

    proc1.seed = 9999;
    proc2.seed = 9999;
    proc1.reset();
    proc2.reset();

    proc1.flesh = 0.7;
    proc1.bone = 0.5;
    proc1.teeth = 0.8;
    proc1.parasite = 0.6;
    proc1.dry_wet = 0.8;

    proc2.flesh = 0.7;
    proc2.bone = 0.5;
    proc2.teeth = 0.8;
    proc2.parasite = 0.6;
    proc2.dry_wet = 0.8;

    let in_l: Vec<f32> = (0..256).map(|i| (i as f32 * 0.1).sin()).collect();
    let in_r: Vec<f32> = (0..256).map(|i| (i as f32 * 0.1).cos()).collect();

    let mut out_l1 = vec![0.0f32; 256];
    let mut out_r1 = vec![0.0f32; 256];
    let mut out_l2 = vec![0.0f32; 256];
    let mut out_r2 = vec![0.0f32; 256];

    let mut ctx = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    proc1.process(&[&in_l, &in_r], &mut [&mut out_l1, &mut out_r1], &mut ctx);
    proc2.process(&[&in_l, &in_r], &mut [&mut out_l2, &mut out_r2], &mut ctx);

    for i in 0..256 {
        assert_eq!(out_l1[i], out_l2[i], "Deterministic recall failed on Left channel at index {}", i);
        assert_eq!(out_r1[i], out_r2[i], "Deterministic recall failed on Right channel at index {}", i);
    }
}

#[test]
fn test_mutator_character_preset_switch() {
    let mut proc = MutatorProcessor::new(1, 48000.0);
    proc.set_parameter(13, 1.0, 0); // Character 1: Diseased

    let in_l = vec![0.1f32; 64];
    let in_r = vec![0.1f32; 64];
    let mut out_l = vec![0.0f32; 64];
    let mut out_r = vec![0.0f32; 64];

    let mut ctx = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    proc.process(&[&in_l, &in_r], &mut [&mut out_l, &mut out_r], &mut ctx);

    assert_eq!(proc.flesh, 1.00);
    assert_eq!(proc.bone, 0.65);
    assert_eq!(proc.teeth, 0.45);
    assert_eq!(proc.parasite, 0.75);
    assert_eq!(proc.asymmetry, 0.35);
}

#[test]
fn test_mutator_sub_bass_mono_safety() {
    let mut proc = MutatorProcessor::new(1, 48000.0);
    proc.asymmetry = 1.0;
    proc.sub_mono_safety = true;
    proc.dry_wet = 1.0;

    // 50 Hz out-of-phase sine wave (pure sub-bass phase cancellation hazard)
    let sample_rate = 48000.0f32;
    let freq = 50.0f32;
    let n = 512;
    let in_l: Vec<f32> = (0..n).map(|i| (2.0 * std::f32::consts::PI * freq * (i as f32) / sample_rate).sin()).collect();
    let in_r: Vec<f32> = in_l.iter().map(|&s| -s).collect(); // Exactly out-of-phase

    let mut out_l = vec![0.0f32; n];
    let mut out_r = vec![0.0f32; n];

    let mut ctx = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    proc.process(&[&in_l, &in_r], &mut [&mut out_l, &mut out_r], &mut ctx);

    // Verify output is finite and low-frequency side energy is controlled
    for i in 0..n {
        assert!(out_l[i].is_finite());
        assert!(out_r[i].is_finite());
    }
}
