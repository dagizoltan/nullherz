//! Stimulus generation and distortion analysis for fidelity measurement.
//!
//! A distortion analyser is only as clean as the tone it grades with, and this
//! module exists because that was not true here. `probe_signal_quality`
//! reported the console's THD+N at unity as **0.046%** (-66.7 dB) — 40 dB above
//! where a gain-and-sum path should sit — and the entire figure came from how
//! the test tone was generated.
//!
//! The natural spelling of a sine table is:
//!
//! ```ignore
//! let s = (i as f32 * freq * TAU / sample_rate).sin();
//! ```
//!
//! The argument grows without bound with `i`, and an f32 ULP grows with it. By
//! sample 65_536 at 997 Hz the argument is ~8_500 rad, where one ULP is ~5e-4
//! rad; the rounded argument yields a sine whose phase jitters by that much
//! from sample to sample. Phase noise is broadband, and it scales with the
//! signal — so it presents as a level-INDEPENDENT residue, which is exactly the
//! evidence that rules out the saturating non-linearities one naturally
//! suspects first. It also grows with playback position:
//!
//! | analysis window starts at | THD+N of the raw tone |
//! |---------------------------|-----------------------|
//! | sample 0                  | 0.0060%               |
//! | sample 16_384             | 0.0155%               |
//! | sample 65_536             | 0.0460%               |
//! | sample 131_072            | 0.0911%               |
//!
//! (Those figures were taken with the Hann window this module originally used,
//! whose 0.0046% floor is the "0.0060% at sample 0" row — at sample 0 the
//! generator was already the same order as the instrument, which is why the
//! trap was invisible until the settling depth grew.)
//!
//! The probe validated its analyser against a tone measured from sample 0, but
//! graded the console on samples 65_536 onward — 256 blocks of settling in. It
//! compared two different signals and charged the difference to the mixer.
//!
//! [`tone_sample`] accumulates phase in f64 and wraps it into `[0, TAU)` before
//! it ever reaches `sin`, so the argument stays small and every mantissa bit
//! stays meaningful. The result is flat at the instrument floor at every offset
//! in that table.
//!
//! # The window
//!
//! The analyser uses a 7-term Blackman-Harris window with a +/-8-bin
//! fundamental exclusion. It previously used Hann with +/-24 bins, which put
//! the floor at -86.8 dB and could not resolve the console at all — every node
//! measured "at the floor" because the floor WAS the measurement. Hann's -31 dB
//! sidelobes had to be bought off with exclusion width, which is the same move
//! as widening a window until a residue disappears, just frozen into a
//! constant. BH7's sidelobes are below -180 dB, so the exclusion NARROWS to
//! +/-8 and the floor drops to -134.7 dB. See [`analyser_floor`].

use crate::SimdFft;

/// 7-term Blackman-Harris cosine-sum coefficients (Nuttall/Rife-Vincent).
///
/// Sidelobes below -180 dB, versus Hann's -31 dB. The window governs how much
/// of the fundamental's own energy leaks into the bins we are calling
/// "distortion", so it — not the console — set the previous measurement floor.
const BH7: [f64; 7] = [
    0.271_051_400_693_42,
    0.433_297_939_234_48,
    0.218_122_999_543_11,
    0.065_925_446_388_03,
    0.010_811_742_098_37,
    0.000_776_584_825_22,
    0.000_013_887_217_35,
];

/// Half-width, in bins, of the fundamental's main lobe.
///
/// A cosine-sum window of `k` terms has a main lobe `2k` bins wide, so `k` is
/// the half-width; the +1 is margin because 997 Hz does not sit on a bin
/// centre.
///
/// Deliberately NOT a tunable. The window is what separates "the signal" from
/// "everything else"; widening it until a residue disappears is how a
/// distortion measurement is made to say whatever you want. The previous
/// analyser used Hann and needed +/-24 bins to reach a 0.0046% floor — it was
/// buying sidelobe rejection with exclusion width, which is that same move in
/// slow motion. BH7 reaches a far lower floor at +/-8, so this is both a
/// NARROWER claim about what counts as signal and a quieter instrument.
/// Re-validate against [`analyser_floor`] if the FFT size or window changes.
const FUND_BINS: usize = BH7.len() + 1;

/// Sample `i` of a `freq` Hz sine at `amp`, with phase wrapped into `[0, TAU)`.
///
/// Use this for ANY signal a measurement will grade. Position-independent by
/// construction: the spectrum of `[n, n+N)` is the same for every `n`, which is
/// what makes a control taken at one offset valid for a measurement taken at
/// another. See the module docs for what happens when it is not.
#[inline]
pub fn tone_sample(i: usize, freq: f32, sample_rate: f32, amp: f32) -> f32 {
    let phase = (i as f64 * freq as f64 * std::f64::consts::TAU / sample_rate as f64)
        .rem_euclid(std::f64::consts::TAU);
    (phase.sin() as f32) * amp
}

/// `len` samples of [`tone_sample`], starting at sample `start`.
pub fn tone(start: usize, len: usize, freq: f32, sample_rate: f32, amp: f32) -> Vec<f32> {
    (start..start + len).map(|i| tone_sample(i, freq, sample_rate, amp)).collect()
}

/// THD+N of `x` about `freq`: the ratio of all energy OUTSIDE the fundamental's
/// main lobe to the energy inside it, as a linear ratio (0.001 = 0.1%).
///
/// `fft_size` must be a power of two; only the first `fft_size` samples of `x`
/// are read. Allocates — this is an offline measurement tool, never an
/// audio-path one.
///
/// Read the result against [`analyser_floor`], never against zero.
pub fn thd_n(x: &[f32], freq: f32, sample_rate: f32, fft_size: usize) -> f32 {
    let mags = spectrum(x, fft_size);
    let fund_bin = bin_of(freq, sample_rate, fft_size);
    // Accumulate the residual DIRECTLY rather than as `total - fund`. The
    // fundamental can be 15+ orders of magnitude larger than the residue we are
    // trying to resolve, and that subtraction cancels the answer away: with a
    // low-leakage window it returns a confident 0.0 — an analyser reporting
    // "perfect" because it ran out of mantissa. Same class of mistake as the
    // one this module exists to document, one level down.
    let mut fund = 0.0f64;
    let mut rest = 0.0f64;
    for (b, m) in mags.iter().enumerate() {
        let e = (*m as f64) * (*m as f64);
        if b.abs_diff(fund_bin) <= FUND_BINS { fund += e } else { rest += e }
    }
    if fund <= 0.0 { return 0.0; }
    (rest / fund).sqrt() as f32
}

/// The analyser's window at sample `i` of `n`. f64 because at -180 dB sidelobes
/// the window's own coefficients must not be what quantises the result.
#[inline]
fn window(i: usize, n: usize) -> f64 {
    let t = std::f64::consts::TAU * i as f64 / n as f64;
    BH7.iter().enumerate()
        .map(|(k, c)| {
            let term = c * (k as f64 * t).cos();
            if k % 2 == 1 { -term } else { term }
        })
        .sum()
}

/// Magnitude spectrum of `x` under the analyser's window: `fft_size / 2` bins,
/// bin `b` centred on `b * sample_rate / fft_size` Hz.
///
/// Exposed so that callers needing bin-level detail — a level reading at the
/// fundamental, or hunting for an alias at a predicted frequency — use the SAME
/// window as [`thd_n`]. They previously hand-rolled a Hann-windowed FFT, so a
/// probe's level column and its THD column came off two different instruments.
pub fn spectrum(x: &[f32], fft_size: usize) -> Vec<f32> {
    let n = fft_size.min(x.len());
    let mut re = vec![0.0f32; fft_size];
    let mut im = vec![0.0f32; fft_size];
    for i in 0..n {
        re[i] = (x[i] as f64 * window(i, fft_size)) as f32;
    }
    SimdFft::new(fft_size).process(&mut re, &mut im);
    (0..fft_size / 2).map(|b| (re[b] * re[b] + im[b] * im[b]).sqrt()).collect()
}

/// Bin holding `freq`. Pair with [`spectrum`].
#[inline]
pub fn bin_of(freq: f32, sample_rate: f32, fft_size: usize) -> usize {
    (freq / sample_rate * fft_size as f32).round() as usize
}

/// Level at `freq`, in dB relative to full scale, read off [`spectrum`].
///
/// Sums the fundamental's whole main lobe rather than peeking at one bin: under
/// a 7-term window a tone that does not sit on a bin centre spreads across
/// several, and reading only the peak bin under-reads it by a rate- and
/// frequency-dependent amount — which would look exactly like the interpolator
/// droop a resampler probe is trying to measure.
pub fn level_db_at(mags: &[f32], freq: f32, sample_rate: f32, fft_size: usize) -> f32 {
    let b0 = bin_of(freq, sample_rate, fft_size);
    let lo = b0.saturating_sub(FUND_BINS);
    let hi = (b0 + FUND_BINS).min(mags.len().saturating_sub(1));
    let e: f64 = (lo..=hi).map(|b| (mags[b] as f64) * (mags[b] as f64)).sum();
    20.0 * (e.sqrt().max(1e-30) as f32).log10()
}

/// What [`thd_n`] reports on an ideal tone: the instrument's own noise floor.
///
/// Currently **-134.7 dB** (0.0000183%), and it is set by the f32 arithmetic of
/// [`SimdFft`], not by window leakage — it is flat across FFT sizes, which is
/// how you tell those two apart. An f64 FFT would reach about -153 dB, the
/// point at which f32 sample storage takes over; that is unnecessary while the
/// quietest thing being measured (the console, at -107 dB) sits 28 dB above.
///
/// Every measured number is read AGAINST this. A reading at or near it is
/// measurement, not distortion — there is no signal below the instrument's own
/// noise floor, and treating one as real is how the 0.046% survived.
pub fn analyser_floor(freq: f32, sample_rate: f32, fft_size: usize) -> f32 {
    thd_n(&tone(0, fft_size, freq, sample_rate, 0.5), freq, sample_rate, fft_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;
    const FFT: usize = 16_384;
    /// 997 Hz: deliberately not a bin-centre multiple, so the FFT is not
    /// flattered by a tone that lands exactly on a bin.
    const TONE: f32 = 997.0;

    /// The analyser floor has to stay where the window was chosen to put it. If
    /// this moves, every THD number in the repo — and the tolerance in the
    /// console transparency test — is being read against a different ruler.
    ///
    /// -134.7 dB measured; asserted at -126 dB to leave room for FFT arithmetic
    /// differing across CPUs. Reverting the window to Hann reports 4.6e-5
    /// (-86.8 dB) and fails this by 250x.
    #[test]
    fn test_analyser_floor_is_where_the_window_was_tuned_for() {
        let floor = analyser_floor(TONE, SR, FFT);
        assert!(
            floor < 5e-7,
            "analyser floor drifted to {:.7}% ({:.1} dB) — the window, the \
             fundamental exclusion or the FFT changed, and every THD figure now \
             reads against a different ruler; re-validate before trusting any \
             of them",
            floor * 100.0,
            20.0 * floor.max(1e-20).log10()
        );
    }

    /// The floor must be set by ARITHMETIC, not by leakage.
    ///
    /// Window leakage is a fixed fraction of the fundamental and does not care
    /// how long the transform is; f32 FFT round-off grows only as
    /// sqrt(log2 N). A floor that is flat across FFT sizes is therefore the
    /// arithmetic one, and that is the state we want — leakage-limited means
    /// the window is still the thing being measured.
    #[test]
    fn test_floor_is_arithmetic_limited_not_leakage_limited() {
        let small = analyser_floor(TONE, SR, 4_096);
        let large = analyser_floor(TONE, SR, 65_536);
        let ratio = (small / large).max(large / small);
        assert!(
            ratio < 2.0,
            "analyser floor moved {ratio:.1}x between a 4k and a 64k transform \
             ({:.7}% vs {:.7}%) — it is LEAKAGE limited, so the window is still \
             the instrument's weakest part and node measurements near the floor \
             are measuring it, not the node",
            small * 100.0, large * 100.0
        );
    }

    /// THE REGRESSION GUARD for the 0.046%.
    ///
    /// The measurement tone must be spectrally IDENTICAL wherever it is sampled
    /// from, because the control is taken at one offset and the console is
    /// graded at another. Reverting `tone_sample` to the natural
    /// `(i as f32 * freq * TAU / sr).sin()` fails this at the 65_536 offset
    /// with 0.046% against a 0.0046% floor — a 10x miss, and the exact figure
    /// that was misattributed to the mixer.
    #[test]
    fn test_tone_is_spectrally_position_independent() {
        let floor = analyser_floor(TONE, SR, FFT);
        // Offsets spanning the settling depth a console probe actually uses
        // (256 blocks of 256 = 65_536) and well past it.
        for start in [0usize, 16_384, 65_536, 131_072, 480_000] {
            let t = thd_n(&tone(start, FFT, TONE, SR, 0.5), TONE, SR, FFT);
            assert!(
                t < floor * 1.5,
                "tone sampled from {start} measures {:.5}% THD+N against a \
                 {:.5}% analyser floor. The stimulus is dirtier than the \
                 instrument, so anything graded with it inherits the \
                 difference as phantom distortion — this is the 0.046% \
                 console-THD bug. Phase must be accumulated in f64 and wrapped \
                 into [0, TAU) before sin().",
                t * 100.0, floor * 100.0
            );
        }
    }

    /// A sanity check in the other direction: the analyser must still SEE
    /// distortion when it is really there. A test that only ever asserts
    /// "clean" passes just as happily on an analyser that returns zero.
    #[test]
    fn test_analyser_detects_real_distortion() {
        // tanh at this drive is the waveshaper that used to sit on every
        // summing node; it measured 1.23% through the console at -6 dBFS.
        let dirty: Vec<f32> = tone(0, FFT, TONE, SR, 0.9).iter().map(|s| s.tanh()).collect();
        let t = thd_n(&dirty, TONE, SR, FFT);
        assert!(
            t > 0.01,
            "analyser reported only {:.5}% on a deliberately tanh-shaped tone — \
             it has gone blind and every 'clean' result it produces is worthless",
            t * 100.0
        );
    }
}
