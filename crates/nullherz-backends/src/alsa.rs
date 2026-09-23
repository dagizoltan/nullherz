// Non-RT plane (device recovery backoff): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
use std::thread;
use std::sync::Arc;
use parking_lot::Mutex;
use std::sync::atomic::{Ordering, AtomicU64};
use nullherz_traits::RenderingEngine;
use crate::AudioBackend;

struct AlsaLib {
    handle: *mut std::ffi::c_void,
    snd_pcm_open: unsafe extern "C" fn(*mut *mut std::ffi::c_void, *const std::os::raw::c_char, std::os::raw::c_int, std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_hw_params_malloc: unsafe extern "C" fn(*mut *mut std::ffi::c_void) -> std::os::raw::c_int,
    snd_pcm_hw_params_any: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> std::os::raw::c_int,
    snd_pcm_hw_params_set_access: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_hw_params_set_format: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_hw_params_set_channels: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, std::os::raw::c_uint) -> std::os::raw::c_int,
    snd_pcm_hw_params_set_rate_near: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::os::raw::c_uint, *mut std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_hw_params_set_period_size_near: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::os::raw::c_ulong, *mut std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_hw_params_set_buffer_size_near: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::os::raw::c_ulong) -> std::os::raw::c_int,
    snd_pcm_hw_params_set_period_size_max: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *mut std::os::raw::c_ulong, *mut std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_hw_params: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> std::os::raw::c_int,
    snd_pcm_hw_params_free: unsafe extern "C" fn(*mut std::ffi::c_void),
    // SOFTWARE params. Optional as a GROUP: if a stripped libasound lacks any
    // of them the stream still plays on ALSA's defaults, which is what happened
    // before this was wired at all. Same reasoning as the device-hint API below
    // — never fail playback over a tuning knob.
    sw: Option<AlsaSwParams>,
    snd_pcm_writei: unsafe extern "C" fn(*mut std::ffi::c_void, *const std::ffi::c_void, std::os::raw::c_ulong) -> isize,
    snd_pcm_recover: unsafe extern "C" fn(*mut std::ffi::c_void, std::os::raw::c_int, std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_close: unsafe extern "C" fn(*mut std::ffi::c_void) -> std::os::raw::c_int,
    snd_pcm_prepare: unsafe extern "C" fn(*mut std::ffi::c_void) -> std::os::raw::c_int,
    // Device discovery. Optional because the PCM path — the part that actually
    // makes sound — must not fail to load just because a stripped libasound is
    // missing the hint API. Absence degrades enumeration to ["default"], which
    // is honest; it never degrades playback.
    snd_device_name_hint: Option<unsafe extern "C" fn(std::os::raw::c_int, *const std::os::raw::c_char, *mut *mut *mut std::ffi::c_void) -> std::os::raw::c_int>,
    snd_device_name_get_hint: Option<unsafe extern "C" fn(*const std::ffi::c_void, *const std::os::raw::c_char) -> *mut std::os::raw::c_char>,
    snd_device_name_free_hint: Option<unsafe extern "C" fn(*mut *mut std::ffi::c_void) -> std::os::raw::c_int>,
}
/// The `snd_pcm_sw_params_*` family, loaded or not loaded as one unit.
struct AlsaSwParams {
    malloc: unsafe extern "C" fn(*mut *mut std::ffi::c_void) -> std::os::raw::c_int,
    free: unsafe extern "C" fn(*mut std::ffi::c_void),
    current: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> std::os::raw::c_int,
    set_start_threshold: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, std::os::raw::c_ulong) -> std::os::raw::c_int,
    set_stop_threshold: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, std::os::raw::c_ulong) -> std::os::raw::c_int,
    set_avail_min: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, std::os::raw::c_ulong) -> std::os::raw::c_int,
    apply: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> std::os::raw::c_int,
}

unsafe impl Send for AlsaLib {}

impl AlsaLib {
    fn load() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(c"libasound.so.2".as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
            if lib.is_null() { return Err("Could not load libasound.so.2".to_string()); }
            let load_sym = |name: &std::ffi::CStr| {
                let sym = libc::dlsym(lib, name.as_ptr());
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                handle: lib,
                snd_pcm_open: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut *mut std::ffi::c_void, *const i8, i32, i32) -> i32>(load_sym(c"snd_pcm_open").ok_or("sym failed")?),
                snd_pcm_hw_params_malloc: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut *mut std::ffi::c_void) -> i32>(load_sym(c"snd_pcm_hw_params_malloc").ok_or("sym failed")?),
                snd_pcm_hw_params_any: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32>(load_sym(c"snd_pcm_hw_params_any").ok_or("sym failed")?),
                snd_pcm_hw_params_set_access: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, i32) -> i32>(load_sym(c"snd_pcm_hw_params_set_access").ok_or("sym failed")?),
                snd_pcm_hw_params_set_format: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, i32) -> i32>(load_sym(c"snd_pcm_hw_params_set_format").ok_or("sym failed")?),
                snd_pcm_hw_params_set_channels: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, u32) -> i32>(load_sym(c"snd_pcm_hw_params_set_channels").ok_or("sym failed")?),
                snd_pcm_hw_params_set_rate_near: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, *mut u32, *mut i32) -> i32>(load_sym(c"snd_pcm_hw_params_set_rate_near").ok_or("sym failed")?),
                snd_pcm_hw_params_set_period_size_near: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, *mut u64, *mut i32) -> i32>(load_sym(c"snd_pcm_hw_params_set_period_size_near").ok_or("sym failed")?),
                snd_pcm_hw_params_set_buffer_size_near: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, *mut u64) -> i32>(load_sym(c"snd_pcm_hw_params_set_buffer_size_near").ok_or("sym failed")?),
                snd_pcm_hw_params_set_period_size_max: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, *mut u64, *mut i32) -> i32>(load_sym(c"snd_pcm_hw_params_set_period_size_max").ok_or("sym failed")?),
                snd_pcm_hw_params: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> i32>(load_sym(c"snd_pcm_hw_params").ok_or("sym failed")?),
                snd_pcm_hw_params_free: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void)>(load_sym(c"snd_pcm_hw_params_free").ok_or("sym failed")?),
                // Already inside the enclosing `unsafe` block.
                sw: (|| {
                    Some(AlsaSwParams {
                        malloc: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut *mut libc::c_void) -> i32>(load_sym(c"snd_pcm_sw_params_malloc")?),
                        free: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void)>(load_sym(c"snd_pcm_sw_params_free")?),
                        current: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> i32>(load_sym(c"snd_pcm_sw_params_current")?),
                        set_start_threshold: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, u64) -> i32>(load_sym(c"snd_pcm_sw_params_set_start_threshold")?),
                        set_stop_threshold: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, u64) -> i32>(load_sym(c"snd_pcm_sw_params_set_stop_threshold")?),
                        set_avail_min: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, u64) -> i32>(load_sym(c"snd_pcm_sw_params_set_avail_min")?),
                        apply: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> i32>(load_sym(c"snd_pcm_sw_params")?),
                    })
                })(),
                snd_pcm_writei: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *const std::ffi::c_void, u64) -> isize>(load_sym(c"snd_pcm_writei").ok_or("sym failed")?),
                snd_pcm_recover: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, i32, i32) -> i32>(load_sym(c"snd_pcm_recover").ok_or("sym failed")?),
                snd_pcm_close: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void) -> i32>(load_sym(c"snd_pcm_close").ok_or("sym failed")?),
                snd_pcm_prepare: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void) -> i32>(load_sym(c"snd_pcm_prepare").ok_or("sym failed")?),
                snd_device_name_hint: load_sym(c"snd_device_name_hint")
                    .map(|s| std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(i32, *const i8, *mut *mut *mut std::ffi::c_void) -> i32>(s)),
                snd_device_name_get_hint: load_sym(c"snd_device_name_get_hint")
                    .map(|s| std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*const std::ffi::c_void, *const i8) -> *mut i8>(s)),
                snd_device_name_free_hint: load_sym(c"snd_device_name_free_hint")
                    .map(|s| std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut *mut std::ffi::c_void) -> i32>(s)),
            })
        }
    }
}
impl Drop for AlsaLib { fn drop(&mut self) { unsafe { libc::dlclose(self.handle); } } }


/// The sample format negotiated with the device.
///
/// Was a `bool is_float`, which could only express "float or 16-bit" — and that
/// is exactly the gap: a device offering S32_LE but not float fell through to
/// 16 bits. Three states because there are three formats.
#[derive(Clone, Copy, PartialEq)]
enum OutFormat { F32, S32, S16 }

impl OutFormat {
    fn name(self) -> &'static str {
        match self {
            OutFormat::F32 => "FLOAT_LE",
            OutFormat::S32 => "S32_LE",
            OutFormat::S16 => "S16_LE (16-bit — the engine is 32-bit float; this truncates)",
        }
    }
}

pub struct AlsaBackend {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    /// Lock-free xrun (buffer-underrun) counter. Incremented on the audio
    /// thread in place of a blocking `eprintln!` — RT-safe observability that
    /// never issues a `write(2)` on the SCHED_FIFO callback.
    xruns: std::sync::Arc<AtomicU64>,
    /// ALSA device to open. Defaults to `$NULLHERZ_ALSA_DEVICE` or `"default"`.
    device: String,
    /// Ring-buffer size the device negotiated, published for the latency
    /// readout. Zero until `start` has configured the PCM.
    buffer_frames: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl Default for AlsaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AlsaBackend {
    pub fn new() -> Self {
        Self {
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            handle: None,
            xruns: std::sync::Arc::new(AtomicU64::new(0)),
            device: Self::default_device(),
            buffer_frames: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }

    /// Where the device name comes from when nobody sets one explicitly.
    ///
    /// `default` routes through whatever the user's ALSA config points at
    /// (PipeWire, PulseAudio, or bare hardware), which is the right choice for
    /// almost everyone. The escape hatch matters for the case `default` cannot
    /// serve: bypassing the sound server to reach an interface directly.
    pub fn default_device() -> String {
        std::env::var("NULLHERZ_ALSA_DEVICE")
            .ok()
            .map(|v| device_id(&v).to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "default".to_string())
    }

    /// Open a specific ALSA device instead of `default`. Accepts either a raw
    /// device id or an entry from [`AudioBackend::enumerate_devices`].
    pub fn set_device(&mut self, device: &str) {
        let id = device_id(device);
        self.device = if id.is_empty() { "default".to_string() } else { id.to_string() };
    }

    /// The device this backend will open (or has opened).
    pub fn device(&self) -> &str { &self.device }

    /// Total ALSA xruns (buffer underruns) recovered since `start`, updated
    /// lock-free from the audio thread. Read it from any thread for metering
    /// or health checks — no blocking I/O on the RT path.
    pub fn xruns(&self) -> u64 { self.xruns.load(Ordering::Relaxed) }
}
impl AudioBackend for AlsaBackend {
    fn start(&mut self, engine_handle: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>, requested_period_size: u64) -> Result<(), String> {
        let alsa = AlsaLib::load()?;
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let xruns = self.xruns.clone();

        // =====================================================================
        // CRITICAL: Open and configure PCM on the MAIN thread.
        // PipeWire's ALSA plugin spawns internal IPC threads during snd_pcm_open.
        // When called from a spawned thread, inherited scheduling state can cause
        // a segfault inside PipeWire's initialization. Opening on the main thread
        // avoids this entirely.
        // =====================================================================
        let mut pcm: *mut std::ffi::c_void = std::ptr::null_mut();
        let name = std::ffi::CString::new(self.device.as_str())
            .map_err(|_| format!("ALSA device name contains a NUL byte: {:?}", self.device))?;

        let open_ret = unsafe { (alsa.snd_pcm_open)(&mut pcm, name.as_ptr(), 0, 0) };
        if open_ret != 0 {
            // Name the device that failed. "error code -2" against an unnamed
            // device is unactionable when the name is configurable.
            return Err(format!(
                "snd_pcm_open failed on device '{}' with error code: {} \
                 (set NULLHERZ_ALSA_DEVICE to pick another; see the device list for valid names)",
                self.device, open_ret
            ));
        }
        eprintln!("[ALSA] snd_pcm_open SUCCESS on '{}'", self.device);

        const SND_PCM_ACCESS_RW_INTERLEAVED: i32 = 3;
        const SND_PCM_FORMAT_S16_LE: i32 = 2;
        const SND_PCM_FORMAT_S32_LE: i32 = 10;
        const SND_PCM_FORMAT_FLOAT_LE: i32 = 14;

        let (out_format, rate, period_size, negotiated_buffer);

        unsafe {
            let mut hw_params: *mut std::ffi::c_void = std::ptr::null_mut();
            (alsa.snd_pcm_hw_params_malloc)(&mut hw_params);
            (alsa.snd_pcm_hw_params_any)(pcm, hw_params);
            (alsa.snd_pcm_hw_params_set_access)(pcm, hw_params, SND_PCM_ACCESS_RW_INTERLEAVED);

            // Best first. S32_LE sits between float and S16 and was MISSING —
            // the chain measures -148 dB THD+N and this handed it to a 16-bit
            // truncation whenever float was unavailable.
            //
            // That is not hypothetical: the built-in codec here (ALC257) offers
            // only S16_LE and S32_LE, so any path that reaches the hardware
            // directly — `NULLHERZ_ALSA_DEVICE=hw:x,y`, or simply no sound
            // server running — landed on 16 bits. PipeWire masked it by
            // accepting FLOAT_LE and itself converting to S32LE, so the good
            // output came from the sound server rather than from this code.
            //
            // 24 dB of dynamic range, discarded silently, on the path with the
            // FEWEST layers between the engine and the converter.
            out_format = if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, SND_PCM_FORMAT_FLOAT_LE) == 0 {
                OutFormat::F32
            } else if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, SND_PCM_FORMAT_S32_LE) == 0 {
                OutFormat::S32
            } else if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, SND_PCM_FORMAT_S16_LE) == 0 {
                OutFormat::S16
            } else {
                (alsa.snd_pcm_hw_params_free)(hw_params);
                (alsa.snd_pcm_close)(pcm);
                return Err("No supported format (tried FLOAT_LE, S32_LE, S16_LE)".to_string());
            };
            eprintln!("[ALSA] Format: {}", out_format.name());

            (alsa.snd_pcm_hw_params_set_channels)(pcm, hw_params, 2);

            let mut target_rate = nullherz_traits::DEFAULT_SAMPLE_RATE as u32;
            {
                let lock = engine_handle.lock();
                if let Some(ref engine) = *lock {
                    target_rate = engine.target_sample_rate() as u32;
                }
            }

            let mut r = target_rate;
            (alsa.snd_pcm_hw_params_set_rate_near)(pcm, hw_params, &mut r, std::ptr::null_mut());
            rate = r;

            let mut ps = requested_period_size;
            let mut dir = 0;
            (alsa.snd_pcm_hw_params_set_period_size_near)(pcm, hw_params, &mut ps, &mut dir);
            let mut max_period = ipc_layer::MAX_BLOCK_SIZE as u64;
            (alsa.snd_pcm_hw_params_set_period_size_max)(pcm, hw_params, &mut max_period, &mut dir);
            // Buffer depth = scheduling slack, so it should depend on how much
            // slack the scheduler will actually give us.
            //
            // It was a flat 8 periods — ~43 ms at 256/48k, which is SIX TIMES the
            // engine's own 7.3 ms deck-to-master latency and the dominant term in
            // the whole product. The 8 came from measuring 411 underruns in 18 s
            // at 4 periods on a loaded desktop. That measurement was real, but it
            // was taken without realtime scheduling: `setup_rt_thread` discarded
            // the result of its own `set_rt_priority` call, so nobody knew
            // whether the run that produced it had RT at all.
            //
            // So ask. `realtime_available()` probes on a throwaway thread and
            // reads the policy back from the kernel:
            //
            //   * realtime obtainable -> 3 periods (~16 ms at 256/48k). Enough to
            //     ride out a scheduling gap when we can preempt the thing causing
            //     it.
            //   * not obtainable -> 8 periods, as before. Without RT the audio
            //     thread is just another task and needs the depth.
            //
            // `NULLHERZ_BUFFER_PERIODS` still overrides either way.
            let rt = ipc_layer::realtime_available();
            let default_periods: u64 = if rt { 3 } else { 8 };
            let buffer_periods: u64 = std::env::var("NULLHERZ_BUFFER_PERIODS")
                .ok().and_then(|v| v.parse().ok()).filter(|&v| (2..=32).contains(&v))
                .unwrap_or(default_periods);
            let mut buffer_size = ps * buffer_periods;
            (alsa.snd_pcm_hw_params_set_buffer_size_near)(pcm, hw_params, &mut buffer_size);
            period_size = ps;

            negotiated_buffer = buffer_size;
            self.buffer_frames.store(buffer_size as u32, Ordering::Relaxed);
            eprintln!(
                "[ALSA] Negotiated: rate={} period={} buffer={} ({:.1} ms, {} periods; realtime {})",
                rate, period_size, buffer_size,
                buffer_size as f64 * 1000.0 / rate.max(1) as f64,
                buffer_size / period_size.max(1),
                if rt { "available" } else { "UNAVAILABLE — using a deep buffer" }
            );
            // `*_near` silently substitutes when it cannot honour a request, so
            // a mismatch is invisible unless we look. Both cases are real:
            // a rate substitution means someone is resampling us, and a period
            // substitution means every duration derived from the requested
            // period (bridge pacing, DSP-load percentages) is computed against
            // a period the device never agreed to.
            if rate != target_rate {
                eprintln!(
                    "[ALSA] NOTE: requested {} Hz, device negotiated {} Hz. The engine adopts \
                     {} Hz, so pitch is correct — but something in the chain is resampling. \
                     Matching the sound server's rate removes that stage.",
                    target_rate, rate, rate
                );
            }
            if period_size != requested_period_size {
                eprintln!(
                    "[ALSA] NOTE: requested period {} frames, device negotiated {}.",
                    requested_period_size, period_size
                );
            }
            #[cfg(debug_assertions)]
            eprintln!("[ALSA] WARNING: DEBUG build — DSP runs 10-30x slower and WILL underrun. Use --release.");

            let hw_ret = (alsa.snd_pcm_hw_params)(pcm, hw_params);
            if hw_ret != 0 {
                (alsa.snd_pcm_hw_params_free)(hw_params);
                (alsa.snd_pcm_close)(pcm);
                return Err(format!("snd_pcm_hw_params failed with error code: {}", hw_ret));
            }
            (alsa.snd_pcm_hw_params_free)(hw_params);

            // SOFTWARE params. Previously never set at all, so the stream ran on
            // ALSA's defaults for both of these.
            //
            //  * start_threshold = buffer_size: begin playing only once the ring
            //    is full. The prefill below fills it, so playback starts from a
            //    complete buffer rather than from whatever the default threshold
            //    happened to be — which is the difference between starting with
            //    full slack and starting one period from an underrun.
            //  * avail_min = period_size: wake us when exactly one period is
            //    free. The default can be larger, which coalesces wakeups and
            //    makes the write loop arrive in bursts — the opposite of what a
            //    low-latency ring wants.
            //  * stop_threshold = buffer_size: stop on a real underrun so
            //    `snd_pcm_recover` can see it, rather than letting the stream
            //    free-run into an inconsistent state.
            if let Some(ref sw) = alsa.sw {
                let mut sw_params: *mut std::ffi::c_void = std::ptr::null_mut();
                if (sw.malloc)(&mut sw_params) == 0 && !sw_params.is_null() {
                    if (sw.current)(pcm, sw_params) == 0 {
                        let _ = (sw.set_start_threshold)(pcm, sw_params, buffer_size);
                        let _ = (sw.set_stop_threshold)(pcm, sw_params, buffer_size);
                        let _ = (sw.set_avail_min)(pcm, sw_params, period_size);
                        let rc = (sw.apply)(pcm, sw_params);
                        if rc != 0 {
                            // Not fatal — the stream plays on the defaults, which
                            // is what it did before. Say so rather than leave it
                            // looking configured.
                            eprintln!("[ALSA] NOTE: snd_pcm_sw_params failed ({rc}); \
                                       running on ALSA's default start/avail thresholds.");
                        } else {
                            eprintln!(
                                "[ALSA] sw_params: start_threshold={} avail_min={}",
                                buffer_size, period_size
                            );
                        }
                    }
                    (sw.free)(sw_params);
                }
            } else {
                eprintln!("[ALSA] NOTE: libasound lacks the snd_pcm_sw_params_* API; \
                           running on default start/avail thresholds.");
            }

            (alsa.snd_pcm_prepare)(pcm);
        }

        eprintln!("[ALSA] PCM configured. Handing to audio thread...");

        // Wrap the raw PCM pointer so we can send it across thread boundaries
        let pcm_raw = pcm as usize; // usize is Send

        let handle = thread::spawn(move || {
            let pcm = pcm_raw as *mut std::ffi::c_void;

            // RT scheduling is the difference between riding out scheduler
            // gaps and drowning in them; report the outcome loudly so a
            // denied request is never mistaken for an engine problem.
            let _ = ipc_layer::set_rt_priority(80);
            // Report the policy the KERNEL gives back, not the verdict of the
            // request. `set_rt_priority` returns Ok when RTKit grants SCHED_RR at
            // priority 20 instead of the FIFO 80 asked for, so its Ok/Err told
            // you almost nothing — and the old message said "FIFO direct or RR
            // via RTKit" precisely because it could not tell which.
            let sched = ipc_layer::register_audio_thread();
            if sched.is_realtime() {
                eprintln!("[ALSA] RT scheduling: {sched}");
            } else {
                eprintln!(
                    "[ALSA] RT scheduling: DENIED — {sched}. The audio thread will be preempted \
                     by ordinary work; underruns are likely under load. Fix: add \
                     '@audio - rtprio 95' to /etc/security/limits.d/audio.conf, add yourself to \
                     that group, and re-login."
                );
            }

            // Opt-in only (NULLHERZ_AUDIO_CPU). See
            // ipc_layer::apply_audio_thread_affinity for why this is not a
            // default: without isolcpus it reserves nothing, and on an SMT
            // machine it can put the audio thread on a worker's sibling.
            if let Some(note) = ipc_layer::apply_audio_thread_affinity() {
                eprintln!("[ALSA] {note}");
            }

            // Denormal protection (FTZ/DAZ) for this audio thread. Without it,
            // signals decaying into the denormal range (filter/reverb tails,
            // releases, near-silence) fall into microcoded FP that runs 10-100x
            // slower — a CPU spike that underruns. This is our own thread, so
            // set it permanently. (setup_rt_thread applies this for the Threaded
            // backend and the worker pool; the ALSA thread takes only
            // set_rt_priority above, so it needs this explicitly.)
            ipc_layer::FpControlGuard::apply_ftz_daz();

            let mut engine_arc_opt = None;
            {
                let lock = engine_handle.lock();
                if let Some(ref engine) = *lock {
                    engine_arc_opt = Some(engine.clone());
                }
            }

            unsafe {
                if let Some(ref engine_arc) = engine_arc_opt {
                     let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn RenderingEngine;
                     (*engine_ptr).set_config(nullherz_traits::AudioConfig {
                        sample_rate: rate as f32,
                        block_size: period_size as usize,
                    });
                }

                let actual_period = period_size as usize;

                // Dynamically allocated buffers to support any ALSA period size negotiated by PipeWire or local driver
                let mut outputs_raw = vec![vec![0.0f32; actual_period]; 2];
                let mut interleaved_f32 = vec![0.0f32; actual_period * 2];
                let mut interleaved_s32 = vec![0i32; actual_period * 2];
                let mut interleaved_s16 = vec![0i16; actual_period * 2];

                // Pre-fill the device buffer with silence: starting (or
                // recovering) with a full buffer of slack instead of one
                // period is what breaks the endless underrun-recover loop.
                let prefill = |alsa: &AlsaLib, pcm: *mut std::ffi::c_void, silence_f32: &[f32], silence_s32: &[i32], silence_s16: &[i16], periods: u64| {
                    for _ in 0..periods.saturating_sub(1) {
                        match out_format {
                            OutFormat::F32 => { (alsa.snd_pcm_writei)(pcm, silence_f32.as_ptr() as *const _, (silence_f32.len() / 2) as u64); }
                            OutFormat::S32 => { (alsa.snd_pcm_writei)(pcm, silence_s32.as_ptr() as *const _, (silence_s32.len() / 2) as u64); }
                            OutFormat::S16 => { (alsa.snd_pcm_writei)(pcm, silence_s16.as_ptr() as *const _, (silence_s16.len() / 2) as u64); }
                        }
                    }
                };
                let silence_f32 = vec![0.0f32; actual_period * 2];
                let silence_s32 = vec![0i32; actual_period * 2];
                let silence_s16 = vec![0i16; actual_period * 2];
                let n_periods = (negotiated_buffer / period_size).max(2);
                prefill(&alsa, pcm, &silence_f32, &silence_s32, &silence_s16, n_periods);
                eprintln!("[ALSA] Audio thread running. period={} engine_bound={}", actual_period, engine_arc_opt.is_some());

                while running.load(Ordering::SeqCst) {
                    // Shared with the PipeWire callback so the split arithmetic
                    // is written and tested once (see `crate::chunking`).
                    for (offset, chunk_size) in crate::chunking::render_blocks(actual_period, ipc_layer::MAX_BLOCK_SIZE) {
                        if let Some(ref engine_arc) = engine_arc_opt {
                            let (ch1, ch2) = outputs_raw.split_at_mut(1);
                            let mut out_refs = [
                                &mut ch1[0][offset..offset + chunk_size],
                                &mut ch2[0][offset..offset + chunk_size],
                            ];
                            let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn RenderingEngine;
                            (*engine_ptr).process_block(&[], &mut out_refs, chunk_size);
                        } else {
                            outputs_raw[0][offset..offset + chunk_size].fill(0.0);
                            outputs_raw[1][offset..offset + chunk_size].fill(0.0);
                        }
                    }

                    // (No per-block diagnostics on the audio thread: the old
                    // periodic peak-log did a full peak scan + a blocking
                    // eprintln here every 500 blocks. Engine telemetry already
                    // carries per-node peaks for the UI; xruns are counted
                    // lock-free below.)

                    let written = match out_format {
                        OutFormat::F32 => {
                            for i in 0..actual_period {
                                interleaved_f32[i*2] = outputs_raw[0][i];
                                interleaved_f32[i*2+1] = outputs_raw[1][i];
                            }
                            (alsa.snd_pcm_writei)(pcm, interleaved_f32.as_ptr() as *const _, actual_period as u64)
                        }
                        OutFormat::S32 => {
                            // Scale by 2^31-1 and ROUND. The f32 mantissa is 24
                            // bits, so every value it can hold is representable
                            // here exactly — 32-bit integer output is lossless
                            // for this engine and needs no dither. Rounding
                            // rather than truncating because `as i32` truncates
                            // toward zero, which is a half-LSB DC bias, and the
                            // S16 arm below has carried that bug all along.
                            for i in 0..actual_period {
                                let l = (outputs_raw[0][i] as f64 * 2_147_483_647.0).round();
                                let r = (outputs_raw[1][i] as f64 * 2_147_483_647.0).round();
                                interleaved_s32[i*2] = l.clamp(-2_147_483_648.0, 2_147_483_647.0) as i32;
                                interleaved_s32[i*2+1] = r.clamp(-2_147_483_648.0, 2_147_483_647.0) as i32;
                            }
                            (alsa.snd_pcm_writei)(pcm, interleaved_s32.as_ptr() as *const _, actual_period as u64)
                        }
                        OutFormat::S16 => {
                            // ROUND, not truncate. `as i16` rounds toward zero,
                            // which is up to a full LSB of signal-correlated
                            // error with a DC bias — audible as distortion on
                            // fades, which is where 16-bit is heard.
                            //
                            // Still undithered: at 16 bits quantisation error IS
                            // correlated with the signal and TPDF dither is the
                            // fix. Not added here because this arm should now be
                            // unreachable on any device offering S32_LE, and a
                            // dither generator on the RT path deserves its own
                            // change rather than riding along with a format fix.
                            for i in 0..actual_period {
                                let l = (outputs_raw[0][i] * 32767.0).round();
                                let r = (outputs_raw[1][i] * 32767.0).round();
                                interleaved_s16[i*2] = l.clamp(-32768.0, 32767.0) as i16;
                                interleaved_s16[i*2+1] = r.clamp(-32768.0, 32767.0) as i16;
                            }
                            (alsa.snd_pcm_writei)(pcm, interleaved_s16.as_ptr() as *const _, actual_period as u64)
                        }
                    };

                    if written < 0 {
                        // RT-safe: bump a lock-free counter instead of a
                        // blocking eprintln on the SCHED_FIFO thread. Logging
                        // here would issue a write(2) at the exact moment the
                        // device is already underrunning — compounding the xrun
                        // it reports. Read the total via AlsaBackend::xruns().
                        xruns.fetch_add(1, Ordering::Relaxed);
                        (alsa.snd_pcm_recover)(pcm, written as i32, 1);
                        (alsa.snd_pcm_prepare)(pcm);
                        prefill(&alsa, pcm, &silence_f32, &silence_s32, &silence_s16, n_periods);
                    }
                }
                eprintln!("[ALSA] Audio loop exiting, closing PCM...");
                (alsa.snd_pcm_close)(pcm);
            }
        });
        self.handle = Some(handle);
        Ok(())
    }
    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// Real playback devices, queried from ALSA via `snd_device_name_hint`.
    ///
    /// This used to return a fabricated `["default", "hw:0,0"]`. That is worse
    /// than returning nothing: the UI presents the list as selectable devices,
    /// so a user could pick `hw:0,0` on a machine where it does not exist and
    /// get an open failure from a menu entry we invented. Everything below
    /// comes from the driver, and the only synthesised entry is `default`,
    /// which is guaranteed to resolve by ALSA's own configuration.
    fn xruns(&self) -> Option<u64> {
        Some(self.xruns.load(Ordering::Relaxed))
    }

    fn buffer_frames(&self) -> Option<u32> {
        match self.buffer_frames.load(Ordering::Relaxed) {
            0 => None,
            n => Some(n),
        }
    }

    fn enumerate_devices(&self) -> Vec<String> {
        let Ok(alsa) = AlsaLib::load() else { return Vec::new() };
        let (Some(hint), Some(get_hint), Some(free_hint)) = (
            alsa.snd_device_name_hint,
            alsa.snd_device_name_get_hint,
            alsa.snd_device_name_free_hint,
        ) else {
            // Hint API unavailable: say only what we can stand behind.
            return vec!["default".to_string()];
        };

        let mut devices = Vec::new();
        unsafe {
            let mut hints: *mut *mut std::ffi::c_void = std::ptr::null_mut();
            // card = -1 asks for every card rather than a specific index.
            if hint(-1, c"pcm".as_ptr(), &mut hints) != 0 || hints.is_null() {
                return vec!["default".to_string()];
            }

            // Each `snd_device_name_get_hint` result is a strdup'd C string that
            // belongs to the caller; not freeing it leaks once per device per
            // call, and this is called from the telemetry loop.
            let fetch = |h: *const std::ffi::c_void, id: &std::ffi::CStr| -> Option<String> {
                let raw = get_hint(h, id.as_ptr());
                if raw.is_null() { return None; }
                let s = std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned();
                libc::free(raw as *mut libc::c_void);
                if s == "null" || s.is_empty() { None } else { Some(s) }
            };

            let mut p = hints;
            while !(*p).is_null() {
                let h = *p as *const std::ffi::c_void;
                p = p.add(1);

                // IOID is absent for duplex devices and "Input"/"Output"
                // otherwise. Capture-only devices cannot be a playback target.
                if let Some(ioid) = fetch(h, c"IOID")
                    && ioid != "Output" { continue; }
                let Some(name) = fetch(h, c"NAME") else { continue };

                // The description's first line is the human-readable card name;
                // the rest is verbose sub-device detail nobody reads in a list.
                match fetch(h, c"DESC") {
                    Some(desc) => {
                        let short = desc.lines().next().unwrap_or("").trim().to_string();
                        if short.is_empty() || short == name {
                            devices.push(name);
                        } else {
                            devices.push(format!("{name} — {short}"));
                        }
                    }
                    None => devices.push(name),
                }
            }
            free_hint(hints);
        }

        // `default` is not always emitted as a hint but always resolves, and it
        // is what we open when nothing is configured — so it must be offerable.
        if !devices.iter().any(|d| d == "default" || d.starts_with("default —")) {
            devices.insert(0, "default".to_string());
        }
        devices
    }
}

/// Strip the human-readable suffix that [`enumerate_devices`] appends, so a
/// string taken straight from the device list can be handed to `snd_pcm_open`.
pub fn device_id(entry: &str) -> &str {
    match entry.split_once(" — ") {
        Some((id, _)) => id.trim(),
        None => entry.trim(),
    }
}
