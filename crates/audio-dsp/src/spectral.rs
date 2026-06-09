use crate::util::AlignedBuffer;
use crate::SimdFft;

/// A Spectral Processor for partitioned convolution.
pub struct SpectralProcessor {
    pub fft: SimdFft,
    pub(crate) in_buffer: AlignedBuffer,
    pub(crate) out_buffer: AlignedBuffer,
    pub(crate) scratch_re: AlignedBuffer,
    pub(crate) scratch_im: AlignedBuffer,
    pub(crate) window: AlignedBuffer,
    pub hop_size: usize,
    pub(crate) in_ptr: usize,
    pub(crate) out_ptr: usize,
    pub(crate) out_mask: usize,
    // Partitioned convolution state
    pub(crate) ir_re: Vec<AlignedBuffer>,
    pub(crate) ir_im: Vec<AlignedBuffer>,
    pub(crate) history_re: Vec<AlignedBuffer>,
    pub(crate) history_im: Vec<AlignedBuffer>,
    pub(crate) partition_idx: usize,
}

impl SpectralProcessor {
    pub fn new(fft_size: usize) -> Self {
        assert!(fft_size.is_power_of_two());
        let mut window = AlignedBuffer::new(fft_size);
        for i in 0..fft_size {
            window[i] = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos());
        }
        let hop_size = fft_size / 2;
        // out_buffer must be power of two for bitwise mask optimization
        let out_buffer_size = (fft_size + hop_size).next_power_of_two();
        Self {
            fft: SimdFft::new(fft_size),
            in_buffer: AlignedBuffer::new(fft_size),
            out_buffer: AlignedBuffer::new(out_buffer_size),
            scratch_re: AlignedBuffer::new(fft_size),
            scratch_im: AlignedBuffer::new(fft_size),
            window,
            hop_size,
            in_ptr: 0,
            out_ptr: 0,
            out_mask: out_buffer_size - 1,
            ir_re: Vec::new(),
            ir_im: Vec::new(),
            history_re: Vec::new(),
            history_im: Vec::new(),
            partition_idx: 0,
        }
    }

    pub fn set_ir(&mut self, ir_data: &[f32]) {
        let n = self.fft.size;
        let num_partitions = ir_data.len().div_ceil(self.hop_size);
        self.ir_re = (0..num_partitions).map(|_| AlignedBuffer::new(n)).collect();
        self.ir_im = (0..num_partitions).map(|_| AlignedBuffer::new(n)).collect();
        self.history_re = (0..num_partitions).map(|_| AlignedBuffer::new(n)).collect();
        self.history_im = (0..num_partitions).map(|_| AlignedBuffer::new(n)).collect();

        for p in 0..num_partitions {
            let start = p * self.hop_size;
            let end = (start + self.hop_size).min(ir_data.len());
            let mut partition = AlignedBuffer::new(n);
            partition[..end-start].copy_from_slice(&ir_data[start..end]);

            let mut re = partition;
            let mut im = AlignedBuffer::new(n);
            self.fft.process(&mut re, &mut im);
            self.ir_re[p] = re;
            self.ir_im[p] = im;
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    /// # Safety
    /// Caller must ensure all slices have the same length and are valid for reading/writing.
    pub unsafe fn complex_mul_accumulate_avx2(re: &mut [f32], im: &mut [f32], hr: &[f32], hi: &[f32], ir: &[f32], ii: &[f32]) {
        use std::arch::x86_64::*;
        let n = re.len();
        let mut i = 0;
        // SAFETY: The caller ensures that re, im, hr, hi, ir, and ii all have the same length 'n'
        // and that they are correctly aligned for AVX2 operations (or unaligned loads are used).
        while i + 8 <= n {
            unsafe {
                let v_hr = _mm256_loadu_ps(hr.as_ptr().add(i));
                let v_hi = _mm256_loadu_ps(hi.as_ptr().add(i));
                let v_ir = _mm256_loadu_ps(ir.as_ptr().add(i));
                let v_ii = _mm256_loadu_ps(ii.as_ptr().add(i));

                let v_re = _mm256_loadu_ps(re.as_ptr().add(i));
                let v_im = _mm256_loadu_ps(im.as_ptr().add(i));

                // re = re + (hr * ir - hi * ii)
                let res_re = _mm256_add_ps(v_re, _mm256_sub_ps(_mm256_mul_ps(v_hr, v_ir), _mm256_mul_ps(v_hi, v_ii)));
                // im = im + (hr * ii + hi * ir)
                let res_im = _mm256_add_ps(v_im, _mm256_add_ps(_mm256_mul_ps(v_hr, v_ii), _mm256_mul_ps(v_hi, v_ir)));

                _mm256_storeu_ps(re.as_mut_ptr().add(i), res_re);
                _mm256_storeu_ps(im.as_mut_ptr().add(i), res_im);
            }
            i += 8;
        }
        while i < n {
            re[i] += hr[i] * ir[i] - hi[i] * ii[i];
            im[i] += hr[i] * ii[i] + hi[i] * ir[i];
            i += 1;
        }
    }

    pub fn process_overlap_add(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len();
        let mask = self.out_mask;
        for i in 0..len {
            self.in_buffer[self.in_ptr] = input[i];
            output[i] = self.out_buffer[self.out_ptr];
            self.out_buffer[self.out_ptr] = 0.0;

            self.in_ptr += 1;
            self.out_ptr = (self.out_ptr + 1) & mask;

            if self.in_ptr >= self.fft.size {
                self.execute_spectral_block();
                self.in_buffer.copy_within(self.hop_size..self.fft.size, 0);
                self.in_ptr = self.fft.size - self.hop_size;
            }
        }
    }

    pub(crate) fn execute_spectral_block(&mut self) {
        let n = self.fft.size;
        self.scratch_im.fill(0.0);

        #[cfg(target_arch = "x86_64")]
        if is_x86_feature_detected!("avx2") {
            unsafe {
                use std::arch::x86_64::*;
                let mut i = 0;
                while i + 8 <= n {
                    // Use unaligned loads if i is not a multiple of 8,
                    // though AlignedBuffer guarantees base alignment.
                    let v_in = _mm256_loadu_ps(self.in_buffer.as_ptr().add(i));
                    let v_win = _mm256_loadu_ps(self.window.as_ptr().add(i));
                    let v_res = _mm256_mul_ps(v_in, v_win);
                    _mm256_storeu_ps(self.scratch_re.as_mut_ptr().add(i), v_res);
                    i += 8;
                }
                while i < n {
                    self.scratch_re[i] = self.in_buffer[i] * self.window[i];
                    i += 1;
                }
            }
        } else {
            for i in 0..n {
                self.scratch_re[i] = self.in_buffer[i] * self.window[i];
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        for i in 0..n {
            self.scratch_re[i] = self.in_buffer[i] * self.window[i];
        }

        self.fft.process(&mut self.scratch_re, &mut self.scratch_im);

        if self.ir_re.is_empty() {
            // Fallback to identity EQ if no IR is loaded
            for i in 0..n {
                let mag_sq = self.scratch_re[i] * self.scratch_re[i] + self.scratch_im[i] * self.scratch_im[i];
                if mag_sq < 0.0001 {
                    self.scratch_re[i] = 0.0;
                    self.scratch_im[i] = 0.0;
                }
            }
        } else {
            // Partitioned Convolution
            self.history_re[self.partition_idx].copy_from_slice(&self.scratch_re);
            self.history_im[self.partition_idx].copy_from_slice(&self.scratch_im);

            self.scratch_re.fill(0.0);
            self.scratch_im.fill(0.0);

            let num_p = self.ir_re.len();
            #[cfg(target_arch = "x86_64")]
            let has_avx2 = is_x86_feature_detected!("avx2");
            for p in 0..num_p {
                let h_idx = (self.partition_idx + num_p - p) % num_p;
                let hr = &self.history_re[h_idx];
                let hi = &self.history_im[h_idx];
                let ir = &self.ir_re[p];
                let ii = &self.ir_im[p];

                #[cfg(target_arch = "x86_64")]
                if has_avx2 {
                    unsafe { Self::complex_mul_accumulate_avx2(&mut self.scratch_re, &mut self.scratch_im, hr, hi, ir, ii); }
                } else {
                    for i in 0..n {
                        self.scratch_re[i] += hr[i] * ir[i] - hi[i] * ii[i];
                        self.scratch_im[i] += hr[i] * ii[i] + hi[i] * ir[i];
                    }
                }

                #[cfg(not(target_arch = "x86_64"))]
                for i in 0..n {
                    self.scratch_re[i] += hr[i] * ir[i] - hi[i] * ii[i];
                    self.scratch_im[i] += hr[i] * ii[i] + hi[i] * ir[i];
                }
            }
            self.partition_idx = (self.partition_idx + 1) % num_p;
        }

        // Inverse FFT
        for i in 0..n { self.scratch_im[i] = -self.scratch_im[i]; }
        self.fft.process(&mut self.scratch_re, &mut self.scratch_im);

        let norm = 1.0 / n as f32;
        let mask = self.out_mask;

        #[cfg(target_arch = "x86_64")]
        if is_x86_feature_detected!("avx2") {
            unsafe {
                use std::arch::x86_64::*;
                let v_norm = _mm256_set1_ps(norm);
                let mut i = 0;
                while i + 8 <= n {
                    let v_re = _mm256_loadu_ps(self.scratch_re.as_ptr().add(i));
                    let v_win = _mm256_loadu_ps(self.window.as_ptr().add(i));
                    let v_val = _mm256_mul_ps(_mm256_mul_ps(v_re, v_norm), v_win);

                    let mut res = [0.0f32; 8];
                    _mm256_storeu_ps(res.as_mut_ptr(), v_val);
                    for j in 0..8 {
                        let target_ptr = (self.out_ptr + i + j) & mask;
                        *self.out_buffer.get_unchecked_mut(target_ptr) += *res.get_unchecked(j);
                    }
                    i += 8;
                }
                while i < n {
                    let val = (self.scratch_re[i] * norm) * self.window[i];
                    let target_ptr = (self.out_ptr + i) & mask;
                    self.out_buffer[target_ptr] += val;
                    i += 1;
                }
            }
        } else {
            for i in 0..n {
                let val = (self.scratch_re[i] * norm) * self.window[i];
                let target_ptr = (self.out_ptr + i) & mask;
                self.out_buffer[target_ptr] += val;
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        for i in 0..n {
            unsafe {
                let val = (*self.scratch_re.get_unchecked(i) * norm) * *self.window.get_unchecked(i);
                let target_ptr = (self.out_ptr + i) & mask;
                *self.out_buffer.get_unchecked_mut(target_ptr) += val;
            }
        }
    }
}
