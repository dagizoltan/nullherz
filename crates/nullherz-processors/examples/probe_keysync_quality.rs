//! How good is KEY? The one console component with no measured number.
//!
//! Everything measured so far — the -132 dB resampler, the -148 dB chain — is
//! the VARISPEED path, keylock OFF. The moment an operator engages KEY, audio
//! goes through `KeySyncProcessor`, a phase vocoder, and nothing in this
//! repository has ever put a number on what comes out the other side. The
//! existing unit tests check that the level lands within a factor of 1.4 and
//! the pitch within 60 cents; both bounds are wide enough to pass audio nobody
//! would play out.
//!
//! That gap matters more than the ones already closed. A float mix bus is
//! transparent by construction, which is why no DJ vendor publishes a THD
//! figure for one — the distortion in DJ software lives in the resampler and
//! the keylock, and keylock is the component the field actually competes on
//! (Traktor and rekordbox both license Elastique rather than ship a vocoder of
//! their own). So this is the number that decides whether KEY is usable.
//!
//! ## What is measured
//!
//! 1. **Gate.** The analyser's own floor, and the unison path — which is not
//!    the vocoder at all but `process_identity`, pure windowed overlap-add. If
//!    COLA is exact that reads at the floor, and a reading above it means the
//!    reconstruction is broken before any pitch shifting is asked for.
//! 2. **Per interval.** Pitch error in cents, level error in dB, and the
//!    signal-to-artifact ratio. Not THD+N in the classical sense: a vocoder's
//!    artifacts are not harmonics of the fundamental but frame-rate sidebands
//!    and mistracked partials, so what is reported is all energy outside the
//!    shifted fundamental's main lobe, against the energy inside it. Same
//!    instrument as every other THD number here, read the same way.
//! 3. **Pre-echo.** The structural cost of overlap-add framing, and the one a
//!    DJ hears as a soft attack. A frame covering input `[t, t+N)` writes
//!    output `[t+N, t+2N)`, so a transient at input position `B` can appear in
//!    the output as early as `B+1` while its correct position is `B+N` — up to
//!    N-1 samples (21.3 ms at N=1024) of energy arriving BEFORE the hit. The
//!    identity path cancels that exactly; the vocoder path does not, and this
//!    measures by how much.
//! 4. **Framing sweep.** Window length against hop. `KeySyncProcessor::new`
//!    picks `N/2` — 50% overlap, the coarsest framing that reconstructs at all.
//!    The phase estimate is extrapolated across the hop, so halving the hop
//!    halves the extrapolation; it also doubles the frame rate. Both sides of
//!    that trade are reported, cost as measured wall-clock, not a proxy.
//!
//! Run: `cargo run --release --example probe_keysync_quality -p nullherz-processors`

use audio_dsp::measurement::{analyser_floor, level_db_at, spectrum, thd_n, tone_sample};
use nullherz_processors::keysync::KeySyncProcessor;
use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor};
use std::time::Instant;

const SR: f32 = 48_000.0;
/// 997 Hz, as everywhere else here: prime-ish, so it lands on no bin centre and
/// the analyser is not flattered.
const TONE: f32 = 997.0;
const AMP: f32 = 0.5;
/// The analysis length. `thd_n` reads exactly this many samples, so every
/// capture handed to it must be exactly this long or it gets zero-padded and
/// the answer is about the padding.
const ANA_FFT: usize = 16_384;
/// A realistic console block, so framing straddles block boundaries the way it
/// does in the graph.
const BLOCK: usize = 256;

/// The shipped configuration, from `factory.rs:215`.
const SHIPPED_FFT: usize = 1024;

fn ctx() -> ProcessContext<'static> {
    ProcessContext { transport: None, host: None, sub_block_offset: 0, is_last_sub_block: true }
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-30).log10()
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() { return 0.0; }
    (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len() as f64).sqrt() as f32
}

/// Push `input` through the processor in console-sized blocks.
fn run(p: &mut KeySyncProcessor, input: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0f32; input.len()];
    for start in (0..input.len()).step_by(BLOCK) {
        let end = (start + BLOCK).min(input.len());
        let ins: [&[f32]; 1] = [&input[start..end]];
        let mut outs: [&mut [f32]; 1] = [&mut out[start..end]];
        p.process(&ins, &mut outs, &mut ctx());
    }
    out
}

/// A settled `ANA_FFT`-sample capture of a steady tone through the vocoder.
///
/// Warmup is 8 windows: the pipeline needs N to fill, and `sum_phase`
/// accumulates from zero, so the first frames after a ratio change beat against
/// themselves before settling. Measuring those would report the transition, not
/// the steady state.
fn capture_tone(fft_size: usize, hop: usize, semitones: f32) -> Vec<f32> {
    let warmup = 8 * fft_size;
    let total = warmup + ANA_FFT;
    let input: Vec<f32> = (0..total).map(|i| tone_sample(i, TONE, SR, AMP)).collect();

    let mut p = KeySyncProcessor::with_framing(0, fft_size, hop);
    AudioProcessor::set_parameter(&mut p, 0, semitones, 0);
    let out = run(&mut p, &input);
    out[warmup..].to_vec()
}

/// Dominant frequency to sub-bin accuracy: peak bin, then a parabola through
/// its dB neighbours. Reading the peak bin alone quantises the answer to
/// 2.9 Hz at this size, which is 5 cents at 997 Hz — the same order as the
/// error being looked for.
fn dominant_hz(mags: &[f32]) -> f32 {
    // Skip DC and the lowest bins: the frame-rate modulation puts real energy
    // there and it is not the pitch.
    let lo = 8usize;
    let (mut best, mut best_m) = (lo, 0.0f32);
    for (b, m) in mags.iter().enumerate().skip(lo) {
        if *m > best_m { best_m = *m; best = b; }
    }
    if best == 0 || best + 1 >= mags.len() { return best as f32 * SR / ANA_FFT as f32; }
    let (a, b2, c) = (db(mags[best - 1]), db(mags[best]), db(mags[best + 1]));
    let denom = a - 2.0 * b2 + c;
    let d = if denom.abs() > 1e-12 { 0.5 * (a - c) / denom } else { 0.0 };
    (best as f32 + d.clamp(-0.5, 0.5)) * SR / ANA_FFT as f32
}

/// Half-width, in bins, of the main lobe credited to a partial. The analyser's
/// own `thd_n` uses 8 under this window; 10 here because a multi-tone capture
/// has partials whose lobes may sit closer together than a single tone's ever
/// does, and under-crediting a lobe would book real signal as artifact.
const LOBE_BINS: usize = 10;

/// Energy outside EVERY expected partial's main lobe, against the energy inside
/// them — `thd_n` generalised from one fundamental to a chord.
///
/// The single-tone number cannot decide the window length. A short window
/// tracks phase better in time and so wins on one steady partial, but it also
/// has coarse frequency resolution: at N=512 and 48 kHz a bin is 93.75 Hz, so
/// two notes a semitone apart in the bass land in the SAME bin and the vocoder
/// cannot tell them apart to shift them. That failure is invisible to a 997 Hz
/// sine and audible on any chord, which is most of the material a DJ plays.
fn artifacts_multi(x: &[f32], targets: &[f32]) -> f32 {
    let mags = spectrum(x, ANA_FFT);
    let mut in_lobes = vec![false; mags.len()];
    for &f in targets {
        let b0 = (f / SR * ANA_FFT as f32).round() as usize;
        let lo = b0.saturating_sub(LOBE_BINS);
        let hi = (b0 + LOBE_BINS).min(mags.len() - 1);
        for flag in in_lobes[lo..=hi].iter_mut() { *flag = true; }
    }
    // Accumulated directly rather than as `total - signal`: the same
    // catastrophic-cancellation trap `measurement::thd_n` documents.
    let (mut sig, mut rest) = (0.0f64, 0.0f64);
    for (b, m) in mags.iter().enumerate() {
        let e = (*m as f64) * (*m as f64);
        if in_lobes[b] { sig += e } else { rest += e }
    }
    if sig <= 0.0 { return 0.0; }
    (rest / sig).sqrt() as f32
}

/// A settled capture of a sum of steady tones, each at `AMP / n` so the sum
/// cannot clip.
fn capture_multi(fft_size: usize, hop: usize, semitones: f32, freqs: &[f32]) -> Vec<f32> {
    let warmup = 8 * fft_size;
    let total = warmup + ANA_FFT;
    let amp = AMP / freqs.len() as f32;
    let input: Vec<f32> = (0..total)
        .map(|i| freqs.iter().map(|&f| tone_sample(i, f, SR, amp)).sum())
        .collect();

    let mut p = KeySyncProcessor::with_framing(0, fft_size, hop);
    AudioProcessor::set_parameter(&mut p, 0, semitones, 0);
    let out = run(&mut p, &input);
    out[warmup..].to_vec()
}

fn main() {
    println!("KEY_SYNC (phase vocoder) quality — the console's unmeasured component\n");
    println!(
        "sample rate {} Hz   probe tone {} Hz @ {:.2} FS   analysis FFT {}   block {}",
        SR as u32, TONE as u32, AMP, ANA_FFT, BLOCK
    );

    // ---------------------------------------------------------------- gate --
    //
    // Two things have to hold before any row below is worth reading. Learned
    // the hard way on `probe_resampler_ceiling`, which happily reported -149 dB
    // through an analyser whose floor was -153: the number was measurement
    // noise dressed as a result.
    println!("\n=== 1. GATE =========================================================\n");

    let floor = db(analyser_floor(TONE, SR, ANA_FFT));
    println!("  analyser floor at {} Hz, {} pt      {:>9.1} dB", TONE as u32, ANA_FFT, floor);

    let unison = capture_tone(SHIPPED_FFT, SHIPPED_FFT / 2, 0.0);
    let unison_art = db(thd_n(&unison, TONE, SR, ANA_FFT));
    println!("  unison (process_identity, no FFT)   {unison_art:>9.1} dB");

    let p_ref = KeySyncProcessor::new(0, SHIPPED_FFT);
    let lat = SignalProcessor::latency_samples(&p_ref);
    println!(
        "  declared latency                   {:>9} smp  ({:.2} ms at {} Hz)",
        lat, lat as f32 * 1000.0 / SR, SR as u32
    );

    if unison_art > floor + 40.0 {
        println!(
            "\n  GATE FAILED: unison is {:.1} dB above the analyser floor. The identity\n  \
             path is windowed overlap-add with a COLA-normalised synthesis window,\n  \
             so it should reconstruct to float error. Something is wrong in the\n  \
             framing itself and every number below inherits it. Stopping."
        , unison_art - floor);
        std::process::exit(1);
    }
    println!(
        "\n  Unison sits {:.1} dB above the floor: overlap-add reconstruction is\n  \
         exact, and the vocoder math below is the only thing under test.",
        unison_art - floor
    );

    // ------------------------------------------------------------ intervals --
    // ------------------------------------------------------------ intervals --
    //
    // Two columns per framing, because the pair is the finding. `level RMS` is
    // the whole capture's RMS against the input's — what the unit test used —
    // and `level lobe` is the energy in the shifted fundamental's main lobe
    // alone. When artifacts sit at the signal's own level, total RMS counts them
    // as signal and reports a level that is 3 dB better than the audio is. The
    // test `test_pitch_shift_preserves_level` passed on exactly that padding.
    println!("\n=== 2. PITCH SHIFT, N={SHIPPED_FFT}: LEGACY hop N/2 vs DEFAULT hop N/8 =====\n");
    println!(
        "{:>6}  {:>9}  {:>7} |{:>10} {:>9} {:>10} |{:>10} {:>9} {:>10}",
        "semis", "target", "cents", "art N/2", "RMS N/2", "lobe N/2", "art N/8", "RMS N/8", "lobe N/8"
    );
    println!("  {}", "-".repeat(92));

    let in_tone: Vec<f32> = (0..ANA_FFT).map(|i| tone_sample(i, TONE, SR, AMP)).collect();
    let in_rms = rms(&in_tone);
    let in_lobe = level_db_at(&spectrum(&in_tone, ANA_FFT), TONE, SR, ANA_FFT);

    let mut worst = (0.0f32, f32::NEG_INFINITY);
    let mut worst_new = (0.0f32, f32::NEG_INFINITY);
    let mut worst_sag = (0.0f32, 0.0f32);

    for &semis in &[-12.0f32, -7.0, -5.0, -3.0, -1.0, 1.0, 3.0, 5.0, 7.0, 12.0] {
        let target = TONE * 2.0f32.powf(semis / 12.0);
        print!("{semis:>+6.0}  {target:>9.1}");

        let mut cents_shown = false;
        for hop in [SHIPPED_FFT / 2, SHIPPED_FFT / 8] {
            let out = capture_tone(SHIPPED_FFT, hop, semis);
            let mags = spectrum(&out, ANA_FFT);
            if !cents_shown {
                let cents = 1200.0 * (dominant_hz(&mags) / target).log2();
                print!("  {cents:>+7.1} |");
                cents_shown = true;
            }
            let art = db(thd_n(&out, target, SR, ANA_FFT));
            let lvl_rms = db(rms(&out) / in_rms.max(1e-30));
            let lvl_lobe = level_db_at(&mags, target, SR, ANA_FFT) - in_lobe;
            print!("{art:>8.1} dB {lvl_rms:>+8.2} {lvl_lobe:>+9.2} |");

            if hop == SHIPPED_FFT / 2 {
                if art > worst.1 { worst = (semis, art); }
            } else {
                if art > worst_new.1 { worst_new = (semis, art); }
                if lvl_lobe < worst_sag.1 { worst_sag = (semis, lvl_lobe); }
            }
        }
        println!();
    }

    println!(
        "\n  Worst artifact: {:+.0} semitones at {:.1} dB on the legacy hop, {:.1} dB on\n  \
         the new default — {:.0} dB for no latency and 4x the FFT work on a node\n  \
         that is detached until KEY is engaged.",
        worst.0, worst.1, worst_new.1, worst.1 - worst_new.1
    );
    println!(
        "\n  The level sag survives it: {:+.2} dB at {:+.0} semitones, measured in the\n  \
         fundamental's lobe. That is the bin-rounding remap, not the framing, and\n  \
         the legacy RMS column hid it by counting artifacts as signal.",
        worst_sag.1, worst_sag.0
    );

    // -------------------------------------------------------------- preecho --
    //
    // A gated tone burst rather than a bare impulse: an impulse has a flat
    // spectrum, which drives the per-frame energy rescale into a corner no
    // musical signal occupies, and the resulting number would be about the
    // pathology rather than about transients.
    println!("\n=== 3. PRE-ECHO (2 ms burst, N={SHIPPED_FFT}) ===========================\n");
    println!(
        "  A frame spanning input [t, t+N) writes output [t+N, t+2N), so burst\n  \
         energy can arrive up to N-1 samples ({:.1} ms) before its correct\n  \
         position. Measured in the window immediately preceding the onset.\n",
        (SHIPPED_FFT - 1) as f32 * 1000.0 / SR
    );
    println!("{:>6}  {:>12}  {:>12}  {:>12}", "semis", "burst RMS", "pre-echo", "rel. burst");
    println!("  {}", "-".repeat(48));

    let burst_len = (0.002 * SR) as usize; // 2 ms
    let b_start = 4 * SHIPPED_FFT;
    let total = b_start + 6 * SHIPPED_FFT;

    for &semis in &[0.0f32, 1.0, 5.0, -5.0, 7.0] {
        let mut input = vec![0.0f32; total];
        for i in 0..burst_len {
            // Raised-cosine attack over the first 10% so the burst is a
            // percussive hit and not a step discontinuity.
            let env = {
                let a = (burst_len / 10).max(1);
                if i < a { 0.5 * (1.0 - (std::f32::consts::PI * i as f32 / a as f32).cos()) } else { 1.0 }
            };
            input[b_start + i] = tone_sample(i, TONE, SR, AMP) * env;
        }

        let mut p = KeySyncProcessor::with_framing(0, SHIPPED_FFT, SHIPPED_FFT / 2);
        AudioProcessor::set_parameter(&mut p, 0, semis, 0);
        let out = run(&mut p, &input);

        // Correct onset is the burst position plus one window of latency.
        let onset = b_start + SHIPPED_FFT;
        // The window immediately before it, less a 32-sample guard so the
        // burst's own leading edge is not counted as its own pre-echo.
        let pre = &out[onset - SHIPPED_FFT + 1..onset - 32];
        let body = &out[onset..onset + burst_len];

        let (pre_rms, body_rms) = (rms(pre), rms(body));
        let note = if semis == 0.0 { "  <- identity path" } else { "" };
        println!(
            "{semis:>+6.0}  {:>12.2}  {:>12.2}  {:>+9.1} dB{note}",
            db(body_rms), db(pre_rms), db(pre_rms / body_rms.max(1e-30))
        );
    }

    // -------------------------------------------------------------- framing --
    println!("\n=== 4. FRAMING SWEEP (at the worst interval, {:+.0} semitones) ==========\n", worst.0);
    println!(
        "{:>6}  {:>6}  {:>9}  {:>11}  {:>9}  {:>10}  {:>10}",
        "N", "hop", "overlap", "artifacts", "level dB", "latency", "x realtime"
    );
    println!("  {}", "-".repeat(74));

    for &fft_size in &[512usize, 1024, 2048] {
        for &div in &[2usize, 4, 8] {
            let hop = fft_size / div;

            let warmup = 8 * fft_size;
            let total = warmup + ANA_FFT;
            let input: Vec<f32> = (0..total).map(|i| tone_sample(i, TONE, SR, AMP)).collect();
            let mut p = KeySyncProcessor::with_framing(0, fft_size, hop);
            AudioProcessor::set_parameter(&mut p, 0, worst.0, 0);

            let t0 = Instant::now();
            let out = run(&mut p, &input);
            let elapsed = t0.elapsed().as_secs_f64();

            let settled = &out[warmup..];
            let art = db(thd_n(settled, TONE * 2.0f32.powf(worst.0 / 12.0), SR, ANA_FFT));
            let level = db(rms(settled) / in_rms.max(1e-30));
            let audio_secs = total as f64 / SR as f64;
            let marker = if fft_size == SHIPPED_FFT && div == 2 { "  <- shipped" } else { "" };

            println!(
                "{fft_size:>6}  {hop:>6}  {:>8.0}%  {art:>8.1} dB  {level:>+9.2}  {:>7.2} ms  {:>9.0}x{marker}",
                100.0 * (1.0 - 1.0 / div as f32),
                fft_size as f32 * 1000.0 / SR,
                audio_secs / elapsed.max(1e-9)
            );
        }
    }

    println!(
        "\n  Latency tracks N alone — one analysis window, whatever the hop — so\n  \
         the hop column buys quality at CPU cost and nothing else. N buys\n  \
         frequency resolution at latency cost: at 48 kHz, N=2048 is 42.7 ms of\n  \
         PDC on every deck carrying KEY, against 10.7 ms at N=512."
    );

    // ------------------------------------------------------ candidates, all --
    //
    // Section 4 swept one interval. A framing that happens to suit +7 semitones
    // is not a framing choice; these are the same candidates across the whole
    // range the parameter allows.
    println!("\n=== 5. CANDIDATE FRAMINGS ACROSS EVERY INTERVAL (single tone) ========\n");

    // The shipped framing, then the plausible replacements. Section 6 is what
    // ranks them: a candidate is only as good as its worst polyphonic interval.
    let candidates: [(usize, usize); 6] = [
        (SHIPPED_FFT, SHIPPED_FFT / 2),
        (1024, 128),
        (1024, 64),
        (2048, 256),
        (2048, 128),
        (4096, 256),
    ];
    print!("{:>6}", "semis");
    for (n, h) in candidates { print!("{:>14}", format!("{n}/{h}")); }
    println!();
    println!("  {}", "-".repeat(62));

    let mut worst_per_cand = vec![f32::NEG_INFINITY; candidates.len()];
    for &semis in &[-12.0f32, -7.0, -5.0, -3.0, -1.0, 1.0, 3.0, 5.0, 7.0, 12.0] {
        let target = TONE * 2.0f32.powf(semis / 12.0);
        print!("{semis:>+6.0}");
        for (ci, &(n, h)) in candidates.iter().enumerate() {
            let out = capture_tone(n, h, semis);
            let art = db(thd_n(&out, target, SR, ANA_FFT));
            if art > worst_per_cand[ci] { worst_per_cand[ci] = art; }
            print!("{:>11.1} dB", art);
        }
        println!();
    }
    println!("  {}", "-".repeat(62));
    print!("{:>6}", "worst");
    for w in &worst_per_cand { print!("{:>11.1} dB", w); }
    println!("   <- the number that decides\n");

    // ------------------------------------------------------------ polyphonic --
    println!("=== 6. POLYPHONIC — A2/C3/E3, where a short window cannot resolve ====\n");
    let chord = [110.0f32, 130.81, 164.81];
    println!(
        "  Partials {:.0}/{:.0}/{:.0} Hz, spaced {:.0} and {:.0} Hz. Bin width is\n  \
         {:.1} Hz at N=512, {:.1} Hz at N=1024 and {:.1} Hz at N=2048 — only the\n  \
         last resolves all three, and a vocoder cannot shift partials it cannot\n  \
         separate.\n",
        chord[0], chord[1], chord[2],
        chord[1] - chord[0], chord[2] - chord[1],
        SR / 512.0, SR / 1024.0, SR / 2048.0
    );
    print!("{:>6}", "semis");
    for (n, h) in candidates { print!("{:>14}", format!("{n}/{h}")); }
    println!();
    println!("  {}", "-".repeat(62));

    let mut worst_poly = vec![f32::NEG_INFINITY; candidates.len()];
    for &semis in &[-7.0f32, -3.0, 3.0, 7.0] {
        let ratio = 2.0f32.powf(semis / 12.0);
        let targets: Vec<f32> = chord.iter().map(|f| f * ratio).collect();
        print!("{semis:>+6.0}");
        for (ci, &(n, h)) in candidates.iter().enumerate() {
            let out = capture_multi(n, h, semis, &chord);
            let art = db(artifacts_multi(&out, &targets));
            if art > worst_poly[ci] { worst_poly[ci] = art; }
            print!("{:>11.1} dB", art);
        }
        println!();
    }
    println!("  {}", "-".repeat(62));
    print!("{:>6}", "worst");
    for w in &worst_poly { print!("{:>11.1} dB", w); }
    println!("   <- the number that decides\n");

    print!("  latency");
    for (n, _) in candidates { print!("{:>11.2} ms", n as f32 * 1000.0 / SR); }
    println!();

    // -------------------------------------------------------- is it timbral --
    //
    // At the new hop the output is CLEAN (-56 dB artifacts) and merely QUIET
    // (-7.5 dB). That combination invites a cheap fix: a per-interval makeup
    // gain. It is only a fix if the sag is the same for every partial. If each
    // partial sags by a different amount the error is timbral, no scalar can
    // correct it, and the remap has to go.
    println!("\n=== 7. IS THE SAG A LEVEL ERROR OR A TIMBRE ERROR? ==================\n");
    println!(
        "  Per-partial lobe level on the A2/C3/E3 chord, N={SHIPPED_FFT} hop N/8. A uniform\n  \
         column is a level error and a makeup gain fixes it; a spread column is\n  \
         the remap redistributing energy between partials.\n"
    );
    println!("{:>6}  {:>11}  {:>11}  {:>11}  {:>9}", "semis", "A2", "C3", "E3", "spread");
    println!("  {}", "-".repeat(56));

    let amp = AMP / chord.len() as f32;
    let dry: Vec<f32> = (0..ANA_FFT)
        .map(|i| chord.iter().map(|&f| tone_sample(i, f, SR, amp)).sum())
        .collect();
    let dry_mags = spectrum(&dry, ANA_FFT);

    let mut worst_spread = 0.0f32;
    for &semis in &[-7.0f32, -3.0, 3.0, 7.0] {
        let ratio = 2.0f32.powf(semis / 12.0);
        let out = capture_multi(SHIPPED_FFT, SHIPPED_FFT / 8, semis, &chord);
        let mags = spectrum(&out, ANA_FFT);

        print!("{semis:>+6.0}");
        let mut errs = Vec::new();
        for &f in &chord {
            let e = level_db_at(&mags, f * ratio, SR, ANA_FFT) - level_db_at(&dry_mags, f, SR, ANA_FFT);
            errs.push(e);
            print!("{e:>+9.2} dB");
        }
        let spread = errs.iter().cloned().fold(f32::MIN, f32::max)
            - errs.iter().cloned().fold(f32::MAX, f32::min);
        if spread > worst_spread { worst_spread = spread; }
        println!("{spread:>+9.2} dB");
    }

    println!(
        "\n  Worst spread between partials: {worst_spread:.2} dB.\n"
    );
    if worst_spread > 3.0 {
        println!(
            "  Timbral. The partials do not sag together, so no makeup gain can\n  \
             correct it — a scalar tuned on a sine would mis-level every chord. The\n  \
             bin-rounding remap has to be replaced by time-stretch plus resampling,\n  \
             which is the route that puts the pitch change back through the -132 dB\n  \
             resampler instead of through integer bin arithmetic."
        );
    } else {
        println!(
            "  Uniform enough to be a level error: a per-interval makeup gain derived\n  \
             from the ratio would recover it without touching the remap."
        );
    }
}
