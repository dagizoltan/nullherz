//! **How low can we actually SEE, and what would better precision buy?**
//!
//! This is the working that produced two decisions now recorded in
//! ARCHITECTURE.md. It keeps its own f64 analyser deliberately, one notch
//! sharper than the shipped one in `audio_dsp::measurement`, so it can grade the
//! instrument itself rather than only what the instrument can see.
//!
//! **1. The analyser was the ceiling.** The console read 0.0046% against a
//! 0.0046% Hann floor — every node measured "at the floor" because the floor WAS
//! the measurement. Hann's first sidelobe is -31 dB and its skirts decay slowly,
//! so its +/-24-bin exclusion was not measuring a main lobe, it was hiding
//! sidelobes. BH7's sidelobes are below -180 dB, so the exclusion NARROWS to
//! +/-8 and the floor drops ~48 dB. The shipped analyser is now BH7; with the
//! production f32 `SimdFft` it floors at -134.7 dB, and the f64 FFT here shows
//! where the remaining limits are (f32 sample storage, -153 dB).
//!
//! **2. The console's real residual is -107 dB, and it is one node.** Not
//! distortion — worst harmonic -152 dBc. `DjIsolator` alone accounts for it;
//! `Gain`, `MasteringEq` and `StereoUtility` are bit-exact identity. The
//! precision sweep at the end prices up f64 state per band, and finds the MID
//! band's HP300^2 pair carries 15 of the 43 dB available. Left in f32: -107 dB
//! is below the 16-bit floor and below any DAC.
//!
//! Sections: window floors on an ideal f64 tone -> the same tone in f32 storage
//! -> the real console -> per-node -> residue shape -> isolator precision.
//!
//!   cargo run --release -p nullherz-conductor --example probe_analyser_resolution

use std::sync::Arc;
use nullherz_traits::{Command, PerformanceCommand};

const SR: f64 = 48_000.0;
const BLOCK: usize = 256;
const FFT: usize = 16_384;
const TONE: f64 = 997.0;
const SETTLE_BLOCKS: usize = 256;

fn db(x: f64) -> f64 { 20.0 * x.max(1e-30).log10() }

#[derive(Clone, Copy, PartialEq)]
enum Win { Hann, Bh4, Bh7 }

impl Win {
    fn name(self) -> &'static str {
        match self { Win::Hann => "Hann", Win::Bh4 => "Blackman-Harris 4", Win::Bh7 => "Blackman-Harris 7" }
    }
    /// Cosine-sum coefficients. Sidelobe levels: Hann -31 dB, BH4 -92 dB,
    /// BH7 -180 dB. The last one is below f64's own noise, so it stops being
    /// the limit entirely.
    fn coeffs(self) -> &'static [f64] {
        match self {
            Win::Hann => &[0.5, 0.5],
            Win::Bh4 => &[0.35875, 0.48829, 0.14128, 0.01168],
            Win::Bh7 => &[
                0.271_051_400_693_42, 0.433_297_939_234_48, 0.218_122_999_543_11,
                0.065_925_446_388_03, 0.010_811_742_098_37, 0.000_776_584_825_22,
                0.000_013_887_217_35,
            ],
        }
    }
    fn at(self, i: usize, n: usize) -> f64 {
        let c = self.coeffs();
        let x = std::f64::consts::TAU * i as f64 / n as f64;
        (0..c.len()).map(|k| {
            let s = if k % 2 == 1 { -1.0 } else { 1.0 };
            s * c[k] * (k as f64 * x).cos()
        }).sum()
    }
    /// Main-lobe half-width in bins. A cosine-sum window with `k` terms has a
    /// main lobe `2k` bins wide, so half-width `k`; +1 bin of margin because
    /// 997 Hz does not sit on a bin centre.
    fn half_width(self) -> usize { self.coeffs().len() + 1 }
}

/// Radix-2 FFT in f64. The production `SimdFft` is f32, whose arithmetic floor
/// (~-130 dB for this size) would otherwise become the thing we are measuring.
fn fft_f64(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0usize;
    for i in 0..n {
        if i < j { re.swap(i, j); im.swap(i, j); }
        let mut m = n >> 1;
        while m >= 1 && j >= m { j -= m; m >>= 1; }
        j += m;
    }
    let mut len = 2usize;
    while len <= n {
        let ang = -std::f64::consts::TAU / len as f64;
        for i in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (ws, wc) = (ang * k as f64).sin_cos();
                let (ar, ai) = (re[i + k], im[i + k]);
                let (br, bi) = (re[i + k + len / 2], im[i + k + len / 2]);
                let (vr, vi) = (br * wc - bi * ws, br * ws + bi * wc);
                re[i + k] = ar + vr;
                im[i + k] = ai + vi;
                re[i + k + len / 2] = ar - vr;
                im[i + k + len / 2] = ai - vi;
            }
        }
        len <<= 1;
    }
}

fn thd(x: &[f64], w: Win, excl: usize) -> f64 {
    let mut re: Vec<f64> = (0..FFT).map(|i| x[i] * w.at(i, FFT)).collect();
    let mut im = vec![0.0f64; FFT];
    fft_f64(&mut re, &mut im);
    let fund_bin = (TONE / SR * FFT as f64).round() as usize;
    // Accumulate the residual DIRECTLY, never as `total - fund`. The
    // fundamental is ~15 orders of magnitude larger than what we are trying to
    // resolve, so that subtraction cancels the answer away entirely and reports
    // a clean 0.0 — an analyser that says "perfect" because it ran out of
    // mantissa, which is the same class of mistake as the one being chased.
    let (mut fund, mut rest) = (0.0f64, 0.0f64);
    for b in 0..FFT / 2 {
        let e = re[b] * re[b] + im[b] * im[b];
        if b.abs_diff(fund_bin) <= excl { fund += e } else { rest += e }
    }
    if fund <= 0.0 { return 0.0; }
    (rest / fund).sqrt()
}

fn ideal_tone(n: usize, amp: f64) -> Vec<f64> {
    (0..n).map(|i| {
        let p = (i as f64 * TONE * std::f64::consts::TAU / SR).rem_euclid(std::f64::consts::TAU);
        p.sin() * amp
    }).collect()
}

fn pump(c: &mut nullherz_conductor::Conductor, l: &mut [f32], r: &mut [f32]) {
    let n = l.len();
    let inputs: Vec<&[f32]> = vec![];
    let mut outs = vec![l, r];
    let mut lock = c.engine_coordinator.backend_manager.engine_handle.lock();
    let arc = lock.as_mut().expect("engine");
    let p = Arc::as_ptr(arc) as *mut dyn nullherz_traits::RenderingEngine;
    unsafe { (*p).process_block(&inputs, &mut outs, n); }
}

fn render_console(amp: f32) -> Vec<f64> {
    let mut c = nullherz_conductor::Conductor::with_library_path(":memory:");
    c.setup_engine();
    c.bootstrap_4channel_mixer();
    let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    for _ in 0..SETTLE_BLOCKS { pump(&mut c, &mut l, &mut r); }

    let frames = SR as usize * 4;
    let mut samples = vec![0.0f32; frames * 2];
    for ch in 0..2 {
        for i in 0..frames {
            samples[ch * frames + i] =
                audio_dsp::measurement::tone_sample(i, TONE as f32, SR as f32, amp);
        }
    }
    let mut meta = nullherz_traits::SampleMetadata::new_empty();
    meta.bpm = 120.0;
    meta.total_samples = frames as u64;
    meta.channels = 2;
    meta.sample_rate = SR as u32;
    c.transfusion_manager.sample_registry
        .register_with_metadata(7001, Arc::new(samples), Arc::new(meta));
    c.apply_mixer_commands(vec![
        Command::Performance(PerformanceCommand::LoadTrackToDeck { deck_id: 'A', sample_id: 7001 }),
        Command::Performance(PerformanceCommand::PlayDeck { deck_id: 'A' }),
    ]);
    for _ in 0..SETTLE_BLOCKS { l.fill(0.0); r.fill(0.0); pump(&mut c, &mut l, &mut r); }

    let mut out = Vec::with_capacity(FFT);
    while out.len() < FFT {
        l.fill(0.0); r.fill(0.0);
        pump(&mut c, &mut l, &mut r);
        out.extend(l.iter().map(|&v| v as f64));
    }
    out.truncate(FFT);
    out
}

fn main() {
    let ideal = ideal_tone(FFT, 0.5);
    // Same tone, round-tripped through f32 storage — the format the console's
    // buffers are in. This is the hard floor no analyser can see past.
    let quantised: Vec<f64> = ideal.iter().map(|&v| (v as f32) as f64).collect();

    println!("=== analyser floor by window (ideal f64 tone, f64 FFT) ===");
    println!("  {:<20} {:>6}  {:>12}  {:>10}", "window", "excl", "THD+N", "dB");
    for w in [Win::Hann, Win::Bh4, Win::Bh7] {
        // The exclusion the production analyser uses, vs the main lobe's own
        // width. Hann needs the wide one; the BH windows do not.
        for excl in [w.half_width(), 24] {
            let t = thd(&ideal, w, excl);
            let tag = if excl == w.half_width() { "main lobe" } else { "wide exclusion" };
            println!("  {:<20} {:>6}  {:>11.7}%  {:>9.1}   {}", w.name(), excl, t * 100.0, db(t), tag);
        }
    }

    println!("\n=== same windows, tone stored as f32 (the console's format) ===");
    for w in [Win::Hann, Win::Bh4, Win::Bh7] {
        let t = thd(&quantised, w, w.half_width());
        println!("  {:<20} {:>6}  {:>11.7}%  {:>9.1}", w.name(), w.half_width(), t * 100.0, db(t));
    }

    println!("\n=== the real console at unity, -6 dBFS ===");
    let console = render_console(0.5);
    for w in [Win::Hann, Win::Bh4, Win::Bh7] {
        let t = thd(&console, w, w.half_width());
        let floor = thd(&quantised, w, w.half_width());
        println!(
            "  {:<20} {:>6}  {:>11.7}%  {:>9.1}   (f32-storage floor {:.7}%, headroom {:+.1} dB)",
            w.name(), w.half_width(), t * 100.0, db(t), floor * 100.0, db(t) - db(floor)
        );
    }

    // ---- per-node, at 60 dB more resolution than the Hann analyser had ------
    // The earlier bisect could only say "every node is at the floor" because
    // the floor was -87 dB. At -155 dB the nodes separate.
    println!("\n=== single nodes under BH7 (f32-storage floor -155.4 dB) ===");
    {
        const WARM: usize = 8192;
        let n = WARM + FFT;
        let input: Vec<f32> = (0..n)
            .map(|i| audio_dsp::measurement::tone_sample(i, TONE as f32, SR as f32, 0.5))
            .collect();
        let run = |name: &str, f: &mut dyn FnMut(&[f32], &mut [f32])| {
            let mut out = vec![0.0f32; n];
            let mut i = 0;
            while i < n {
                let b = BLOCK.min(n - i);
                f(&input[i..i + b], &mut out[i..i + b]);
                i += b;
            }
            let tail: Vec<f64> = out[WARM..].iter().map(|&v| v as f64).collect();
            let t = thd(&tail, Win::Bh7, Win::Bh7.half_width());
            println!("  {name:<28} {:>11.7}%  {:>8.1} dB", t * 100.0, db(t));
        };

        run("passthrough (storage floor)", &mut |i, o| o.copy_from_slice(i));

        let mut iso = audio_dsp::DjIsolatorStereo::with_sample_rate(SR as f32);
        let mut scratch = vec![0.0f32; BLOCK];
        run("DjIsolatorStereo", &mut |i, o| {
            iso.process_stereo(i, i, o, &mut scratch[..i.len()]);
        });

        let mut eq = audio_dsp::MasteringEq::with_sample_rate(SR as f32);
        run("MasteringEq", &mut |i, o| {
            let mut oo = [o];
            audio_dsp::DspKernel::process(&mut eq, &[i], &mut oo);
        });

        let mut gain = audio_dsp::Gain::new(1.0, 0.05);
        run("Gain @ 1.0", &mut |i, o| {
            let mut oo = [o];
            audio_dsp::DspKernel::process(&mut gain, &[i], &mut oo);
        });

        let mut su = nullherz_processors::StereoUtilityProcessor::new(1);
        let mut r_in = vec![0.0f32; BLOCK];
        let mut r_out = vec![0.0f32; BLOCK];
        run("StereoUtility (M/S round trip)", &mut |i, o| {
            r_in[..i.len()].copy_from_slice(i);
            let mut ctx = nullherz_traits::ProcessContext {
                transport: None, host: None, sub_block_offset: 0, is_last_sub_block: false };
            let (a, b) = (&mut o[..], &mut r_out[..i.len()]);
            let mut outs: [&mut [f32]; 2] = [a, b];
            nullherz_traits::SignalProcessor::process(
                &mut su, &[i, &r_in[..i.len()]], &mut outs, &mut ctx);
        });
    }

    // With a window that is no longer the limit, the residue's SHAPE is finally
    // readable. Harmonics => non-linearity. Broadband => precision.
    println!("\n=== where the console's residue actually sits (BH7) ===");
    let mut re: Vec<f64> = (0..FFT).map(|i| console[i] * Win::Bh7.at(i, FFT)).collect();
    let mut im = vec![0.0f64; FFT];
    fft_f64(&mut re, &mut im);
    let fund_bin = (TONE / SR * FFT as f64).round() as usize;
    let fund = (re[fund_bin] * re[fund_bin] + im[fund_bin] * im[fund_bin]).sqrt();
    println!("  harmonics of {TONE} Hz:");
    for h in 2..=7 {
        let b = (TONE * h as f64 / SR * FFT as f64).round() as usize;
        let m = (b.saturating_sub(2)..=(b + 2).min(FFT / 2 - 1))
            .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt())
            .fold(0.0f64, f64::max);
        println!("    h{h} ({:>6.0} Hz)  {:>8.1} dBc", TONE * h as f64, db(m) - db(fund));
    }
    let mut peaks: Vec<(usize, f64)> = (1..FFT / 2)
        .filter(|b| b.abs_diff(fund_bin) > Win::Bh7.half_width())
        .map(|b| (b, (re[b] * re[b] + im[b] * im[b]).sqrt()))
        .collect();
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    print!("  loudest non-fundamental bins:");
    for (b, m) in peaks.iter().take(5) {
        print!("  {:.0}Hz@{:.0}dBc", *b as f64 * SR / FFT as f64, db(*m) - db(fund));
    }
    println!();

    isolator_precision();
}

/// One DF2T biquad whose STATE is either f32 or f64. Coefficients are held in
/// f64 and narrowed on use, so the f32 variant is arithmetically identical to
/// the production `BiquadFilter` (verified: it reproduces its -107.2 dB).
struct Stage { b0: f64, b1: f64, b2: f64, a1: f64, a2: f64, hi: bool, z1: f64, z2: f64 }

impl Stage {
    fn new(c: audio_dsp::BiquadCoefficients, hi: bool) -> Self {
        Self { b0: c.b0 as f64, b1: c.b1 as f64, b2: c.b2 as f64,
               a1: c.a1 as f64, a2: c.a2 as f64, hi, z1: 0.0, z2: 0.0 }
    }
    fn process(&mut self, x: f64) -> f64 {
        if self.hi {
            let y = x * self.b0 + self.z1;
            self.z1 = x * self.b1 - y * self.a1 + self.z2;
            self.z2 = x * self.b2 - y * self.a2;
            y
        } else {
            let (b0, b1, b2) = (self.b0 as f32, self.b1 as f32, self.b2 as f32);
            let (a1, a2) = (self.a1 as f32, self.a2 as f32);
            let (xf, z1, z2) = (x as f32, self.z1 as f32, self.z2 as f32);
            let y = xf * b0 + z1;
            self.z1 = (xf * b1 - y * a1 + z2) as f64;
            self.z2 = (xf * b2 - y * a2) as f64;
            y as f64
        }
    }
}

/// The production isolator's exact band structure, with per-STAGE precision.
/// LOW = LP300^2, MID = HP300^2 LP3000^2, HIGH = HP3000^2, summed at unity.
/// Flags are `[low0 low1, mid_hp0 mid_hp1 mid_lp0 mid_lp1, high0 high1]`.
fn build(f: [bool; 8]) -> (Vec<Stage>, Vec<Stage>, Vec<Stage>) {
    let sr = SR as f32;
    let lp3 = audio_dsp::BiquadCoefficients::linkwitz_riley_lp(300.0, sr);
    let hp3 = audio_dsp::BiquadCoefficients::linkwitz_riley_hp(300.0, sr);
    let lp30 = audio_dsp::BiquadCoefficients::linkwitz_riley_lp(3000.0, sr);
    let hp30 = audio_dsp::BiquadCoefficients::linkwitz_riley_hp(3000.0, sr);
    (
        vec![Stage::new(lp3, f[0]), Stage::new(lp3, f[1])],
        vec![Stage::new(hp3, f[2]), Stage::new(hp3, f[3]),
             Stage::new(lp30, f[4]), Stage::new(lp30, f[5])],
        vec![Stage::new(hp30, f[6]), Stage::new(hp30, f[7])],
    )
}

fn isolator_precision() {
    const WARM: usize = 8192;
    let n = WARM + FFT;
    let input: Vec<f32> = (0..n)
        .map(|i| audio_dsp::measurement::tone_sample(i, TONE as f32, SR as f32, 0.5))
        .collect();

    // `sum_hi` mirrors the band precision: an all-f32 isolator also sums in f32.
    let run = |label: &str, flags: [bool; 8], sum_hi: bool| {
        let (mut l, mut m, mut h) = build(flags);
        let out: Vec<f64> = input.iter().map(|&x| {
            let xd = x as f64;
            let (mut a, mut b, mut c) = (xd, xd, xd);
            for s in l.iter_mut() { a = s.process(a); }
            for s in m.iter_mut() { b = s.process(b); }
            for s in h.iter_mut() { c = s.process(c); }
            let y = if sum_hi { a + b + c } else { ((a as f32) + (b as f32) + (c as f32)) as f64 };
            (y as f32) as f64
        }).collect();
        let t = thd(&out[WARM..], Win::Bh7, Win::Bh7.half_width());
        println!("  {label:<40} {:>11.7}%  {:>8.1} dB", t * 100.0, db(t));
    };

    const N: bool = false;
    const Y: bool = true;
    println!("\n=== isolator precision variants (f32 output floor -153.3 dB) ===");
    println!("  {:<40} {:>12}  {:>11}   f64 stages", "variant", "THD+N", "");
    run("all f32 (production)", [N; 8], false);
    run("f64 LOW band  (LP300^2)", [Y, Y, N, N, N, N, N, N], false);
    run("f64 MID band  (HP300^2 LP3000^2)", [N, N, Y, Y, Y, Y, N, N], false);
    run("f64 HIGH band (HP3000^2)", [N, N, N, N, N, N, Y, Y], false);

    // Which HALF of the MID band? HP300 has b0 ~= 0.97 (full-scale state);
    // LP300/LP3000 scale the input down at the numerator before the recursion,
    // so their state carries far smaller values and rounds to far less absolute
    // noise. If that reasoning holds, the two HP300 stages are the whole story.
    println!("  --- inside the MID band ---");
    run("f64 MID's HP300^2 only  (2 of 8)", [N, N, Y, Y, N, N, N, N], false);
    run("f64 MID's LP3000^2 only (2 of 8)", [N, N, N, N, Y, Y, N, N], false);

    println!("  --- combinations ---");
    run("f64 all 300 Hz stages   (4 of 8)", [Y, Y, Y, Y, N, N, N, N], false);
    run("f64 LOW+MID bands       (6 of 8)", [Y, Y, Y, Y, Y, Y, N, N], false);
    run("f64 everything          (8 of 8)", [Y; 8], true);
}
