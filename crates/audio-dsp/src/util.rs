/// A simple 64-byte aligned buffer for SIMD operations to avoid Undefined Behavior.
pub struct AlignedBuffer {
    pub(crate) ptr: *mut f32,
    pub(crate) size: usize,
    pub(crate) layout: std::alloc::Layout,
}

impl AlignedBuffer {
    pub fn new(size: usize) -> Self {
        let layout = std::alloc::Layout::from_size_align(size * std::mem::size_of::<f32>(), 64).unwrap();
        // SAFETY: AlignedBuffer ensures 64-byte alignment and zero-initialization.
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) as *mut f32 };
        if ptr.is_null() { std::alloc::handle_alloc_error(layout); }
        Self { ptr, size, layout }
    }
}

// SAFETY: AlignedBuffer owns its data and provides thread-safe access to its contents.
unsafe impl Send for AlignedBuffer {}
unsafe impl Sync for AlignedBuffer {}

impl std::ops::Deref for AlignedBuffer {
    type Target = [f32];
    fn deref(&self) -> &Self::Target {
        // SAFETY: ptr is valid for 'size' elements as guaranteed by new().
        unsafe { std::slice::from_raw_parts(self.ptr, self.size) }
    }
}

impl std::ops::DerefMut for AlignedBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: ptr is valid and unique as AlignedBuffer owns the allocation.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        // SAFETY: layout matches the one used in alloc_zeroed().
        unsafe { std::alloc::dealloc(self.ptr as *mut u8, self.layout); }
    }
}

/// A high-fidelity Lagrange 4-point resampler.
pub struct LagrangeResampler {
    pub history: [f32; 4],
}

impl Default for LagrangeResampler {
    fn default() -> Self {
        Self::new()
    }
}

impl LagrangeResampler {
    pub fn new() -> Self { Self { history: [0.0; 4] } }

    pub fn process_sample(&mut self, input: f32, fraction: f32) -> f32 {
        self.history[0] = self.history[1];
        self.history[1] = self.history[2];
        self.history[2] = self.history[3];
        self.history[3] = input;

        let a = self.history[0];
        let b = self.history[1];
        let c = self.history[2];
        let d = self.history[3];

        // 4-point Lagrange interpolation
        let c0 = b;
        let c1 = c - (1.0/3.0)*a - 0.5*b - (1.0/6.0)*d;
        let c2 = 0.5*(a + c) - b;
        let c3 = (1.0/6.0)*(d - a) + 0.5*(b - c);

        c3*fraction*fraction*fraction + c2*fraction*fraction + c1*fraction + c0
    }
}

/// Calculates a sliding window magnitude average for spectral envelope extraction.
/// Optimized with SIMD where possible.
pub fn extract_spectral_envelope(re: &[f32], im: &[f32], env: &mut [f32], window_size: usize) {
    let n = re.len();
    let window_size = window_size.max(1).min(n / 2);

    // First, calculate raw magnitudes
    let mut magnitudes = AlignedBuffer::new(n);
    {
        use crate::simd_vec::*;
        let mut i = 0;
        while i + 8 <= n {
            let v_re = load_f32x8(re, i);
            let v_im = load_f32x8(im, i);
            let v_mag = (v_re * v_re + v_im * v_im).sqrt();
            store_f32x8(&mut magnitudes, i, v_mag);
            i += 8;
        }
        while i < n {
            magnitudes[i] = (re[i] * re[i] + im[i] * im[i]).sqrt();
            i += 1;
        }
    }

    // Then, sliding window average
    // A simple O(N) approach using a running sum
    let mut current_sum = 0.0;
    let mut count = 0;

    // Initial window
    for i in 0..window_size.min(n) {
        current_sum += magnitudes[i];
        count += 1;
    }

    for i in 0..n {
        // Add new element to window if possible
        let add_idx = i + window_size;
        if add_idx < n {
            current_sum += magnitudes[add_idx];
            count += 1;
        }

        // Remove old element from window if possible
        if i > window_size {
            let rem_idx = i - window_size - 1;
            current_sum -= magnitudes[rem_idx];
            count -= 1;
        }

        env[i] = current_sum / count as f32;
    }
}

/// A spectral flux based transient detector for onset analysis.
pub struct TransientDetector {
    prev_magnitudes: Vec<f32>,
    threshold: f32,
}

impl TransientDetector {
    pub fn new(size: usize, threshold: f32) -> Self {
        Self {
            prev_magnitudes: vec![0.0; size],
            threshold,
        }
    }

    /// Calculates the spectral flux between the current and previous spectral frames.
    /// Returns a value indicating the intensity of the onset.
    pub fn detect_onset(&mut self, re: &[f32], im: &[f32]) -> f32 {
        let n = re.len().min(self.prev_magnitudes.len());
        let mut flux = 0.0;

        // Use a small epsilon to avoid sqrt(0) issues or tiny fluctuations
        let eps = 1e-6;

        for i in 0..n {
            let mag = (re[i] * re[i] + im[i] * im[i] + eps).sqrt();
            let diff = mag - self.prev_magnitudes[i];

            // Weight higher frequencies slightly more for percussive onsets
            let weight = 1.0 + (i as f32 / n as f32);

            // Only accumulate positive changes (onsets)
            if diff > 0.0 {
                flux += diff * weight;
            }
            self.prev_magnitudes[i] = mag;
        }

        flux / n as f32
    }

    pub fn is_transient(&mut self, re: &[f32], im: &[f32]) -> bool {
        self.detect_onset(re, im) > self.threshold
    }
}
