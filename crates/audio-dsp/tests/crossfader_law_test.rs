//! **The crossfader curve must be a knob, not a switch.**
//!
//! `curve` has always been documented "0.0 = Linear, 1.0 = Power", but was
//! implemented as `if curve > 0.5 { power } else { linear }` — two positions,
//! with 84% of the control's travel doing nothing and the default (0.5) landing
//! exactly on the boundary, so the shipped law was chosen by a comparison
//! operator rather than by anyone.

use audio_dsp::Crossfader;

fn db(x: f32) -> f32 { 20.0 * x.max(1e-20).log10() }

/// The two endpoints must actually be the two laws they claim to be.
#[test]
fn test_curve_endpoints_are_linear_and_constant_power() {
    let mut xf = Crossfader::new();
    xf.set_position(0.5);

    xf.set_curve(0.0);
    let (a, b) = xf.gains();
    assert!((a - 0.5).abs() < 1e-6 && (b - 0.5).abs() < 1e-6,
        "curve 0.0 at centre gave ({a}, {b}); linear must be (0.5, 0.5), i.e. -6.02 dB per side");

    xf.set_curve(1.0);
    let (a, b) = xf.gains();
    let root_half = 0.5f32.sqrt();
    assert!((a - root_half).abs() < 1e-6 && (b - root_half).abs() < 1e-6,
        "curve 1.0 at centre gave ({a}, {b}); constant power must be (0.7071, 0.7071), i.e. -3.01 dB per side");
}

/// THE guard: the control must be CONTINUOUS across its travel.
///
/// Against the old binary branch this fails immediately — every curve value at
/// or below 0.5 returns identical gains, so the control is flat for the first
/// half of its range and then steps.
#[test]
fn test_curve_is_continuous_not_a_two_position_switch() {
    let mut xf = Crossfader::new();
    xf.set_position(0.5);

    let mut prev = {
        xf.set_curve(0.0);
        xf.gains().0
    };
    let mut steps = Vec::new();
    for i in 1..=32 {
        xf.set_curve(i as f32 / 32.0);
        let g = xf.gains().0;
        steps.push(g - prev);
        prev = g;
    }

    // Every step must move, and none may dominate: a switch shows up as 31
    // zero steps and one large jump.
    let largest = steps.iter().cloned().fold(0.0f32, f32::max);
    let smallest = steps.iter().cloned().fold(f32::INFINITY, f32::min);
    assert!(
        smallest > 1e-5,
        "curve has DEAD TRAVEL: the smallest step across 32 positions was \
         {smallest:.2e}. The control is documented as a continuous blend from \
         linear to constant power; a region where turning it does nothing means \
         it is still a threshold."
    );
    assert!(
        largest < smallest * 4.0,
        "curve is DISCONTINUOUS: largest step {largest:.5} against smallest \
         {smallest:.5}. A jump means automating the curve across that point \
         clicks."
    );
}

/// Centre level must land between the two laws for intermediate curve values,
/// and never boost.
#[test]
fn test_intermediate_curve_sits_between_the_two_laws() {
    let mut xf = Crossfader::new();
    xf.set_position(0.5);
    xf.set_curve(0.5);
    let (a, _) = xf.gains();
    assert!(
        db(a) > -6.03 && db(a) < -3.00,
        "curve 0.5 at centre is {:.2} dB; it must sit strictly between linear \
         (-6.02) and constant power (-3.01), not snap to one of them",
        db(a)
    );
}

/// Gains must never exceed unity at any position or curve. A crossfader
/// attenuates; one that boosts eats headroom the operator did not give it.
#[test]
fn test_crossfader_never_boosts() {
    let mut xf = Crossfader::new();
    for pi in 0..=20 {
        for ci in 0..=20 {
            xf.set_position(pi as f32 / 20.0);
            xf.set_curve(ci as f32 / 20.0);
            let (a, b) = xf.gains();
            assert!(
                a <= 1.0 + 1e-6 && b <= 1.0 + 1e-6 && a >= 0.0 && b >= 0.0,
                "position {} curve {} gave gains ({a}, {b}) — outside [0, 1]",
                pi as f32 / 20.0, ci as f32 / 20.0
            );
        }
    }
}

/// The default must be a deliberate law, not whatever a boundary comparison
/// happens to select.
#[test]
fn test_default_is_constant_power() {
    let xf = Crossfader::new();
    assert_eq!(xf.position, 0.5, "crossfader should default to centre");
    let (a, b) = xf.gains();
    let root_half = 0.5f32.sqrt();
    assert!(
        (a - root_half).abs() < 1e-6 && (b - root_half).abs() < 1e-6,
        "default crossfader at centre gives ({a}, {b}) = {:.2} dB per side. \
         Expected constant power (-3.01 dB), the hardware convention for two \
         uncorrelated tracks. If linear is wanted, change it deliberately and \
         say so here — the point of this test is that the default stops being \
         an accident of `>` versus `>=`.",
        db(a)
    );
}
