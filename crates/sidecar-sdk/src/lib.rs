use ipc_layer::{ShmRingBuffer, AudioBlock, ShmSignal, EventFd};
pub use nullherz_traits::{AudioProcessor, ProcessContext};

use ipc_layer::SharedMemory;

/// High-level abstraction for memory mapping providers (e.g. native SHM, virtio, etc.)
pub trait MemoryMapper {
    type Mapping: AsRef<[u8]>;
    fn open(&self, name: &str, size: usize) -> Result<Self::Mapping, String>;
    fn ptr(&self, mapping: &Self::Mapping) -> *mut u8;
}

/// Default native shared memory implementation.
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeMemoryMapper;
#[cfg(not(target_arch = "wasm32"))]
impl MemoryMapper for NativeMemoryMapper {
    type Mapping = SharedMemory;
    fn open(&self, name: &str, size: usize) -> Result<Self::Mapping, String> {
        SharedMemory::open(name, size).map_err(|e| e.to_string())
    }
    fn ptr(&self, mapping: &Self::Mapping) -> *mut u8 {
        mapping.ptr()
    }
}

/// WASM shared memory implementation that parses pointer addresses from the name.
pub struct WasmMemoryMapper;
impl MemoryMapper for WasmMemoryMapper {
    type Mapping = *mut u8;
    fn open(&self, name: &str, _size: usize) -> Result<Self::Mapping, String> {
        let ptr_val = name.parse::<usize>().map_err(|_| "WASM memory name must be a raw pointer integer string".to_string())?;
        Ok(ptr_val as *mut u8)
    }
    fn ptr(&self, mapping: &Self::Mapping) -> *mut u8 {
        *mapping
    }
}

/// A sidecar DSP process context.
pub struct SidecarHost<M: MemoryMapper = NativeMemoryMapper> {
    mapper: M,
    shm_cmd: M::Mapping,
    shm_midi: Option<M::Mapping>,
    shm_signal: M::Mapping,
    shm_inputs: Vec<M::Mapping>,
    shm_sidechains: Vec<M::Mapping>,
    shm_outputs: Vec<M::Mapping>,
    event_fd: Option<EventFd>,
}

impl<M: MemoryMapper> SidecarHost<M> {
    /// # Safety
    /// All shared memory segment names must exist and be accessible by the current process via the mapper.
    pub unsafe fn new_with_mapper(mapper: M, cmd_name: &str, sig_name: &str, in_names: &[String], sc_names: &[String], out_names: &[String], efd: i32) -> Self {
        let (cmd_layout, _) = ShmRingBuffer::<nullherz_traits::TimestampedCommand>::layout(64);
        let shm_cmd = mapper.open(cmd_name, cmd_layout.size()).expect("Failed to open cmd SHM");

        let shm_signal = mapper.open(sig_name, std::mem::size_of::<ShmSignal>()).expect("Failed to open signal SHM");

        let (audio_layout, _) = ShmRingBuffer::<AudioBlock>::layout(16);
        let mut shm_inputs = Vec::new();
        for name in in_names {
            shm_inputs.push(mapper.open(name, audio_layout.size()).expect("Failed to open input SHM"));
        }

        let mut shm_sidechains = Vec::new();
        for name in sc_names {
            shm_sidechains.push(mapper.open(name, audio_layout.size()).expect("Failed to open sidechain SHM"));
        }

        let mut shm_outputs = Vec::new();
        for name in out_names {
            shm_outputs.push(mapper.open(name, audio_layout.size()).expect("Failed to open output SHM"));
        }

        let event_fd = if efd >= 0 { Some(EventFd::from_raw(efd)) } else { None };

        Self {
            mapper,
            shm_cmd,
            shm_midi: None,
            shm_signal,
            shm_inputs,
            shm_sidechains,
            shm_outputs,
            event_fd,
        }
    }

    pub fn run(&mut self, processor: impl AudioProcessor) {
        let mut context = SidecarContext::new_with_mapper(
            &self.mapper,
            processor,
            &self.shm_cmd,
            &self.shm_signal,
            &self.shm_inputs,
            &self.shm_sidechains,
            &self.shm_outputs,
            self.event_fd.take()
        );

        context.run_loop();
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SidecarHost<NativeMemoryMapper> {
     pub unsafe fn new(cmd_name: &str, sig_name: &str, in_names: &[String], sc_names: &[String], out_names: &[String], efd: i32) -> Self {
         unsafe { Self::new_with_mapper(NativeMemoryMapper, cmd_name, sig_name, in_names, sc_names, out_names, efd) }
     }
}

impl SidecarHost<WasmMemoryMapper> {
     /// Creates a new SidecarHost specifically for the WASM environment using raw pointers.
     /// The WASM host passes integer pointers into the linear memory for the shared structures.
     pub unsafe fn new_wasm(cmd_ptr: usize, sig_ptr: usize, in_ptrs: &[usize], sc_ptrs: &[usize], out_ptrs: &[usize]) -> Self {
         let in_names: Vec<String> = in_ptrs.iter().map(|p| p.to_string()).collect();
         let sc_names: Vec<String> = sc_ptrs.iter().map(|p| p.to_string()).collect();
         let out_names: Vec<String> = out_ptrs.iter().map(|p| p.to_string()).collect();
         unsafe { Self::new_with_mapper(WasmMemoryMapper, &cmd_ptr.to_string(), &sig_ptr.to_string(), &in_names, &sc_names, &out_names, -1) }
     }
}

/// Interface for handling sidecar-specific extensions (Opaque Envelopes).
pub trait SidecarExtensionHandler: Send {
    fn handle_extension(&mut self, processor: &mut dyn AudioProcessor, envelope: &nullherz_traits::OpaqueEnvelope);
}

pub struct SidecarContext<'a, P: AudioProcessor> {
    processor: P,
    extension_handler: Option<Box<dyn SidecarExtensionHandler>>,
    command_buffer: &'a ShmRingBuffer<nullherz_traits::TimestampedCommand>,
    midi_buffer: Option<&'a ShmRingBuffer<nullherz_traits::MidiEvent>>,
    #[allow(dead_code)]
    feedback_buffer: Option<&'a ShmRingBuffer<nullherz_traits::ProcessorMetadata>>,
    input_buffers: Vec<&'a ShmRingBuffer<AudioBlock>>,
    sidechain_buffers: Vec<&'a ShmRingBuffer<AudioBlock>>,
    output_buffers: Vec<&'a ShmRingBuffer<AudioBlock>>,
    signal: &'a ShmSignal,
    event_fd: Option<EventFd>,
}

impl<'a, P: AudioProcessor> SidecarContext<'a, P> {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(
        processor: P,
        shm_cmd: &'a SharedMemory,
        shm_signal: &'a SharedMemory,
        shm_inputs: &'a [SharedMemory],
        shm_sidechains: &'a [SharedMemory],
        shm_outputs: &'a [SharedMemory],
        event_fd: Option<EventFd>,
    ) -> Self {
        Self::new_with_mapper(&NativeMemoryMapper, processor, shm_cmd, shm_signal, shm_inputs, shm_sidechains, shm_outputs, event_fd)
    }

    pub fn new_with_mapper<M: MemoryMapper>(
        mapper: &M,
        processor: P,
        shm_cmd: &'a M::Mapping,
        shm_signal: &'a M::Mapping,
        shm_inputs: &'a [M::Mapping],
        shm_sidechains: &'a [M::Mapping],
        shm_outputs: &'a [M::Mapping],
        event_fd: Option<EventFd>,
    ) -> Self {
        let command_buffer = unsafe { &*(mapper.ptr(shm_cmd) as *const ShmRingBuffer<nullherz_traits::TimestampedCommand>) };
        let signal = unsafe { &*(mapper.ptr(shm_signal) as *const ShmSignal) };
        let mut input_buffers = Vec::new();
        for shm in shm_inputs {
            input_buffers.push(unsafe { &*(mapper.ptr(shm) as *const ShmRingBuffer<AudioBlock>) });
        }
        let mut sidechain_buffers = Vec::new();
        for shm in shm_sidechains {
            sidechain_buffers.push(unsafe { &*(mapper.ptr(shm) as *const ShmRingBuffer<AudioBlock>) });
        }
        let mut output_buffers = Vec::new();
        for shm in shm_outputs {
            output_buffers.push(unsafe { &*(mapper.ptr(shm) as *const ShmRingBuffer<AudioBlock>) });
        }

        Self {
            processor,
            extension_handler: None,
            command_buffer,
            midi_buffer: None,
            feedback_buffer: None,
            input_buffers,
            sidechain_buffers,
            output_buffers,
            signal,
            event_fd,
        }
    }

    pub fn with_extension_handler(mut self, handler: Box<dyn SidecarExtensionHandler>) -> Self {
        self.extension_handler = Some(handler);
        self
    }

    pub fn process_once(&mut self) {
        self.signal.pulse_heartbeat();

        if let Some(midi_rb) = self.midi_buffer {
            while let Some(event) = midi_rb.pop() {
                self.processor.apply_midi(event, None);
            }
        }

        while let Some(ts_cmd) = self.command_buffer.pop() {
            match &ts_cmd.command {
                nullherz_traits::Command::Extension(envelope) => {
                    if let Some(handler) = &mut self.extension_handler {
                        handler.handle_extension(&mut self.processor, envelope);
                    } else {
                        self.processor.apply_command(&ts_cmd.command);
                    }
                }
                _ => self.processor.apply_command(&ts_cmd.command),
            }
        }

        let mut in_blocks = [AudioBlock { data: [0.0; ipc_layer::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] }; 16];
        let mut sc_blocks = [AudioBlock { data: [0.0; ipc_layer::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] }; 16];
        let mut out_blocks = [AudioBlock { data: [0.0; ipc_layer::MAX_BLOCK_SIZE], len: 0, _pad: [0; 15] }; 16];
        let num_channels = self.input_buffers.len().min(self.output_buffers.len()).min(16);
        let num_sidechains = self.sidechain_buffers.len().min(16);

        let mut available = true;
        for (i, in_buffer) in self.input_buffers.iter().enumerate().take(num_channels) {
            if let Some(block) = in_buffer.pop() {
                in_blocks[i] = block;
            } else {
                available = false;
                break;
            }
        }

        if available {
            for (i, sc_buffer) in self.sidechain_buffers.iter().enumerate().take(num_sidechains) {
                if let Some(block) = sc_buffer.pop() {
                    sc_blocks[i] = block;
                } else {
                    // Sidechains are optional, but if we expect them we might want to wait.
                    // For now assume optional.
                }
            }
        }

        if available && num_channels > 0 {
            let block_len = in_blocks[0].len as usize;
            let mut in_slices_arr: [&[f32]; 32] = [&[]; 32];
            for i in 0..num_channels { in_slices_arr[i] = &in_blocks[i].data[..block_len]; }
            for i in 0..num_sidechains { in_slices_arr[num_channels + i] = &sc_blocks[i].data[..block_len]; }

            let mut out_data_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
            for i in 0..num_channels {
                out_data_ptrs[i] = out_blocks[i].data.as_mut_ptr();
            }

            let mut context = ProcessContext {
                transport: None,
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };

            // SAFETY: We reconstruct the mutable slices from raw pointers to satisfy the borrow checker,
            // as we know each channel's buffer is distinct within the out_blocks array.
            let mut out_slices_arr: [&mut [f32]; 16] = std::array::from_fn(|i| {
                if !out_data_ptrs[i].is_null() {
                    unsafe { std::slice::from_raw_parts_mut(out_data_ptrs[i], block_len) }
                } else {
                    &mut [][..]
                }
            });

            // Call processor once with ALL channels and sidechains
            self.processor.process(
                &in_slices_arr[..num_channels + num_sidechains],
                &mut out_slices_arr[..num_channels],
                &mut context
            );

            for i in 0..num_channels {
                out_blocks[i].len = block_len as u32;
                let _ = self.output_buffers[i].push(out_blocks[i]);
            }
        }
    }

    pub fn run_loop(&mut self) {
        loop {
            if let Some(efd) = &self.event_fd {
                // Hardened: handle potential EventFD counter overflow or starvation
                let count = efd.wait().min(32); // Process at most 32 blocks per wakeup to prevent starvation
                for _ in 0..count {
                    self.process_once();
                }
            } else {
                if !self.signal.check_and_clear() {
                    std::thread::yield_now();
                    continue;
                }
                self.process_once();
            }
        }
    }
}

pub struct DnaKernel;

impl DnaKernel {
    pub fn apply_spectral_personality(output: &mut [f32], input: &[f32], dna: &nullherz_traits::SoundDNA, bias: f32) {
        use audio_dsp::simd_vec::{FloatX16, load_f32x16, store_f32x16};

        let n = output.len().min(input.len());
        let latent = dna.spectral.latent_space;

        // Target gain derived from latent space, clamped to [0, 1]
        let mut target_arr = [0.0f32; 16];
        for i in 0..16 { target_arr[i] = latent[i].max(0.0).min(1.0); }

        let v_target = FloatX16::new(target_arr);
        let v_bias = FloatX16::splat(bias);
        let v_inv_bias = FloatX16::splat(1.0 - bias);

        // current_gain = (1.0 * (1.0 - bias)) + (target_gain * bias)
        let v_gain = v_inv_bias + (v_target * v_bias);

        for bin in (0..n).step_by(16).filter(|&b| b + 16 <= n) {
            let v_in = load_f32x16(input, bin);
            let v_out = v_in * v_gain;
            store_f32x16(output, bin, v_out);
        }

        // Scalar fallback for remaining samples
        for i in (n - (n % 16))..n {
            let dim = i % 16;
            let target_gain = latent[dim].max(0.0).min(1.0);
            let current_gain = (1.0 - bias) + (target_gain * bias);
            output[i] = input[i] * current_gain;
        }
    }

    pub fn apply_rhythmic_offset(samples: &mut [f32], dna: &nullherz_traits::SoundDNA, sample_rate: f32, step: usize) {
        // Hardened: Re-utilizing robust rhythmic grid logic for Layer 3 micro-timing
        Self::apply_rhythmic_grid(samples, dna, sample_rate, step);
    }

    /// RhythmicGrid: High-performance micro-timing utility for sidecars.
    /// Applies rhythmic jitter to an entire audio block using linear interpolation for sub-sample accuracy.
    pub fn apply_rhythmic_grid(samples: &mut [f32], dna: &nullherz_traits::SoundDNA, sample_rate: f32, step: usize) {
        let micro_offset_ms = dna.rhythmic.micro_timing[step % 12] as f32;
        let delay_samples_f = micro_offset_ms * sample_rate * 0.001;

        if delay_samples_f.abs() < 0.001 { return; }

        let mut buffer = [0.0f32; 1024];
        let len = samples.len().min(1024);
        buffer[..len].copy_from_slice(&samples[..len]);

        let int_delay = delay_samples_f.floor() as i32;
        let frac = delay_samples_f - delay_samples_f.floor();

        for i in 0..len {
            let read_pos = i as i32 - int_delay;
            let sample = if read_pos >= 1 && read_pos < len as i32 {
                // Linear Interpolation: y = y0 * (1 - frac) + y1 * frac
                buffer[read_pos as usize - 1] * frac + buffer[read_pos as usize] * (1.0 - frac)
            } else {
                0.0
            };
            samples[i] = sample;
        }
    }

    /// SpectralWarp: Non-linear frequency shifter using Stage 6 SoundDNA latent space.
    /// Accelerated by WASM SIMD FloatX16 pathways.
    pub fn apply_spectral_warp(re: &mut [f32], im: &mut [f32], dna: &nullherz_traits::SoundDNA, warp_strength: f32) {
        use audio_dsp::simd_vec::{FloatX16, load_f32x16, store_f32x16};

        let n = re.len();
        let latent = dna.spectral.latent_space;

        for bin in (0..n).step_by(16).filter(|&b| b + 16 <= n) {
            let v_re = load_f32x16(re, bin);
            let v_im = load_f32x16(im, bin);

            // Warp factor derived from latent space dimensions 0-15
            let v_warp = FloatX16::new(latent);
            let v_strength = FloatX16::splat(warp_strength);

            // Non-linear perturbation: re = re * (1 + warp*strength), im = im * (1 - warp*strength)
            let v_res_re = v_re * (FloatX16::splat(1.0) + v_warp * v_strength);
            let v_res_im = v_im * (FloatX16::splat(1.0) - v_warp * v_strength);

            store_f32x16(re, bin, v_res_re);
            store_f32x16(im, bin, v_res_im);
        }
    }
}

/// Example of how to handle Opaque Envelope extensions in a Sidecar.
pub fn handle_extension(processor: &mut dyn AudioProcessor, envelope: &nullherz_traits::OpaqueEnvelope) {
    // Domain 0x53444B31 is "SDK1"
    if envelope.domain_id == 0x53444B31 {
        match envelope.opcode {
            0x01 => {
                // Custom SDK Command implementation
            }
            _ => {}
        }
    }
    // Fallback to standard processor command handling if needed
    processor.apply_command(&nullherz_traits::Command::Extension(*envelope));
}
