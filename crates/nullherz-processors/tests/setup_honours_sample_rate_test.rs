//! Every processor must honour the rate handed to it by `setup`.
//!
//! `SignalProcessor::setup` has a default no-op body, so a processor that bakes
//! rate-derived state at construction and forgets to re-derive it compiles
//! perfectly and fails silently. That is not hypothetical: `DjIsolatorProcessor`
//! did exactly this, and because the crossover corners (300 Hz / 3 kHz) are
//! baked into biquad coefficients, a 48 kHz-constructed isolator running at
//! 44.1 kHz crossed over at ~276 Hz / ~2.76 kHz instead. Audible, and invisible
//! from the type system.
//!
//! The contract asserted here: a processor built at rate A and then `setup` to
//! rate B must behave the same as one built at rate B directly. It is the
//! backend's whole justification for calling `set_config` with the negotiated
//! rate — if processors ignore it, that feedback path does nothing.

use nullherz_traits::{AudioConfig, ProcessContext, ProcessorTypeId};

const RATE_A: f32 = 44_100.0;
const RATE_B: f32 = 96_000.0;
const N: usize = 512;

/// Deterministic broadband stimulus: an impulse plus a few partials spread
/// across the spectrum, so any shift in a filter corner or time constant shows
/// up as a difference in the output rather than being masked by a narrow probe.
fn stimulus() -> Vec<f32> {
    let mut v = vec![0.0f32; N];
    v[0] = 1.0;
    for (i, s) in v.iter_mut().enumerate() {
        let t = i as f32;
        for hz in [120.0f32, 900.0, 5_000.0, 12_000.0] {
            *s += (t * hz * std::f32::consts::TAU / RATE_B).sin() * 0.1;
        }
    }
    v
}

fn run(p: &mut dyn nullherz_traits::AudioProcessor, input: &[f32]) -> Vec<f32> {
    let mut out_l = vec![0.0f32; N];
    let mut out_r = vec![0.0f32; N];
    let mut ctx = ProcessContext {
        transport: None,
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };
    {
        let ins: [&[f32]; 2] = [input, input];
        let (l, r) = (&mut out_l[..], &mut out_r[..]);
        let mut outs: [&mut [f32]; 2] = [l, r];
        p.process(&ins, &mut outs, &mut ctx);
    }
    out_l
}

/// Processor types the 4-deck console actually instantiates, plus the ones
/// whose behaviour is rate-derived. Sources (sampler, sequencer) are excluded:
/// they read the rate from `Transport` per block rather than from `setup`, so
/// this contract does not apply to them.
fn rate_derived_types() -> Vec<(&'static str, ProcessorTypeId)> {
    vec![
        ("DjIsolator", ProcessorTypeId::DJ_ISOLATOR),
        ("Biquad", ProcessorTypeId::BIQUAD),
        ("BiquadEq", ProcessorTypeId::BIQUAD_EQ),
        ("SimdBiquad", ProcessorTypeId::SIMD_BIQUAD),
        ("MasteringEq", ProcessorTypeId::MASTERING_EQ),
        ("Limiter", ProcessorTypeId::LIMITER),
        ("Delay", ProcessorTypeId::DELAY),
        ("EnvelopeFollower", ProcessorTypeId::ENVELOPE_FOLLOWER),
        ("KeySync", ProcessorTypeId::KEY_SYNC),
    ]
}

#[test]
fn test_setup_rate_matches_construction_rate() {
    let registry = nullherz_processors::registry::ProcessorRegistry::default();
    let input = stimulus();
    let mut offenders: Vec<String> = Vec::new();

    for (name, type_id) in rate_derived_types() {
        // Built at A, then told to run at B.
        let Some(mut reconfigured) = registry.create(type_id, 0, RATE_A) else {
            continue;
        };
        reconfigured.setup(AudioConfig { sample_rate: RATE_B, block_size: N });

        // Built at B directly — the reference for "correct at B".
        let Some(mut native) = registry.create(type_id, 0, RATE_B) else {
            continue;
        };
        native.setup(AudioConfig { sample_rate: RATE_B, block_size: N });

        let a = run(reconfigured.as_mut(), &input);
        let b = run(native.as_mut(), &input);

        let peak = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max);

        // Floor well above f32 arithmetic noise but far below anything audible,
        // so reordered-but-equivalent maths passes and a real coefficient
        // difference does not.
        if peak > 1e-6 {
            offenders.push(format!(
                "{name}: peak difference {peak:.3e} between a processor built at \
                 {RATE_A} Hz then setup({RATE_B}) and one built at {RATE_B} Hz"
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "these processors ignore the rate passed to setup(), so the rate the audio \
         device negotiates never reaches their coefficients:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn test_dj_isolator_crossover_actually_moves_with_the_rate() {
    // Narrower and more direct than the sweep above: prove the isolator's
    // coefficients change at all when the rate does. If `set_sample_rate` were
    // a no-op the test above could still pass by comparing two identically
    // broken processors.
    let mut a = audio_dsp::DjIsolatorStereo::with_sample_rate(RATE_A);
    let b = audio_dsp::DjIsolatorStereo::with_sample_rate(RATE_B);

    let input = stimulus();
    let mut out_a = vec![0.0f32; N];
    let mut out_b = vec![0.0f32; N];
    let mut scratch = vec![0.0f32; N];

    a.set_gain(0, 4.0); // boost lows so the crossover position matters
    let mut b_boosted = b;
    b_boosted.set_gain(0, 4.0);

    a.process_stereo(&input, &input, &mut out_a, &mut scratch);
    b_boosted.process_stereo(&input, &input, &mut out_b, &mut scratch);

    let peak = out_a
        .iter()
        .zip(out_b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max);
    assert!(
        peak > 1e-4,
        "isolator output is identical at {RATE_A} Hz and {RATE_B} Hz (peak diff \
         {peak:.3e}) — with_sample_rate is not actually deriving from the rate, so \
         the fix for the setup() bug would be untestable"
    );

    // And the fix: after set_sample_rate, A must match B.
    let mut converted = audio_dsp::DjIsolatorStereo::with_sample_rate(RATE_A);
    converted.set_gain(0, 4.0);
    converted.set_sample_rate(RATE_B);
    let mut out_c = vec![0.0f32; N];
    converted.process_stereo(&input, &input, &mut out_c, &mut scratch);

    let peak_cb = out_c
        .iter()
        .zip(out_b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max);
    assert!(
        peak_cb <= 1e-6,
        "set_sample_rate({RATE_B}) did not reproduce a natively-built {RATE_B} isolator \
         (peak diff {peak_cb:.3e})"
    );
    assert_eq!(
        converted.gains[0], 4.0,
        "set_sample_rate must preserve band gains — rebuilding from scratch would \
         silently reset the user's EQ on every device change"
    );
}
