use crate::util::AlignedBuffer;
use crate::SimdFft;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpectralWindowShape {
    Hann = 0,
    Hamming = 1,
    Blackman = 2,
    Rectangular = 3,
}

/// A reusable Spectral Pipeline handling FFT, Windowing, and Overlap-Add.
pub struct SpectralPipeline {
    pub fft: SimdFft,
    pub(crate) in_buffer: AlignedBuffer,
    pub(crate) out_buffer: AlignedBuffer,
    pub(crate) scratch_re: AlignedBuffer,
    pub(crate) scratch_im: AlignedBuffer,
    pub window: AlignedBuffer,
    /// Synthesis window: the analysis window divided by the overlap-add (COLA)
    /// sum at each frame-local position. Windowing on BOTH analysis and
    /// synthesis means the signal is multiplied by `window[i]^2`, whose
    /// overlapping sum is not unity for any of the shapes here — for Hann at
    /// 50% overlap it oscillates between 0.5 and 1.0, and for Rectangular it
    /// is a flat 2.0. Pre-dividing by that sum makes reconstruction exact for
    /// any window shape and any hop size, at no hot-path cost.
    pub synth_window: AlignedBuffer,
    pub hop_size: usize,
    pub(crate) in_ptr: usize,
    pub(crate) out_ptr: usize,
    pub(crate) out_mask: usize,
    pub(crate) window_shape: SpectralWindowShape,
}

impl SpectralPipeline {
    pub fn new(fft_size: usize) -> Self {
        assert!(fft_size.is_power_of_two());
        let mut pipeline = Self {
            fft: SimdFft::new(fft_size),
            in_buffer: AlignedBuffer::new(fft_size),
            out_buffer: AlignedBuffer::new((fft_size + fft_size / 2).next_power_of_two()),
            scratch_re: AlignedBuffer::new(fft_size),
            scratch_im: AlignedBuffer::new(fft_size),
            window: AlignedBuffer::new(fft_size),
            synth_window: AlignedBuffer::new(fft_size),
            hop_size: fft_size / 2,
            in_ptr: 0,
            out_ptr: 0,
            out_mask: (fft_size + fft_size / 2).next_power_of_two() - 1,
            window_shape: SpectralWindowShape::Hann,
        };
        pipeline.update_window(SpectralWindowShape::Hann);
        pipeline
    }

    pub fn update_window(&mut self, shape: SpectralWindowShape) {
        self.window_shape = shape;
        let n = self.fft.size;
        match shape {
            SpectralWindowShape::Hann => {
                for i in 0..n {
                    self.window[i] = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos());
                }
            }
            SpectralWindowShape::Hamming => {
                for i in 0..n {
                    self.window[i] = 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos();
                }
            }
            SpectralWindowShape::Blackman => {
                for i in 0..n {
                    self.window[i] = 0.42 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos()
                        + 0.08 * (4.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos();
                }
            }
            SpectralWindowShape::Rectangular => {
                self.window.fill(1.0);
            }
        }
        self.rebuild_synthesis_window();
    }

    /// Derive the synthesis window from the analysis window so that
    /// analysis * synthesis sums to exactly 1.0 across overlapping frames.
    ///
    /// The overlap-add sum at absolute output position `p` gathers every frame
    /// whose local index is congruent to `p` modulo the hop size, so the sum
    /// `S[j] = sum_m window[j + m*hop]^2` depends only on `j = p % hop`.
    /// Dividing the synthesis window by `S[i % hop]` therefore makes the total
    /// contribution at every position exactly 1, for any shape or hop size.
    fn rebuild_synthesis_window(&mut self) {
        let n = self.fft.size;
        let hop = self.hop_size.max(1);

        let mut cola = vec![0.0f32; hop];
        for (j, slot) in cola.iter_mut().enumerate() {
            let mut sum = 0.0f32;
            let mut k = j;
            while k < n {
                sum += self.window[k] * self.window[k];
                k += hop;
            }
            *slot = sum;
        }

        for i in 0..n {
            // A window with a true zero in its COLA sum cannot be normalized;
            // leaving those positions unscaled is better than emitting NaN.
            let denom = cola[i % hop];
            self.synth_window[i] = if denom > 1e-6 { self.window[i] / denom } else { self.window[i] };
        }
    }

    pub fn process<F>(&mut self, input: &[f32], output: &mut [f32], mut spectral_op: F)
    where F: FnMut(&mut [f32], &mut [f32], usize, &AlignedBuffer, &SimdFft) {
        let len = input.len();
        let mask = self.out_mask;
        for i in 0..len {
            self.in_buffer[self.in_ptr] = input[i];
            output[i] = self.out_buffer[self.out_ptr];
            self.out_buffer[self.out_ptr] = 0.0;

            self.in_ptr += 1;
            self.out_ptr = (self.out_ptr + 1) & mask;

            if self.in_ptr >= self.fft.size {
                self.execute_block(&mut spectral_op);
                self.in_buffer.copy_within(self.hop_size..self.fft.size, 0);
                self.in_ptr = self.fft.size - self.hop_size;
            }
        }
    }

    fn execute_block<F>(&mut self, spectral_op: &mut F)
    where F: FnMut(&mut [f32], &mut [f32], usize, &AlignedBuffer, &SimdFft) {
        let n = self.fft.size;
        self.scratch_im.fill(0.0);

        // Window & FFT
        {
            use crate::simd_vec::*;
            let mut i = 0;
            while i + 16 <= n {
                let v_in = load_f32x16(&self.in_buffer, i);
                let v_win = load_f32x16(&self.window, i);
                let v_res = v_in * v_win;
                store_f32x16(&mut self.scratch_re, i, v_res);
                i += 16;
            }
            while i + 8 <= n {
                let v_in = load_f32x8(&self.in_buffer, i);
                let v_win = load_f32x8(&self.window, i);
                let v_res = v_in * v_win;
                store_f32x8(&mut self.scratch_re, i, v_res);
                i += 8;
            }
            while i < n {
                self.scratch_re[i] = self.in_buffer[i] * self.window[i];
                i += 1;
            }
        }

        self.fft.process(&mut self.scratch_re, &mut self.scratch_im);

        // Run user operation
        spectral_op(&mut self.scratch_re, &mut self.scratch_im, n, &self.window, &self.fft);

        // Safety pass: clamp and handle non-finite values
        for i in 0..n {
            if !self.scratch_re[i].is_finite() { self.scratch_re[i] = 0.0; }
            else { self.scratch_re[i] = self.scratch_re[i].clamp(-1e6, 1e6); }

            if !self.scratch_im[i].is_finite() { self.scratch_im[i] = 0.0; }
            else { self.scratch_im[i] = self.scratch_im[i].clamp(-1e6, 1e6); }
        }

        // IFFT
        for i in 0..n { self.scratch_im[i] = -self.scratch_im[i]; }
        self.fft.process(&mut self.scratch_re, &mut self.scratch_im);

        // Window & Accumulate
        let norm = 1.0 / n as f32;
        let mask = self.out_mask;
        {
            use crate::simd_vec::*;
            use wide::*;
            let v_norm_16 = FloatX16::splat(norm);
            let mut i = 0;
            while i + 16 <= n {
                let v_re = load_f32x16(&self.scratch_re, i);
                let v_win = load_f32x16(&self.synth_window, i);
                let v_val = (v_re * v_norm_16) * v_win;

                #[cfg(target_feature = "avx512f")]
                {
                    let res: [f32; 16] = v_val.into();
                    for (j, val) in res.iter().enumerate() {
                        let target_ptr = (self.out_ptr + i + j) & mask;
                        unsafe { *self.out_buffer.get_unchecked_mut(target_ptr) += *val; }
                    }
                }
                #[cfg(not(target_feature = "avx512f"))]
                {
                    let res_low: [f32; 8] = v_val.low.into();
                    let res_high: [f32; 8] = v_val.high.into();
                    for (j, val) in res_low.iter().enumerate() {
                        let target_ptr = (self.out_ptr + i + j) & mask;
                        unsafe { *self.out_buffer.get_unchecked_mut(target_ptr) += *val; }
                    }
                    for (j, val) in res_high.iter().enumerate() {
                        let target_ptr = (self.out_ptr + i + 8 + j) & mask;
                        unsafe { *self.out_buffer.get_unchecked_mut(target_ptr) += *val; }
                    }
                }
                i += 16;
            }
            let v_norm = f32x8::from(norm);
            while i + 8 <= n {
                let v_re = load_f32x8(&self.scratch_re, i);
                let v_win = load_f32x8(&self.synth_window, i);
                let v_val = (v_re * v_norm) * v_win;
                let res: [f32; 8] = v_val.into();
                for (j, val) in res.iter().enumerate() {
                    let target_ptr = (self.out_ptr + i + j) & mask;
                    unsafe { *self.out_buffer.get_unchecked_mut(target_ptr) += *val; }
                }
                i += 8;
            }
            while i < n {
                let val = (self.scratch_re[i] * norm) * self.synth_window[i];
                let target_ptr = (self.out_ptr + i) & mask;
                self.out_buffer[target_ptr] += val;
                i += 1;
            }
        }
    }

    pub fn reset(&mut self) {
        self.in_buffer.fill(0.0);
        self.out_buffer.fill(0.0);
        self.scratch_re.fill(0.0);
        self.scratch_im.fill(0.0);
        self.in_ptr = 0;
        self.out_ptr = 0;
    }
}

/// A Spectral Processor for partitioned convolution.
pub struct SpectralProcessor {
    pub pipeline: SpectralPipeline,
    // Partitioned convolution state
    pub(crate) ir_re: Vec<AlignedBuffer>,
    pub(crate) ir_im: Vec<AlignedBuffer>,
    pub(crate) history_re: Vec<AlignedBuffer>,
    pub(crate) history_im: Vec<AlignedBuffer>,
    pub(crate) partition_idx: usize,
}

impl SpectralProcessor {
    pub fn new(fft_size: usize) -> Self {
        Self {
            pipeline: SpectralPipeline::new(fft_size),
            ir_re: Vec::new(),
            ir_im: Vec::new(),
            history_re: Vec::new(),
            history_im: Vec::new(),
            partition_idx: 0,
        }
    }

    pub fn set_ir(&mut self, ir_data: &[f32]) {
        let n = self.pipeline.fft.size;
        let num_partitions = ir_data.len().div_ceil(self.pipeline.hop_size);
        self.ir_re = (0..num_partitions).map(|_| AlignedBuffer::new(n)).collect();
        self.ir_im = (0..num_partitions).map(|_| AlignedBuffer::new(n)).collect();
        self.history_re = (0..num_partitions).map(|_| AlignedBuffer::new(n)).collect();
        self.history_im = (0..num_partitions).map(|_| AlignedBuffer::new(n)).collect();

        for p in 0..num_partitions {
            let start = p * self.pipeline.hop_size;
            let end = (start + self.pipeline.hop_size).min(ir_data.len());
            let mut partition = AlignedBuffer::new(n);
            partition[..end-start].copy_from_slice(&ir_data[start..end]);

            let mut re = partition;
            let mut im = AlignedBuffer::new(n);
            self.pipeline.fft.process(&mut re, &mut im);
            self.ir_re[p] = re;
            self.ir_im[p] = im;
        }
    }

    pub fn complex_mul_accumulate_simd(re: &mut [f32], im: &mut [f32], hr: &[f32], hi: &[f32], ir: &[f32], ii: &[f32]) {
        use crate::simd_vec::*;
        let n = re.len();
        let mut i = 0;
        while i + 16 <= n {
            let v_hr = load_f32x16(hr, i);
            let v_hi = load_f32x16(hi, i);
            let v_ir = load_f32x16(ir, i);
            let v_ii = load_f32x16(ii, i);

            let v_re = load_f32x16(re, i);
            let v_im = load_f32x16(im, i);

            let res_re = v_re + (v_hr * v_ir - v_hi * v_ii);
            let res_im = v_im + (v_hr * v_ii + v_hi * v_ir);

            store_f32x16(re, i, res_re);
            store_f32x16(im, i, res_im);
            i += 16;
        }
        while i + 8 <= n {
            let v_hr = load_f32x8(hr, i);
            let v_hi = load_f32x8(hi, i);
            let v_ir = load_f32x8(ir, i);
            let v_ii = load_f32x8(ii, i);

            let v_re = load_f32x8(re, i);
            let v_im = load_f32x8(im, i);

            let res_re = v_re + (v_hr * v_ir - v_hi * v_ii);
            let res_im = v_im + (v_hr * v_ii + v_hi * v_ir);

            store_f32x8(re, i, res_re);
            store_f32x8(im, i, res_im);
            i += 8;
        }
        while i + 4 <= n {
            let mut v_re = load_f32x4(re, i);
            let mut v_im = load_f32x4(im, i);
            let v_hr = load_f32x4(hr, i);
            let v_hi = load_f32x4(hi, i);
            let v_ir = load_f32x4(ir, i);
            let v_ii = load_f32x4(ii, i);

            complex_mul_accumulate_wasm_simd(&mut v_re, &mut v_im, v_hr, v_hi, v_ir, v_ii);

            store_f32x4(re, i, v_re);
            store_f32x4(im, i, v_im);
            i += 4;
        }
        while i < n {
            re[i] += hr[i] * ir[i] - hi[i] * ii[i];
            im[i] += hr[i] * ii[i] + hi[i] * ir[i];
            i += 1;
        }
    }

    pub fn process_overlap_add(&mut self, input: &[f32], output: &mut [f32]) {
        let ir_re = &self.ir_re;
        let ir_im = &self.ir_im;
        let history_re = &mut self.history_re;
        let history_im = &mut self.history_im;
        let partition_idx = &mut self.partition_idx;

        self.pipeline.process(input, output, |re, im, n, _window, _fft| {
            if ir_re.is_empty() {
                for i in 0..n {
                    let mag_sq = re[i] * re[i] + im[i] * im[i];
                    if mag_sq < 0.0001 { re[i] = 0.0; im[i] = 0.0; }
                }
            } else {
                history_re[*partition_idx].copy_from_slice(re);
                history_im[*partition_idx].copy_from_slice(im);

                re.fill(0.0);
                im.fill(0.0);

                let num_p = ir_re.len();
                for p in 0..num_p {
                    let h_idx = (*partition_idx + num_p - p) % num_p;
                    let hr = &history_re[h_idx];
                    let hi = &history_im[h_idx];
                    let ir = &ir_re[p];
                    let ii = &ir_im[p];
                    SpectralProcessor::complex_mul_accumulate_simd(re, im, hr, hi, ir, ii);
                }
                *partition_idx = (*partition_idx + 1) % num_p;
            }
        });
    }
}

#[cfg(test)]
mod pipeline_reconstruction_tests {
    use super::*;

    const SHAPES: [SpectralWindowShape; 4] = [
        SpectralWindowShape::Hann,
        SpectralWindowShape::Hamming,
        SpectralWindowShape::Blackman,
        SpectralWindowShape::Rectangular,
    ];

    /// Run a steady sine through the pipeline with an identity spectral op and
    /// report (peak ratio, rms ratio) measured in the steady-state region.
    fn identity_gain(shape: SpectralWindowShape, n: usize) -> (f32, f32) {
        let mut p = SpectralPipeline::new(n);
        p.update_window(shape);

        let len = n * 24;
        // Integer cycles per frame keeps the test free of spectral leakage.
        let cycles_per_frame = 8.0;
        let input: Vec<f32> = (0..len)
            .map(|i| (2.0 * std::f32::consts::PI * cycles_per_frame * i as f32 / n as f32).sin() * 0.5)
            .collect();
        let mut output = vec![0.0; len];
        p.process(&input, &mut output, |_re, _im, _n, _w, _f| {});

        // Skip pipeline priming; measure only steady state.
        let start = n * 4;
        let peak = |s: &[f32]| s.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        let rms = |s: &[f32]| (s.iter().map(|v| (v * v) as f64).sum::<f64>() / s.len() as f64).sqrt() as f32;

        (
            peak(&output[start..]) / peak(&input[start..]),
            rms(&output[start..]) / rms(&input[start..]),
        )
    }

    /// Windowing on both analysis and synthesis squares the window, and the
    /// overlapping sum of a squared window is not unity: Hann at 50% overlap
    /// oscillates between 0.5 and 1.0 (an audible ~344 Hz buzz at fft_size
    /// 256), and Rectangular doubles the signal outright. The COLA-normalized
    /// synthesis window must reconstruct the input exactly instead.
    #[test]
    fn test_identity_reconstruction_is_unity_for_every_window() {
        for &shape in &SHAPES {
            let (peak_ratio, rms_ratio) = identity_gain(shape, 256);
            assert!(
                (peak_ratio - 1.0).abs() < 0.02,
                "{:?}: identity peak gain {:.4} is not unity",
                shape,
                peak_ratio
            );
            assert!(
                (rms_ratio - 1.0).abs() < 0.02,
                "{:?}: identity RMS gain {:.4} is not unity — the overlap-add sum \
                 ripples, which amplitude-modulates the signal even though the peak looks correct",
                shape,
                rms_ratio
            );
        }
    }

    /// The normalization must hold across FFT sizes, not just the one measured.
    #[test]
    fn test_identity_reconstruction_holds_across_fft_sizes() {
        for &n in &[128usize, 256, 512, 1024] {
            let (peak_ratio, rms_ratio) = identity_gain(SpectralWindowShape::Hann, n);
            assert!(
                (peak_ratio - 1.0).abs() < 0.02 && (rms_ratio - 1.0).abs() < 0.02,
                "fft_size {}: peak {:.4} rms {:.4} — expected unity",
                n,
                peak_ratio,
                rms_ratio
            );
        }
    }

    /// The synthesis window is only correct if analysis*synthesis sums to 1
    /// at every position modulo the hop; check the invariant directly.
    #[test]
    fn test_cola_sum_is_unity_at_every_hop_position() {
        for &shape in &SHAPES {
            let n = 256;
            let p = {
                let mut p = SpectralPipeline::new(n);
                p.update_window(shape);
                p
            };
            for j in 0..p.hop_size {
                let mut sum = 0.0f32;
                let mut k = j;
                while k < n {
                    sum += p.window[k] * p.synth_window[k];
                    k += p.hop_size;
                }
                assert!(
                    (sum - 1.0).abs() < 1e-4,
                    "{:?}: overlap-add sum at hop position {} is {:.6}, not 1.0",
                    shape,
                    j,
                    sum
                );
            }
        }
    }
}
