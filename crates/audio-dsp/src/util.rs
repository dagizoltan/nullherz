/// A simple 64-byte aligned buffer for SIMD operations to avoid Undefined Behavior.
pub struct AlignedBuffer {
    pub(crate) ptr: *mut f32,
    pub(crate) size: usize,
    pub(crate) layout: std::alloc::Layout,
}

impl AlignedBuffer {
    pub fn new(size: usize) -> Self {
        let layout = std::alloc::Layout::from_size_align(size * std::mem::size_of::<f32>(), 64).unwrap();
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) as *mut f32 };
        if ptr.is_null() { std::alloc::handle_alloc_error(layout); }
        Self { ptr, size, layout }
    }
}

unsafe impl Send for AlignedBuffer {}
unsafe impl Sync for AlignedBuffer {}

impl std::ops::Deref for AlignedBuffer {
    type Target = [f32];
    fn deref(&self) -> &Self::Target {
        unsafe { std::slice::from_raw_parts(self.ptr, self.size) }
    }
}

impl std::ops::DerefMut for AlignedBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self.ptr as *mut u8, self.layout); }
    }
}

/// A high-fidelity Lagrange 4-point resampler.
pub struct LagrangeResampler {
    pub history: [f32; 4],
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
