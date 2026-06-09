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

    pub fn complex_mul_accumulate_simd(re: &mut [f32], im: &mut [f32], hr: &[f32], hi: &[f32], ir: &[f32], ii: &[f32]) {
        use wide::*;
        let n = re.len();
        let mut i = 0;
        while i + 8 <= n {
            let v_hr = f32x8::new(hr[i..i+8].try_into().unwrap());
            let v_hi = f32x8::new(hi[i..i+8].try_into().unwrap());
            let v_ir = f32x8::new(ir[i..i+8].try_into().unwrap());
            let v_ii = f32x8::new(ii[i..i+8].try_into().unwrap());

            let v_re = f32x8::new(re[i..i+8].try_into().unwrap());
            let v_im = f32x8::new(im[i..i+8].try_into().unwrap());

            let res_re = v_re + (v_hr * v_ir - v_hi * v_ii);
            let res_im = v_im + (v_hr * v_ii + v_hi * v_ir);

            let arr_re: [f32; 8] = res_re.into();
            re[i..i+8].copy_from_slice(&arr_re);
            let arr_im: [f32; 8] = res_im.into();
            im[i..i+8].copy_from_slice(&arr_im);
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

        {
            use wide::*;
            let mut i = 0;
            while i + 8 <= n {
                let v_in = f32x8::new(self.in_buffer[i..i+8].try_into().unwrap());
                let v_win = f32x8::new(self.window[i..i+8].try_into().unwrap());
                let v_res = v_in * v_win;
                let arr_res: [f32; 8] = v_res.into();
                self.scratch_re[i..i+8].copy_from_slice(&arr_res);
                i += 8;
            }
            while i < n {
                self.scratch_re[i] = self.in_buffer[i] * self.window[i];
                i += 1;
            }
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
            for p in 0..num_p {
                let h_idx = (self.partition_idx + num_p - p) % num_p;
                let hr = &self.history_re[h_idx];
                let hi = &self.history_im[h_idx];
                let ir = &self.ir_re[p];
                let ii = &self.ir_im[p];

                Self::complex_mul_accumulate_simd(&mut self.scratch_re, &mut self.scratch_im, hr, hi, ir, ii);
            }
            self.partition_idx = (self.partition_idx + 1) % num_p;
        }

        // Inverse FFT
        for i in 0..n { self.scratch_im[i] = -self.scratch_im[i]; }
        self.fft.process(&mut self.scratch_re, &mut self.scratch_im);

        let norm = 1.0 / n as f32;
        let mask = self.out_mask;

        {
            use wide::*;
            let v_norm = f32x8::from(norm);
            let mut i = 0;
            while i + 8 <= n {
                let v_re = f32x8::new(self.scratch_re[i..i+8].try_into().unwrap());
                let v_win = f32x8::new(self.window[i..i+8].try_into().unwrap());
                let v_val = (v_re * v_norm) * v_win;

                let res: [f32; 8] = v_val.into();
                for j in 0..8 {
                    let target_ptr = (self.out_ptr + i + j) & mask;
                    unsafe { *self.out_buffer.get_unchecked_mut(target_ptr) += res[j]; }
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
    }
}
