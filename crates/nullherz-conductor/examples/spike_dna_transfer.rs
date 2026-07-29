//! **Spike (measurement, not a feature): does donor→recipient DNA transfer do
//! anything musically meaningful?**
//!
//! `PersonalityInheritanceProcessor` is the only implementation of the transfer
//! model the project is named for, and it has never run with real DNA — its
//! `set_source_personality` is called from exactly one unit test, so the donor
//! has been permanently all-zero. Phases 2–5 of the roadmap build a great deal
//! of infrastructure (L3 DNA layer, chunked container, P2P gossip) whose purpose
//! is to carry this. This measures whether it carries anything, BEFORE that.
//!
//! Method: analyse a donor signal into real `SoundDNA`, feed it to the processor,
//! run a different recipient signal through, and compare the output against both
//! inputs — level, spectral centroid, and per-band energy. Transfer that works
//! should move the recipient's spectrum toward the donor's while preserving its
//! level and identity. Run:
//!
//!   cargo run --release -p nullherz-conductor --example spike_dna_transfer

use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor, Transport};
use std::sync::Arc;

const SR: f32 = 48_000.0;
const FFT: usize = 1024;

/// Harmonic stack — stands in for a tonal source (recipient).
fn tonal(frames: usize, f0: f32) -> Vec<f32> {
    (0..frames)
        .map(|i| {
            let t = i as f32 / SR;
            (1..=6)
                .map(|h| (t * f0 * h as f32 * std::f32::consts::TAU).sin() * 0.4 / h as f32)
                .sum()
        })
        .collect()
}

/// Bright, noisy, high-tilted source — stands in for a very differently-coloured
/// donor. If transfer works at all, THIS is what should audibly bleed into the
/// recipient.
fn bright_noise(frames: usize) -> Vec<f32> {
    let mut s = 0u32;
    (0..frames)
        .map(|i| {
            s = s.wrapping_mul(1103515245).wrapping_add(12345);
            let n = (s >> 8) as f32 / 8388608.0 - 1.0;
            // Tilt the noise upward so its band profile is unmistakably different.
            let t = i as f32 / SR;
            n * 0.3 + (t * 9000.0 * std::f32::consts::TAU).sin() * 0.3
        })
        .collect()
}

/// Steady-state window, past the processor's FFT latency.
///
/// `latency_samples()` reports `fft.size` (1024) — but the REAL latency measures
/// **48267 samples**, because the rhythmic delay line reads `delay_buffer[write_ptr]`
/// BEFORE writing the current sample there, so the delay is one whole buffer
/// (1 second at 48 kHz) even when `rhythmic_bias` is 0. Analysing before that
/// point reads silence, which made the first version of this spike "prove" the
/// processor did nothing at all.
const ANALYSIS_OFFSET: usize = 60_000;

fn steady(x: &[f32]) -> &[f32] {
    &x[ANALYSIS_OFFSET.min(x.len())..]
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() { return 0.0; }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-12).log10()
}

/// 16-band energy profile, normalised to sum 1 — the same shape `latent_space`
/// claims to describe, so donor / recipient / output are directly comparable.
fn band_profile(x: &[f32]) -> [f32; 16] {
    let mut re = vec![0.0f32; FFT];
    let mut im = vec![0.0f32; FFT];
    let n = x.len().min(FFT);
    re[..n].copy_from_slice(&x[..n]);
    let fft = audio_dsp::SimdFft::new(FFT);
    fft.process(&mut re, &mut im);

    let mut mags = [0.0f32; 512];
    let mut total = 0.0;
    for b in 0..512 {
        mags[b] = (re[b] * re[b] + im[b] * im[b]).sqrt();
        total += mags[b];
    }
    let mut out = [0.0f32; 16];
    for i in 0..16 {
        let sum: f32 = (0..32).map(|k| mags[i * 32 + k]).sum();
        out[i] = if total > 0.0 { sum / total } else { 0.0 };
    }
    out
}

/// Spectral centroid in band units (0..15) — one number for "how bright".
fn centroid(p: &[f32; 16]) -> f32 {
    let w: f32 = p.iter().sum();
    if w <= 0.0 { return 0.0; }
    p.iter().enumerate().map(|(i, v)| i as f32 * v).sum::<f32>() / w
}

/// Euclidean distance between two normalised band profiles.
fn dist(a: &[f32; 16], b: &[f32; 16]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f32>().sqrt()
}

fn render(donor_dna: &nullherz_traits::SoundDNA, recipient: &[f32], bias: f32) -> Vec<f32> {
    let mut proc = nullherz_processors::transfusion::PersonalityInheritanceProcessor::new(0, FFT, SR);

    // The wiring 6.1 calls for: push real donor DNA in through the metadata path.
    let mut meta = nullherz_traits::SampleMetadata::new_empty();
    meta.dna = donor_dna.clone();
    proc.apply_topology_mutation(nullherz_traits::TopologyMutation::UpdateMetadata {
        node_idx: 0,
        metadata: Arc::new(meta),
    });

    // Spectral bias only. Rhythmic/artifact/spatial are held at 0 so this
    // measures the SPECTRAL transfer alone rather than a sum of four effects.
    proc.set_parameter(0, bias, 0);
    proc.set_parameter(1, 0.0, 0);
    proc.set_parameter(2, 0.0, 0);
    proc.set_parameter(3, 0.0, 0);

    let transport = Transport {
        bpm: 120.0,
        beat_position: 0.0,
        is_playing: true,
        sample_rate: SR,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    };

    let mut out = vec![0.0f32; recipient.len()];
    let block = 256;
    for start in (0..recipient.len()).step_by(block) {
        let end = (start + block).min(recipient.len());
        let inp: [&[f32]; 1] = [&recipient[start..end]];
        let mut o = out[start..end].to_vec();
        {
            let mut slice: [&mut [f32]; 1] = [&mut o];
            let mut ctx = ProcessContext {
                transport: Some(&transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: end == recipient.len(),
            };
            proc.process(&inp, &mut slice, &mut ctx);
        }
        out[start..end].copy_from_slice(&o);
    }
    out
}

/// The transfer the processor *should* be doing (roadmap 6.5), prototyped here
/// rather than in the processor so the spike answers the question without
/// shipping a change.
///
/// The shipped version computes `target_mag / current_mag` where `target_mag` is
/// `latent_space[i]` — a DIMENSIONLESS band-energy RATIO — and `current_mag` is
/// an ABSOLUTE FFT bin magnitude. Those units are incommensurable, so the result
/// depends on the recipient's absolute level and the FFT scaling rather than on
/// the donor at all. At full bias it drives every bin in a band to the same
/// absolute value (~0.6), which is why level collapses.
///
/// This version compares like with like: the donor's band SHARE against the
/// recipient's own band SHARE in the same frame. That is level-independent by
/// construction, so it reshapes the spectrum without gating it.
fn render_relative(donor_dna: &nullherz_traits::SoundDNA, recipient: &[f32], bias: f32) -> Vec<f32> {
    let mut pipeline = audio_dsp::SpectralPipeline::new(FFT);
    let donor = donor_dna.spectral.latent_space;
    // Normalise the donor profile to a share, so it is a distribution rather
    // than 16 arbitrary clamped numbers.
    let donor_sum: f32 = donor.iter().sum::<f32>().max(1e-9);

    let mut out = vec![0.0f32; recipient.len()];
    let block = 256;
    for start in (0..recipient.len()).step_by(block) {
        let end = (start + block).min(recipient.len());
        let mut o = vec![0.0f32; end - start];
        pipeline.process(&recipient[start..end], &mut o, |re, im, n, _w, _f| {
            let per = n / 2 / 16;
            if per == 0 { return; }

            // Recipient's own band shares for THIS frame.
            let mut cur = [0.0f32; 16];
            let mut total = 0.0f32;
            for i in 0..16 {
                let mut s = 0.0;
                for b in i * per..(i + 1) * per {
                    s += (re[b] * re[b] + im[b] * im[b]).sqrt();
                }
                cur[i] = s;
                total += s;
            }
            if total <= 1e-9 { return; }

            // Transfer the SHAPE, preserve the GAIN.
            //
            // Share-matching alone is inherently lossy between dissimilar
            // sources: making a tonal recipient match broadband noise means
            // cutting its fundamental ~22 dB and boosting highs that hold almost
            // no energy, so the sum collapses (measured: −33.8 dBFS at full
            // bias). A spectral morph should re-colour without re-levelling —
            // level is the recipient's identity, not the donor's. Renormalising
            // to the frame's original energy separates the two.
            let mut after = 0.0f32;
            for i in 0..16 {
                let want = donor[i] / donor_sum;          // donor's share of energy
                let have = (cur[i] / total).max(1e-9);    // recipient's share, same frame
                // Ratio of shares — dimensionless on both sides.
                let ratio = (want / have).clamp(0.03, 32.0);
                // Interpolate in the LOG domain: `ratio^bias`, not
                // `1 + (ratio-1)*bias`. A band needing a 22 dB cut gets 11 dB at
                // half bias instead of 5.4 dB, so the control moves the sound
                // evenly across its range. Linear-in-ratio measured as doing
                // almost nothing until 0.75 and then jumping — an unusable feel
                // for what is meant to be a performance control.
                let scale = ratio.powf(bias);
                for b in i * per..(i + 1) * per {
                    re[b] *= scale;
                    im[b] *= scale;
                    // MIRROR the conjugate half. The input is real, so bins
                    // n-b mirror bins b; scaling only the positive half breaks
                    // that symmetry, which both halves the intended effect and
                    // leaks energy (the inverse transform averages a scaled
                    // half against an unscaled one). The shipped processor does
                    // not do this — it has a comment wondering whether it needs
                    // to, and the answer is yes.
                    if b > 0 && b < n / 2 {
                        re[n - b] *= scale;
                        im[n - b] *= scale;
                    }
                }
                after += cur[i] * scale;
            }
            // Restore the frame's original energy.
            if after > 1e-9 {
                let g = total / after;
                for b in 0..n {
                    re[b] *= g;
                    im[b] *= g;
                }
            }
        });
        out[start..end].copy_from_slice(&o);
    }
    out
}

/// **The architecture question: can DNA transfer be done WITHOUT an FFT?**
///
/// `latent_space` is a 16-band energy profile. Imposing one spectrum's band
/// profile onto another is an EQ curve — and an EQ curve is a filter, not a
/// resynthesis. Filters have no window, therefore no block latency.
///
/// The console already proves this is viable: `DjIsolator` is a 3-band
/// Linkwitz-Riley multiband sitting in every deck chain at **zero** declared
/// latency. This is the same operation at 16 bands.
///
/// The split that makes it work:
///   * **Analysis off-path.** The recipient's own band profile is measured on a
///     tap, which may lag by a whole window without delaying anything, because
///     it is not in the signal path (Phase 5.3 taps are exactly this).
///   * **Filter in-path.** Only the correction is applied to the audio, as a
///     cascade of peaking biquads. Coefficients update at control rate.
///
/// This models that: profile measured once up front (standing in for the tap),
/// gains applied as a static biquad cascade.
fn render_filterbank(donor_dna: &nullherz_traits::SoundDNA, recipient: &[f32], bias: f32) -> Vec<f32> {
    let donor = donor_dna.spectral.latent_space;
    let donor_sum: f32 = donor.iter().sum::<f32>().max(1e-9);
    let have = band_profile(steady(recipient)); // the off-path analysis tap

    let bin_hz = SR / FFT as f32;
    let mut filters: Vec<audio_dsp::BiquadFilter> = Vec::new();
    for i in 0..16 {
        let want = donor[i] / donor_sum;
        let cur = have[i].max(1e-9);
        // Same log-domain interpolation as the spectral version, so the two are
        // compared on equal terms rather than on a different taper.
        let gain = (want / cur).clamp(0.03, 32.0).powf(bias);
        // Band i spans 32 bins; centre it and set Q from its width.
        let centre = (i as f32 * 32.0 + 16.0) * bin_hz;
        let bandwidth = 32.0 * bin_hz;
        if centre >= SR * 0.5 { continue; }
        let q = (centre / bandwidth).max(0.3);
        filters.push(audio_dsp::BiquadFilter::new(
            audio_dsp::BiquadCoefficients::peaking(centre, q, gain, SR),
        ));
    }

    let mut buf = recipient.to_vec();
    let mut scratch = vec![0.0f32; buf.len()];
    for f in filters.iter_mut() {
        f.process_block_unrolled(&buf, &mut scratch);
        buf.copy_from_slice(&scratch);
    }
    buf
}

/// The CORRECT zero-latency topology: a Linkwitz-Riley crossover bank.
///
/// The cascaded-peaking version above does not work, and the reason matters.
/// Peaking bells have skirts that overlap their neighbours, so a cascade of 16
/// of them does not produce the target curve — hitting a specified magnitude
/// response that way needs the gains solved as a system, not set band by band.
/// Measured: it moved the profile the WRONG way at low bias and cost 10 dB.
///
/// LR crossovers are complementary — a matched LP/HP pair sums flat — so at
/// THREE bands this works, which is what `DjIsolator` does in every deck at zero
/// latency today.
///
/// **NEGATIVE RESULT: it does not extend naively to 16.** This is a SERIAL split
/// (band i taken from the remainder after i crossovers), and phase accumulates
/// down the cascade: LR sums to allpass, not to unity, so by band 15 the lower
/// bands have accumulated phase the sum cannot reconcile. Measured: not even
/// transparent at bias 0 (centroid 0.95 against the recipient's 0.48), and it
/// moves the profile the WRONG way as bias rises.
///
/// Correcting it needs allpass compensation on every lower band — standard,
/// well-documented, and real design work rather than a swap. Kept here as the
/// negative result: "just use filters" is the right DIRECTION and not a free
/// substitution.
fn render_lr_bank(donor_dna: &nullherz_traits::SoundDNA, recipient: &[f32], bias: f32) -> Vec<f32> {
    let donor = donor_dna.spectral.latent_space;
    let donor_sum: f32 = donor.iter().sum::<f32>().max(1e-9);
    let have = band_profile(steady(recipient)); // off-path analysis tap
    let bin_hz = SR / FFT as f32;

    let mut out = vec![0.0f32; recipient.len()];
    let mut rest = recipient.to_vec();
    let mut band = vec![0.0f32; recipient.len()];
    let mut next = vec![0.0f32; recipient.len()];

    for i in 0..16 {
        let want = donor[i] / donor_sum;
        let gain = (want / have[i].max(1e-9)).clamp(0.03, 32.0).powf(bias);
        let split = ((i + 1) as f32 * 32.0) * bin_hz;

        if i == 15 || split >= SR * 0.5 * 0.98 {
            for (o, r) in out.iter_mut().zip(rest.iter()) { *o += r * gain; }
            break;
        }
        // LR is 2 cascaded Butterworth sections per side.
        let mut lp1 = audio_dsp::BiquadFilter::new(audio_dsp::BiquadCoefficients::linkwitz_riley_lp(split, SR));
        let mut lp2 = audio_dsp::BiquadFilter::new(audio_dsp::BiquadCoefficients::linkwitz_riley_lp(split, SR));
        let mut hp1 = audio_dsp::BiquadFilter::new(audio_dsp::BiquadCoefficients::linkwitz_riley_hp(split, SR));
        let mut hp2 = audio_dsp::BiquadFilter::new(audio_dsp::BiquadCoefficients::linkwitz_riley_hp(split, SR));

        lp1.process_block_unrolled(&rest, &mut band);
        lp2.process_block_unrolled(&band.clone(), &mut band);
        hp1.process_block_unrolled(&rest, &mut next);
        hp2.process_block_unrolled(&next.clone(), &mut next);

        for (o, b) in out.iter_mut().zip(band.iter()) { *o += b * gain; }
        rest.copy_from_slice(&next);
    }
    out
}

fn main() {
    let frames = (SR as usize) * 2;
    let recipient = tonal(frames, 220.0);
    let donor = bright_noise(frames);

    let mut kernel = nullherz_conductor::analysis_kernel::AnalysisKernel::new(SR);
    let (_, donor_dna) = kernel.analyze(&donor);
    let (_, recip_dna) = kernel.analyze(&recipient);

    let p_recip = band_profile(steady(&recipient));
    let p_donor = band_profile(steady(&donor));

    println!("=== inputs ===");
    println!("recipient  rms {:>7.1} dBFS   centroid {:>5.2}", db(rms(steady(&recipient))), centroid(&p_recip));
    println!("donor      rms {:>7.1} dBFS   centroid {:>5.2}", db(rms(steady(&donor))), centroid(&p_donor));
    println!("donor/recipient band distance: {:.4}  (how different they are to begin with)", dist(&p_recip, &p_donor));

    println!("\n=== donor DNA actually handed to the processor ===");
    print!("latent_space:");
    for v in donor_dna.spectral.latent_space { print!(" {v:.3}"); }
    println!();
    println!("recipient latent_space (for comparison):");
    print!("            ");
    for v in recip_dna.spectral.latent_space { print!(" {v:.3}"); }
    println!();
    println!("NOTE: these are DIMENSIONLESS energy RATIOS (band share x10, clamped to 1),");
    println!("      not magnitudes. analysis_kernel.rs:247.");

    // How faithfully can latent_space represent a spectrum AT ALL?
    // It is `(band_share * 10).min(1.0)` — the clamp saturates any band holding
    // more than 10% of the energy, and the encoding is all a transfer has to aim at.
    let mut implied = [0.0f32; 16];
    let ls_sum: f32 = donor_dna.spectral.latent_space.iter().sum::<f32>().max(1e-9);
    for i in 0..16 { implied[i] = donor_dna.spectral.latent_space[i] / ls_sum; }
    let clamped_donor = donor_dna.spectral.latent_space.iter().filter(|v| **v >= 0.999).count();
    let mut implied_r = [0.0f32; 16];
    let rs_sum: f32 = recip_dna.spectral.latent_space.iter().sum::<f32>().max(1e-9);
    for i in 0..16 { implied_r[i] = recip_dna.spectral.latent_space[i] / rs_sum; }
    let clamped_recip = recip_dna.spectral.latent_space.iter().filter(|v| **v >= 0.999).count();
    println!("\n=== the ENCODING ceiling ===");
    println!("donor: true profile vs the profile latent_space implies -> distance {:.4}  ({} of 16 bands clamped)",
        dist(&implied, &p_donor), clamped_donor);
    println!("recip: true profile vs the profile latent_space implies -> distance {:.4}  ({} of 16 bands clamped)",
        dist(&implied_r, &p_recip), clamped_recip);
    println!("A perfect transfer can only ever reach the IMPLIED profile, so this distance");
    println!("is a floor on d(out,donor) that no amount of DSP work can cross.");

    println!("\n=== transfer sweep (spectral bias only) ===");
    println!("{:>5}  {:>10}  {:>9}  {:>12}  {:>12}", "bias", "rms dBFS", "centroid", "d(out,recip)", "d(out,donor)");
    for bias in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let out = render(&donor_dna, &recipient, bias);
        let p = band_profile(steady(&out));
        println!(
            "{:>5.2}  {:>10.1}  {:>9.2}  {:>12.4}  {:>12.4}",
            bias, db(rms(steady(&out))), centroid(&p), dist(&p, &p_recip), dist(&p, &p_donor)
        );
    }

    println!("\n=== SAME sweep with corrected (relative) transfer — roadmap 6.5 ===");
    println!("{:>5}  {:>10}  {:>9}  {:>12}  {:>12}", "bias", "rms dBFS", "centroid", "d(out,recip)", "d(out,donor)");
    for bias in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let out = render_relative(&donor_dna, &recipient, bias);
        let p = band_profile(steady(&out));
        println!(
            "{:>5.2}  {:>10.1}  {:>9.2}  {:>12.4}  {:>12.4}",
            bias, db(rms(steady(&out))), centroid(&p), dist(&p, &p_recip), dist(&p, &p_donor)
        );
    }

    println!("\n=== SAME sweep as a ZERO-LATENCY BIQUAD BANK (no FFT at all) ===");
    println!("{:>5}  {:>10}  {:>9}  {:>12}  {:>12}", "bias", "rms dBFS", "centroid", "d(out,recip)", "d(out,donor)");
    for bias in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let out = render_filterbank(&donor_dna, &recipient, bias);
        let p = band_profile(steady(&out));
        println!(
            "{:>5.2}  {:>10.1}  {:>9.2}  {:>12.4}  {:>12.4}",
            bias, db(rms(steady(&out))), centroid(&p), dist(&p, &p_recip), dist(&p, &p_donor)
        );
    }
    println!("  ^ naive cascaded peaking bells — does NOT reproduce the target curve");

    println!("\n=== SAME sweep as a LINKWITZ-RILEY BANK (correct zero-latency topology) ===");
    println!("{:>5}  {:>10}  {:>9}  {:>12}  {:>12}", "bias", "rms dBFS", "centroid", "d(out,recip)", "d(out,donor)");
    for bias in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let out = render_lr_bank(&donor_dna, &recipient, bias);
        let p = band_profile(steady(&out));
        println!(
            "{:>5.2}  {:>10.1}  {:>9.2}  {:>12.4}  {:>12.4}",
            bias, db(rms(steady(&out))), centroid(&p), dist(&p, &p_recip), dist(&p, &p_donor)
        );
    }
    println!("  ^ NEGATIVE RESULT: serial LR split needs allpass compensation per band;");
    println!("    zero latency is achieved but the target curve is not. See fn docs.");

    println!("\nReading it: transfer that WORKS should move d(out,donor) DOWN as bias rises");
    println!("(output takes on the donor's colour) while rms stays near the recipient's.");
    println!("Level collapse or an unchanged donor distance means the transfer is not");
    println!("carrying the donor's character.");
}
