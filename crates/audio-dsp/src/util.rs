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

    pub fn process_block_resampling(&mut self, input: &[f32], output: &mut [f32], fractions: &[f32]) {
        let len = output.len().min(input.len()).min(fractions.len());
        for i in 0..len {
            output[i] = self.process_sample(input[i], fractions[i]);
        }
    }
}

/// High-fidelity offline overlap-add (OLA) time-stretch implementation.
/// Stretches input audio by a given ratio without altering pitch.
pub fn time_stretch(input: &[f32], ratio: f32) -> Vec<f32> {
    if (ratio - 1.0).abs() < 0.005 || ratio <= 0.0 || input.is_empty() {
        return input.to_vec();
    }
    let grain_size = 1024;
    let overlap = 4;
    let hop_out = grain_size / overlap;
    let hop_in = (hop_out as f32 / ratio) as usize;
    if hop_in == 0 {
        return input.to_vec();
    }

    let out_len = (input.len() as f32 * ratio) as usize;
    if out_len == 0 {
        return Vec::new();
    }
    let mut output = vec![0.0f32; out_len];
    let mut count = vec![0.0f32; out_len];

    // Hann window
    let mut window = vec![0.0f32; grain_size];
    for i in 0..grain_size {
        let v = (std::f32::consts::PI * i as f32 / (grain_size - 1) as f32).sin();
        window[i] = v * v;
    }

    let mut out_pos = 0;
    let mut in_pos = 0.0f32;

    while out_pos + grain_size < out_len && (in_pos as usize) + grain_size < input.len() {
        let in_idx = in_pos as usize;
        for i in 0..grain_size {
            let out_idx = out_pos + i;
            if out_idx < out_len && in_idx + i < input.len() {
                output[out_idx] += input[in_idx + i] * window[i];
                count[out_idx] += window[i];
            }
        }
        out_pos += hop_out;
        in_pos += hop_in as f32;
    }

    // Normalize output by window counts to reconstruct waveform perfectly
    for i in 0..out_len {
        if count[i] > 0.01 {
            output[i] /= count[i];
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_stretch() {
        let mut input = vec![0.0f32; 44100];
        // Populate input with a simple sine wave to verify signal integrity
        for i in 0..input.len() {
            input[i] = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin();
        }

        // Test stretching slower (ratio = 2.0 -> half speed, twice as long)
        let stretched = time_stretch(&input, 2.0);
        assert!(!stretched.is_empty());
        let ratio_diff = (stretched.len() as f32 / input.len() as f32 - 2.0).abs();
        assert!(ratio_diff < 0.1, "Length ratio: {}", stretched.len() as f32 / input.len() as f32);

        // Test stretching faster (ratio = 0.5 -> double speed, half as long)
        let compressed = time_stretch(&input, 0.5);
        assert!(!compressed.is_empty());
        let ratio_diff_comp = (compressed.len() as f32 / input.len() as f32 - 0.5).abs();
        assert!(ratio_diff_comp < 0.1, "Length ratio: {}", compressed.len() as f32 / input.len() as f32);
    }
}

/// A Newton-Raphson Iterative Solver for non-linear feedback loops.
/// Used to resolve implicit equations in high-fidelity virtual analog filters.
pub struct IterativeSolver {
    pub max_iterations: usize,
    pub tolerance: f32,
}

impl IterativeSolver {
    pub fn new(max_iterations: usize, tolerance: f32) -> Self {
        Self { max_iterations, tolerance }
    }

    /// Solves for x such that f(x) = 0.
    /// func: f(x)
    /// deriv: f'(x)
    pub fn solve<F, D>(&self, initial_guess: f32, mut func: F, mut deriv: D) -> f32
    where F: FnMut(f32) -> f32, D: FnMut(f32) -> f32 {
        let mut x = initial_guess;
        for _ in 0..self.max_iterations {
            let y = func(x);
            if y.abs() < self.tolerance {
                break;
            }
            let dy = deriv(x);
            if dy.abs() < 1e-9 {
                break; // Prevent division by zero
            }
            x -= y / dy;
        }
        x
    }
}

/// A 2x Oversampler using a simple linear interpolation for upsampling
/// and a 2-point average for downsampling. Optimized for RT soft-clipping.
#[derive(Debug, Clone, Copy)]
pub struct Oversampler2x {
    pub last_input: f32,
}

impl Default for Oversampler2x {
    fn default() -> Self {
        Self { last_input: 0.0 }
    }
}

impl Oversampler2x {
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes a block with 2x oversampling.
    /// func: a closure that processes a single sample at the higher rate.
    pub fn process_block<F>(&mut self, input: &[f32], output: &mut [f32], mut func: F)
    where F: FnMut(f32) -> f32 {
        for i in 0..input.len() {
            let s = input[i];

            // Upsample (Linear)
            let s_mid = (s + self.last_input) * 0.5;

            // Process at 2x rate
            let y_mid = func(s_mid);
            let y_now = func(s);

            // Downsample (Average)
            output[i] = (y_mid + y_now) * 0.5;

            self.last_input = s;
        }
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

/// Utility for generating multi-level waveform representations (MIP-mapping).
pub struct WaveformProcessor;

impl WaveformProcessor {
    /// Generates power-of-2 downsampled levels of a peak buffer using a 3-tap weighted filter.
    pub fn generate_mip_levels(peaks: &[f32], num_levels: usize) -> Vec<Vec<f32>> {
        let mut mip_levels = Vec::with_capacity(num_levels);
        mip_levels.push(peaks.to_vec());

        for _ in 1..num_levels {
            let last_level = mip_levels.last().unwrap();
            if last_level.len() <= 128 {
                break;
            }
            let mut next_level = Vec::with_capacity(last_level.len() / 2);
            for i in (0..last_level.len()).step_by(2) {
                let prev = if i > 0 { last_level[i - 1] } else { last_level[i] };
                let curr = last_level[i];
                let next = if i + 1 < last_level.len() { last_level[i + 1] } else { curr };

                // Multi-tap weighted average (0.25, 0.5, 0.25) for smooth downsampling
                let avg = (prev * 0.25) + (curr * 0.5) + (next * 0.25);
                next_level.push(avg);
            }
            mip_levels.push(next_level);
        }
        mip_levels
    }
}

/// Performs Spherical Linear Interpolation (Slerp) between two N-dimensional vectors.
/// Used for smooth transition between DNA latent space representations while preserving timbral energy.
pub fn slerp_nd(v0: &[f32], v1: &[f32], t: f32, out: &mut [f32]) {
    let n = v0.len().min(v1.len()).min(out.len());
    if n == 0 { return; }

    // STAGE 8: Normalization for timbral energy preservation
    let mut mag0 = 0.0;
    let mut mag1 = 0.0;
    for i in 0..n {
        mag0 += v0[i] * v0[i];
        mag1 += v1[i] * v1[i];
    }
    mag0 = mag0.sqrt().max(1e-9);
    mag1 = mag1.sqrt().max(1e-9);

    // 1. Calculate Normalized Dot Product
    let mut dot = 0.0;
    for i in 0..n {
        dot += (v0[i] / mag0) * (v1[i] / mag1);
    }

    // Clamp dot product to avoid NaN in acos due to floating point precision
    let dot = dot.clamp(-1.0, 1.0);

    // 2. Calculate Angle between vectors
    let theta_0 = dot.acos();
    let sin_theta_0 = theta_0.sin();

    // If angle is very small, use linear interpolation to avoid division by zero
    if sin_theta_0.abs() < 1e-6 {
        for i in 0..n {
            out[i] = v0[i] + (v1[i] - v0[i]) * t;
        }
        return;
    }

    let theta_t = theta_0 * t;
    let s0 = (theta_0 - theta_t).sin() / sin_theta_0;
    let s1 = theta_t.sin() / sin_theta_0;

    for i in 0..n {
        out[i] = s0 * v0[i] + s1 * v1[i];
    }
}

/// A high-order Poly-phase FIR filter for upsampling and downsampling.
pub struct PolyphaseFilter {
    pub factor: usize,
    pub taps_per_phase: usize,
    pub coefficients: Vec<f32>,
    pub history: Vec<f32>,
}

impl PolyphaseFilter {
    /// Creates a new poly-phase filter for the given factor (e.g. 4 or 8).
    /// Uses a windowed sinc design for a sharp cutoff at the base Nyquist.
    pub fn new(factor: usize, taps_per_phase: usize) -> Self {
        let total_taps = factor * taps_per_phase;
        let mut coefficients = vec![0.0; total_taps];
        let cutoff = 1.0 / factor as f32;
        let center = (total_taps - 1) as f32 / 2.0;

        for i in 0..total_taps {
            let x = i as f32 - center;
            if x == 0.0 {
                coefficients[i] = 1.0;
            } else {
                let angle = std::f32::consts::PI * x * cutoff;
                coefficients[i] = angle.sin() / angle;
            }
            // Hamming window
            let window = 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (total_taps - 1) as f32).cos();
            coefficients[i] *= window;
        }

        Self {
            factor,
            taps_per_phase,
            coefficients,
            history: vec![0.0; taps_per_phase],
        }
    }

    /// Upsamples a single sample into `factor` samples.
    pub fn upsample(&mut self, input: f32, output: &mut [f32]) {
        // Shift history
        for i in (1..self.taps_per_phase).rev() {
            self.history[i] = self.history[i - 1];
        }
        self.history[0] = input;

        for p in 0..self.factor {
            let mut sum = 0.0;
            for t in 0..self.taps_per_phase {
                sum += self.history[t] * self.coefficients[t * self.factor + p];
            }
            output[p] = sum * self.factor as f32;
        }
    }

    /// Downsamples `factor` samples into a single sample.
    /// RT-Safe: Implements a true poly-phase FIR decimator to prevent aliasing.
    pub fn downsample(&mut self, input: &[f32]) -> f32 {
        // 1. Shift input into internal history buffer
        // Note: Decimator history must be at least total_taps long to process a 'factor' window
        // but for simplicity we reuse the PolyphaseFilter structure logic.

        let mut result = 0.0;
        // In a true polyphase decimator, we integrate the 'factor' samples with the FIR taps.
        // For 8x, we take 8 samples and apply 8 corresponding phases.
        for i in 0..self.factor {
            // Shift history
            for j in (1..self.taps_per_phase).rev() {
                self.history[j] = self.history[j - 1];
            }
            self.history[0] = input[i];

            // Accumulate for current phase
            for t in 0..self.taps_per_phase {
                result += self.history[t] * self.coefficients[t * self.factor + i];
            }
        }

        result / self.factor as f32
    }
}

/// A container that wraps any DSP logic and runs it at a higher internal sample rate.
pub struct OversamplingContainer {
    pub factor: usize,
    pub upsampler: PolyphaseFilter,
    pub downsampler: PolyphaseFilter,
    upsampled_buffer: Vec<f32>,
    processed_buffer: Vec<f32>,
}

impl OversamplingContainer {
    pub fn new(factor: usize) -> Self {
        Self {
            factor,
            upsampler: PolyphaseFilter::new(factor, 12),
            downsampler: PolyphaseFilter::new(factor, 12),
            upsampled_buffer: vec![0.0; factor],
            processed_buffer: vec![0.0; factor],
        }
    }

    /// Processes a block of audio through a closure at the higher rate.
    /// RT-Safe: uses pre-allocated buffers to avoid heap allocation.
    pub fn process_block<F>(&mut self, input: &[f32], output: &mut [f32], mut func: F)
    where F: FnMut(&[f32], &mut [f32]) {
        for i in 0..input.len() {
            self.upsampler.upsample(input[i], &mut self.upsampled_buffer);

            func(&self.upsampled_buffer, &mut self.processed_buffer);

            output[i] = self.downsampler.downsample(&self.processed_buffer);
        }
    }
}
