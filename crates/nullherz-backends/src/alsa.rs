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

pub struct AlsaBackend {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    /// Lock-free xrun (buffer-underrun) counter. Incremented on the audio
    /// thread in place of a blocking `eprintln!` — RT-safe observability that
    /// never issues a `write(2)` on the SCHED_FIFO callback.
    xruns: std::sync::Arc<AtomicU64>,
    /// ALSA device to open. Defaults to `$NULLHERZ_ALSA_DEVICE` or `"default"`.
    device: String,
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
        const SND_PCM_FORMAT_FLOAT_LE: i32 = 14;

        let (is_float, rate, period_size, negotiated_buffer);

        unsafe {
            let mut hw_params: *mut std::ffi::c_void = std::ptr::null_mut();
            (alsa.snd_pcm_hw_params_malloc)(&mut hw_params);
            (alsa.snd_pcm_hw_params_any)(pcm, hw_params);
            (alsa.snd_pcm_hw_params_set_access)(pcm, hw_params, SND_PCM_ACCESS_RW_INTERLEAVED);

            let mut float_ok = true;
            if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, SND_PCM_FORMAT_FLOAT_LE) != 0 {
                float_ok = false;
                if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, SND_PCM_FORMAT_S16_LE) != 0 {
                    (alsa.snd_pcm_hw_params_free)(hw_params);
                    (alsa.snd_pcm_close)(pcm);
                    return Err("Neither FLOAT_LE nor S16_LE format accepted".to_string());
                }
            }
            is_float = float_ok;
            eprintln!("[ALSA] Format: {}", if is_float { "FLOAT_LE" } else { "S16_LE" });

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
            // Buffer depth = scheduling slack. period*4 (~23ms at 256/44k1) is
            // not enough on a loaded 2-core desktop (measured: 411 underruns in
            // 18s under stress); default to 8 periods (~46ms), overridable via
            // NULLHERZ_BUFFER_PERIODS for low-latency setups with RT privileges.
            let buffer_periods: u64 = std::env::var("NULLHERZ_BUFFER_PERIODS")
                .ok().and_then(|v| v.parse().ok()).filter(|&v| (2..=32).contains(&v)).unwrap_or(8);
            let mut buffer_size = ps * buffer_periods;
            (alsa.snd_pcm_hw_params_set_buffer_size_near)(pcm, hw_params, &mut buffer_size);
            period_size = ps;

            negotiated_buffer = buffer_size;
            eprintln!(
                "[ALSA] Negotiated: rate={} period={} buffer={} ({:.1} ms)",
                rate, period_size, buffer_size,
                buffer_size as f64 * 1000.0 / rate.max(1) as f64
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
            match ipc_layer::set_rt_priority(80) {
                Ok(()) => eprintln!("[ALSA] RT scheduling: ACQUIRED (SCHED_FIFO direct or SCHED_RR via RTKit)"),
                Err(_) => eprintln!("[ALSA] RT scheduling: DENIED — running at normal priority. Underruns likely under load. Fix: add '@audio - rtprio 95' to /etc/security/limits.d/audio.conf and re-login."),
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
                let mut interleaved_s16 = vec![0i16; actual_period * 2];

                // Pre-fill the device buffer with silence: starting (or
                // recovering) with a full buffer of slack instead of one
                // period is what breaks the endless underrun-recover loop.
                let prefill = |alsa: &AlsaLib, pcm: *mut std::ffi::c_void, silence_f32: &[f32], silence_s16: &[i16], periods: u64| {
                    for _ in 0..periods.saturating_sub(1) {
                        if is_float {
                            (alsa.snd_pcm_writei)(pcm, silence_f32.as_ptr() as *const _, (silence_f32.len() / 2) as u64);
                        } else {
                            (alsa.snd_pcm_writei)(pcm, silence_s16.as_ptr() as *const _, (silence_s16.len() / 2) as u64);
                        }
                    }
                };
                let silence_f32 = vec![0.0f32; actual_period * 2];
                let silence_s16 = vec![0i16; actual_period * 2];
                let n_periods = (negotiated_buffer / period_size).max(2);
                prefill(&alsa, pcm, &silence_f32, &silence_s16, n_periods);
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

                    let written = if is_float {
                        for i in 0..actual_period {
                            interleaved_f32[i*2] = outputs_raw[0][i];
                            interleaved_f32[i*2+1] = outputs_raw[1][i];
                        }
                        (alsa.snd_pcm_writei)(pcm, interleaved_f32.as_ptr() as *const _, actual_period as u64)
                    } else {
                        for i in 0..actual_period {
                            interleaved_s16[i*2] = (outputs_raw[0][i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
                            interleaved_s16[i*2+1] = (outputs_raw[1][i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        }
                        (alsa.snd_pcm_writei)(pcm, interleaved_s16.as_ptr() as *const _, actual_period as u64)
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
                        prefill(&alsa, pcm, &silence_f32, &silence_s16, n_periods);
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
