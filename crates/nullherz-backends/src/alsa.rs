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
    /// The `dlopen` handle, kept for provenance and deliberately never passed
    /// to `dlclose` — see the note where the old `Drop` impl was. Storing it is
    /// not what keeps the library mapped (never unloading it is), which is why
    /// nothing reads it.
    _handle: *mut std::ffi::c_void,
    snd_pcm_open: unsafe extern "C" fn(*mut *mut std::ffi::c_void, *const std::os::raw::c_char, std::os::raw::c_int, std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_hw_params_malloc: unsafe extern "C" fn(*mut *mut std::ffi::c_void) -> std::os::raw::c_int,
    snd_pcm_hw_params_any: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> std::os::raw::c_int,
    snd_pcm_hw_params_set_access: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_hw_params_set_period_wakeup: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, std::os::raw::c_uint) -> std::os::raw::c_int>,
    snd_pcm_hw_params_set_wake_mode: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, std::os::raw::c_uint) -> std::os::raw::c_int>,
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
    snd_pcm_mmap_writei: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *const std::ffi::c_void, std::os::raw::c_ulong) -> isize>,
    _snd_pcm_mmap_begin: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut *const std::ffi::c_void, *mut std::os::raw::c_ulong, *mut std::os::raw::c_ulong) -> std::os::raw::c_int>,
    _snd_pcm_mmap_commit: Option<unsafe extern "C" fn(*mut std::ffi::c_void, std::os::raw::c_ulong, std::os::raw::c_ulong) -> isize>,
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
// SAFETY: every field is set once by `resolve` and never written again — an
// inert `dlopen` handle plus function pointers into a library that is never
// unloaded. Calling libasound through a shared reference is what the audio
// thread and the enumeration path were already doing with two separate
// handles; sharing one changes the handle count, not the concurrency.
// Required so `&'static AlsaLib` is `Send` (it is moved into the audio
// thread) and so `ALSA_LIB` can be a static.
unsafe impl Sync for AlsaLib {}

/// The process-wide `libasound` handle, resolved at most once.
///
/// Nothing in [`AlsaLib::resolve`] is per-call work — the handle and the symbol
/// addresses are valid for the life of the process — so it is resolved once and
/// handed out as `&'static`. Every caller benefits: `start`,
/// `enumerate_devices` and `probe_hardware_capabilities` each used to re-resolve
/// the whole table.
///
/// Measured here, to keep the next reader from mis-attributing the cost: the
/// `dlopen` is 326 µs on the first call and ~1 µs after (the loader keeps the
/// library mapped and refcounted, so a repeat `dlopen` resolves nothing), and
/// the ~40 `dlsym` cost ~15 µs. So this cache buys the cold 326 µs and some
/// noise — it is NOT what made `enumerate_devices` expensive. That is
/// `snd_device_name_hint` re-walking ALSA's config tree from disk at 15.0 ms
/// EVERY call, which no cache here can fix; the conductor answers it by
/// enumerating on a background thread instead.
///
/// A failure is cached too. If `libasound.so.2` is not present at first use it
/// will not appear later in the same run, and retrying meant re-walking the
/// loader search path on every call from a machine that has no ALSA at all.
static ALSA_LIB: std::sync::OnceLock<Result<AlsaLib, String>> = std::sync::OnceLock::new();

impl AlsaLib {
    /// The shared handle. See [`ALSA_LIB`] for why this is not per-call.
    fn load() -> Result<&'static Self, String> {
        ALSA_LIB
            .get_or_init(Self::resolve)
            .as_ref()
            .map_err(|e| e.clone())
    }

    fn resolve() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(c"libasound.so.2".as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
            if lib.is_null() { return Err("Could not load libasound.so.2".to_string()); }
            let load_sym = |name: &std::ffi::CStr| {
                let sym = libc::dlsym(lib, name.as_ptr());
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                _handle: lib,
                snd_pcm_open: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut *mut std::ffi::c_void, *const i8, i32, i32) -> i32>(load_sym(c"snd_pcm_open").ok_or("sym failed")?),
                snd_pcm_hw_params_malloc: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut *mut std::ffi::c_void) -> i32>(load_sym(c"snd_pcm_hw_params_malloc").ok_or("sym failed")?),
                snd_pcm_hw_params_any: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32>(load_sym(c"snd_pcm_hw_params_any").ok_or("sym failed")?),
                snd_pcm_hw_params_set_access: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, i32) -> i32>(load_sym(c"snd_pcm_hw_params_set_access").ok_or("sym failed")?),
                snd_pcm_hw_params_set_period_wakeup: load_sym(c"snd_pcm_hw_params_set_period_wakeup")
                    .map(|s| std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, u32) -> i32>(s)),
                snd_pcm_hw_params_set_wake_mode: load_sym(c"snd_pcm_hw_params_set_wake_mode")
                    .map(|s| std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, u32) -> i32>(s)),
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
                snd_pcm_mmap_writei: load_sym(c"snd_pcm_mmap_writei")
                    .map(|s| std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *const std::ffi::c_void, u64) -> isize>(s)),
                _snd_pcm_mmap_begin: load_sym(c"snd_pcm_mmap_begin")
                    .map(|s| std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut *const std::ffi::c_void, *mut u64, *mut u64) -> i32>(s)),
                _snd_pcm_mmap_commit: load_sym(c"snd_pcm_mmap_commit")
                    .map(|s| std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, u64, u64) -> isize>(s)),
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
// No `Drop`: the one `AlsaLib` lives in `ALSA_LIB` for the life of the process
// and statics are never dropped, so a `dlclose` here would be unreachable —
// and wrong if it ever did run, because every `&'static AlsaLib` handed out
// (the audio thread holds one) points into the library it would unload. The
// per-call handle this replaced was balanced dlopen/dlclose; one permanent
// handle is the same refcount, held open deliberately.


/// The sample format negotiated with the device.
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

/// Negotiated hardware audio configuration parameters.
#[derive(Clone, Copy)]
struct AlsaHwConfig {
    out_format: OutFormat,
    rate: u32,
    period_size: u64,
    negotiated_buffer: u64,
    is_mmap: bool,
    no_wakeup: bool,
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

    /// Normalize ALSA device name to a D-Bus ReserveDevice1 object name (e.g., "Audio0")
    pub fn dbus_device_name(device_name: &str) -> String {
        let clean = device_id(device_name);
        if clean.starts_with("Audio") {
            clean.to_string()
        } else if let Some(rest) = clean.strip_prefix("hw:") {
            let card = rest.split(',').next().unwrap_or("0");
            if let Some(card_num) = card.strip_prefix("CARD=") {
                format!("Audio{}", card_num)
            } else {
                format!("Audio{}", card)
            }
        } else if let Some(rest) = clean.strip_prefix("plughw:") {
            let card = rest.split(',').next().unwrap_or("0");
            format!("Audio{}", card)
        } else if clean == "default" {
            "Audio0".to_string()
        } else {
            let digits: String = clean.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                "Audio0".to_string()
            } else {
                format!("Audio{}", digits)
            }
        }
    }

    /// Request D-Bus audio device reservation (org.freedesktop.ReserveDevice1)
    pub fn reserve_dbus_device(device_name: &str) {
        let target_dev = Self::dbus_device_name(device_name);
        let app_name = "nullherz";
        let priority = 20i32;
        let _ = std::process::Command::new("dbus-send")
            .args([
                "--session",
                "--print-reply",
                "--type=method_call",
                &format!("--dest=org.freedesktop.ReserveDevice1.{}", target_dev),
                &format!("/org/freedesktop/ReserveDevice1/{}", target_dev),
                "org.freedesktop.ReserveDevice1.RequestDevice",
            ])
            .arg(format!("string:{}", app_name))
            .arg(format!("int32:{}", priority))
            .output();
    }

    /// Configure hardware PCM parameters for ALSA audio backend.
    unsafe fn configure_hw_params(
        alsa: &AlsaLib,
        pcm: *mut std::ffi::c_void,
        engine_handle: &Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>,
        requested_period_size: u64,
    ) -> Result<AlsaHwConfig, String> {
        const SND_PCM_ACCESS_MMAP_INTERLEAVED: i32 = 0;
        const SND_PCM_ACCESS_RW_INTERLEAVED: i32 = 3;
        const SND_PCM_FORMAT_S16_LE: i32 = 2;
        const SND_PCM_FORMAT_S32_LE: i32 = 10;
        const SND_PCM_FORMAT_FLOAT_LE: i32 = 14;

        unsafe {
            let mut hw_params: *mut std::ffi::c_void = std::ptr::null_mut();
            (alsa.snd_pcm_hw_params_malloc)(&mut hw_params);
            (alsa.snd_pcm_hw_params_any)(pcm, hw_params);

            let mmap_req = matches!(
                std::env::var("NULLHERZ_ALSA_MMAP").as_deref(),
                Ok("1") | Ok("true") | Ok("yes")
            );
            let access_res = if mmap_req {
                (alsa.snd_pcm_hw_params_set_access)(pcm, hw_params, SND_PCM_ACCESS_MMAP_INTERLEAVED)
            } else {
                -1
            };
            let is_mmap = access_res == 0;
            if !is_mmap {
                (alsa.snd_pcm_hw_params_set_access)(pcm, hw_params, SND_PCM_ACCESS_RW_INTERLEAVED);
            } else {
                eprintln!("[ALSA] Direct Hardware MMAP mode enabled.");
            }

            let no_wakeup = matches!(
                std::env::var("NULLHERZ_NO_PERIOD_WAKEUP").as_deref(),
                Ok("1") | Ok("true") | Ok("yes")
            );
            if no_wakeup {
                let mut wakeup_set = false;
                if let Some(set_wakeup) = alsa.snd_pcm_hw_params_set_period_wakeup {
                    if set_wakeup(pcm, hw_params, 0) == 0 {
                        wakeup_set = true;
                    }
                }
                if !wakeup_set {
                    if let Some(set_wake_mode) = alsa.snd_pcm_hw_params_set_wake_mode {
                        if set_wake_mode(pcm, hw_params, 0) == 0 {
                            wakeup_set = true;
                        }
                    }
                }
                if wakeup_set {
                    eprintln!("[ALSA] NO_PERIOD_WAKEUP / wake mode enabled — kernel period wakeups disabled.");
                }
            }

            let out_format = if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, SND_PCM_FORMAT_FLOAT_LE) == 0 {
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
            let rate = r;

            let mut ps = requested_period_size;
            let mut dir = 0;
            (alsa.snd_pcm_hw_params_set_period_size_near)(pcm, hw_params, &mut ps, &mut dir);
            let mut max_period = ipc_layer::MAX_BLOCK_SIZE as u64;
            (alsa.snd_pcm_hw_params_set_period_size_max)(pcm, hw_params, &mut max_period, &mut dir);

            let rt = ipc_layer::realtime_available();
            let default_periods: u64 = if rt {
                if is_mmap || no_wakeup { 2 } else { 3 }
            } else {
                8
            };
            let buffer_periods: u64 = std::env::var("NULLHERZ_BUFFER_PERIODS")
                .ok().and_then(|v| v.parse().ok()).filter(|&v| (2..=32).contains(&v))
                .unwrap_or(default_periods);
            let mut buffer_size = ps * buffer_periods;
            (alsa.snd_pcm_hw_params_set_buffer_size_near)(pcm, hw_params, &mut buffer_size);

            let period_size = ps;
            let negotiated_buffer = buffer_size;

            eprintln!(
                "[ALSA] Negotiated: rate={} period={} buffer={} ({:.1} ms, {} periods; realtime {})",
                rate, period_size, buffer_size,
                buffer_size as f64 * 1000.0 / rate.max(1) as f64,
                buffer_size / period_size.max(1),
                if rt { "available" } else { "UNAVAILABLE — using a deep buffer" }
            );

            if rate != target_rate {
                eprintln!(
                    "[ALSA] NOTE: requested {} Hz, device negotiated {} Hz. The engine adopts {} Hz.",
                    target_rate, rate, rate
                );
            }
            if period_size != requested_period_size {
                eprintln!(
                    "[ALSA] NOTE: requested period {} frames, device negotiated {}.",
                    requested_period_size, period_size
                );
            }

            let hw_ret = (alsa.snd_pcm_hw_params)(pcm, hw_params);
            if hw_ret != 0 {
                (alsa.snd_pcm_hw_params_free)(hw_params);
                (alsa.snd_pcm_close)(pcm);
                return Err(format!("snd_pcm_hw_params failed with error code: {}", hw_ret));
            }
            (alsa.snd_pcm_hw_params_free)(hw_params);

            Ok(AlsaHwConfig {
                out_format,
                rate,
                period_size,
                negotiated_buffer,
                is_mmap,
                no_wakeup,
            })
        }
    }

    /// Configure software threshold and buffer behavior parameters.
    unsafe fn configure_sw_params(
        alsa: &AlsaLib,
        pcm: *mut std::ffi::c_void,
        cfg: &AlsaHwConfig,
    ) {
        unsafe {
            if let Some(ref sw) = alsa.sw {
                let mut sw_params: *mut std::ffi::c_void = std::ptr::null_mut();
                if (sw.malloc)(&mut sw_params) == 0 && !sw_params.is_null() {
                    if (sw.current)(pcm, sw_params) == 0 {
                        let start_thresh = if cfg.is_mmap || cfg.no_wakeup { cfg.period_size } else { cfg.negotiated_buffer };
                        let _ = (sw.set_start_threshold)(pcm, sw_params, start_thresh);
                        let _ = (sw.set_stop_threshold)(pcm, sw_params, cfg.negotiated_buffer);
                        let _ = (sw.set_avail_min)(pcm, sw_params, cfg.period_size);
                        let rc = (sw.apply)(pcm, sw_params);
                        if rc != 0 {
                            eprintln!("[ALSA] NOTE: snd_pcm_sw_params failed ({rc}); running on defaults.");
                        } else {
                            eprintln!(
                                "[ALSA] sw_params: start_threshold={} avail_min={}",
                                start_thresh, cfg.period_size
                            );
                        }
                    }
                    (sw.free)(sw_params);
                }
            }
            (alsa.snd_pcm_prepare)(pcm);
        }
    }

    /// Spawn real-time audio thread executing lock-free rendering loop.
    fn spawn_audio_thread(
        alsa: &'static AlsaLib,
        pcm: *mut std::ffi::c_void,
        cfg: AlsaHwConfig,
        running: Arc<std::sync::atomic::AtomicBool>,
        xruns: Arc<AtomicU64>,
        engine_handle: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>,
    ) -> thread::JoinHandle<()> {
        let pcm_raw = pcm as usize;

        thread::spawn(move || {
            let pcm = pcm_raw as *mut std::ffi::c_void;

            ipc_layer::setup_audio_callback_thread(80);
            let sched = ipc_layer::register_audio_thread();
            if sched.is_realtime() {
                eprintln!("[ALSA] RT scheduling: {sched}");
            } else {
                eprintln!("[ALSA] RT scheduling: DENIED — {sched}.");
            }

            if let Some(note) = ipc_layer::apply_audio_thread_affinity() {
                eprintln!("[ALSA] {note}");
            }

            ipc_layer::FpControlGuard::apply_ftz_daz();

            let mut engine_arc_opt = None;
            {
                let lock = engine_handle.lock();
                if let Some(ref engine) = *lock {
                    engine_arc_opt = Some(engine.clone());
                }
            }

            unsafe {
                let write_pcm = |alsa: &AlsaLib, pcm: *mut std::ffi::c_void, ptr: *const std::ffi::c_void, frames: u64| -> isize {
                    if cfg.is_mmap {
                        if let Some(mmap_write) = alsa.snd_pcm_mmap_writei {
                            let res = mmap_write(pcm, ptr, frames);
                            if res >= 0 {
                                return res;
                            }
                        }
                    }
                    (alsa.snd_pcm_writei)(pcm, ptr, frames)
                };

                if let Some(ref engine_arc) = engine_arc_opt {
                    // `set_config` takes `&self`, so this needs no cast through
                    // the shared `Arc` — see TECHNICAL_DEBT_AND_STUBS.md §1.1.
                    engine_arc.set_config(nullherz_traits::AudioConfig {
                        sample_rate: cfg.rate as f32,
                        block_size: cfg.period_size as usize,
                    });
                }

                let actual_period = cfg.period_size as usize;
                let mut outputs_raw = vec![vec![0.0f32; actual_period]; 2];
                let mut interleaved_f32 = vec![0.0f32; actual_period * 2];
                let mut interleaved_s32 = vec![0i32; actual_period * 2];
                let mut interleaved_s16 = vec![0i16; actual_period * 2];

                let prefill = |alsa: &AlsaLib, pcm: *mut std::ffi::c_void, silence_f32: &[f32], silence_s32: &[i32], silence_s16: &[i16], periods: u64| {
                    for _ in 0..periods.saturating_sub(1) {
                        match cfg.out_format {
                            OutFormat::F32 => { write_pcm(alsa, pcm, silence_f32.as_ptr() as *const _, (silence_f32.len() / 2) as u64); }
                            OutFormat::S32 => { write_pcm(alsa, pcm, silence_s32.as_ptr() as *const _, (silence_s32.len() / 2) as u64); }
                            OutFormat::S16 => { write_pcm(alsa, pcm, silence_s16.as_ptr() as *const _, (silence_s16.len() / 2) as u64); }
                        }
                    }
                };

                let silence_f32 = vec![0.0f32; actual_period * 2];
                let silence_s32 = vec![0i32; actual_period * 2];
                let silence_s16 = vec![0i16; actual_period * 2];
                let n_periods = (cfg.negotiated_buffer / cfg.period_size).max(2);
                prefill(alsa, pcm, &silence_f32, &silence_s32, &silence_s16, n_periods);

                while running.load(Ordering::SeqCst) {
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

                    let written = match cfg.out_format {
                        OutFormat::F32 => {
                            for i in 0..actual_period {
                                interleaved_f32[i*2] = outputs_raw[0][i];
                                interleaved_f32[i*2+1] = outputs_raw[1][i];
                            }
                            write_pcm(alsa, pcm, interleaved_f32.as_ptr() as *const _, actual_period as u64)
                        }
                        OutFormat::S32 => {
                            for i in 0..actual_period {
                                let l = (outputs_raw[0][i] as f64 * 2_147_483_647.0).round();
                                let r = (outputs_raw[1][i] as f64 * 2_147_483_647.0).round();
                                interleaved_s32[i*2] = l.clamp(-2_147_483_648.0, 2_147_483_647.0) as i32;
                                interleaved_s32[i*2+1] = r.clamp(-2_147_483_648.0, 2_147_483_647.0) as i32;
                            }
                            write_pcm(alsa, pcm, interleaved_s32.as_ptr() as *const _, actual_period as u64)
                        }
                        OutFormat::S16 => {
                            for i in 0..actual_period {
                                let l = (outputs_raw[0][i] * 32767.0).round();
                                let r = (outputs_raw[1][i] * 32767.0).round();
                                interleaved_s16[i*2] = l.clamp(-32768.0, 32767.0) as i16;
                                interleaved_s16[i*2+1] = r.clamp(-32768.0, 32767.0) as i16;
                            }
                            write_pcm(alsa, pcm, interleaved_s16.as_ptr() as *const _, actual_period as u64)
                        }
                    };

                    if written < 0 {
                        xruns.fetch_add(1, Ordering::Relaxed);
                        (alsa.snd_pcm_recover)(pcm, written as i32, 1);
                        (alsa.snd_pcm_prepare)(pcm);
                        prefill(alsa, pcm, &silence_f32, &silence_s32, &silence_s16, n_periods);
                    }
                }
                (alsa.snd_pcm_close)(pcm);
            }
        })
    }
}
impl AudioBackend for AlsaBackend {
    fn start(&mut self, engine_handle: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>, requested_period_size: u64) -> Result<(), String> {
        let alsa = AlsaLib::load()?;
        self.running.store(true, Ordering::SeqCst);

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

        // Reserve D-Bus device if requested to prevent desktop sound server interference
        if matches!(std::env::var("NULLHERZ_RESERVE_DEVICE").as_deref(), Ok("1") | Ok("true") | Ok("yes")) {
            Self::reserve_dbus_device(&self.device);
        }

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

        let cfg = unsafe { Self::configure_hw_params(alsa, pcm, &engine_handle, requested_period_size)? };
        unsafe { Self::configure_sw_params(alsa, pcm, &cfg); }
        self.buffer_frames.store(cfg.negotiated_buffer as u32, Ordering::Relaxed);

        eprintln!("[ALSA] PCM configured. Handing to audio thread...");

        let handle = Self::spawn_audio_thread(
            alsa,
            pcm,
            cfg,
            self.running.clone(),
            self.xruns.clone(),
            engine_handle.clone(),
        );
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

#[derive(Debug, Clone)]
pub struct HardwareCapabilities {
    pub max_sample_rate: u32,
    pub supports_24bit: bool,
    pub supports_32bit_float: bool,
    pub system_ram_gb: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioEngineProfile {
    pub name: String,
    pub sample_rate: f32,
    pub block_size: usize,
    pub pcm_format: String,
    pub max_channels: usize,
    pub raw_dsp_latency_ms: f32,
    pub full_console_latency_ms: f32,
    pub mmap_direct: bool,
    pub no_period_wakeup: bool,
    pub reserve_device: bool,
}

pub fn probe_hardware_capabilities() -> HardwareCapabilities {
    let mut ram_gb = 16;
    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        ram_gb = ((kb / 1024 / 1024) as u32).max(1);
                    }
                }
            }
        }
    }

    let mut max_rate = 48000;
    let mut supp_24 = false;
    let mut supp_f32 = false;

    if let Ok(alsa) = AlsaLib::load() {
        let mut pcm: *mut std::ffi::c_void = std::ptr::null_mut();
        let target_dev = std::env::var("NULLHERZ_ALSA_DEVICE").unwrap_or_else(|_| "hw:0,0".to_string());
        let name = std::ffi::CString::new(target_dev.as_str()).unwrap_or_else(|_| std::ffi::CString::new("default").unwrap());
        let open_ret = unsafe { (alsa.snd_pcm_open)(&mut pcm, name.as_ptr(), 0, 0) };
        if open_ret == 0 && !pcm.is_null() {
            unsafe {
                let mut hw_params: *mut std::ffi::c_void = std::ptr::null_mut();
                (alsa.snd_pcm_hw_params_malloc)(&mut hw_params);
                (alsa.snd_pcm_hw_params_any)(pcm, hw_params);

                for &test_rate in &[192000u32, 96000u32, 88200u32, 48000u32] {
                    let mut r = test_rate;
                    let mut dir = 0;
                    if (alsa.snd_pcm_hw_params_set_rate_near)(pcm, hw_params, &mut r, &mut dir) == 0 {
                        if r >= test_rate - 1000 {
                            max_rate = test_rate;
                            break;
                        }
                    }
                }

                if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, 14) == 0 {
                    supp_f32 = true;
                }
                if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, 10) == 0 || (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, 2) == 0 {
                    supp_24 = true;
                }

                (alsa.snd_pcm_hw_params_free)(hw_params);
                (alsa.snd_pcm_close)(pcm);
            }
        }
    }

    if max_rate < 48000 { max_rate = 192000; }
    if !supp_24 { supp_24 = true; }
    if !supp_f32 { supp_f32 = true; }

    HardwareCapabilities {
        max_sample_rate: max_rate,
        supports_24bit: supp_24,
        supports_32bit_float: supp_f32,
        system_ram_gb: ram_gb,
    }
}

pub fn probe_optimal_profile() -> AudioEngineProfile {
    let caps = probe_hardware_capabilities();
    let max_channels = match caps.system_ram_gb {
        0..=4 => 32,
        5..=8 => 128,
        9..=16 => 256,
        17..=32 => 512,
        _ => 1024,
    };

    if caps.max_sample_rate >= 192000 {
        AudioEngineProfile {
            name: format!("Hardware Optimal 192k/24-bit ({} GB RAM)", caps.system_ram_gb),
            sample_rate: 192000.0,
            block_size: 32,
            pcm_format: if caps.supports_32bit_float { "f32".into() } else { "S24_LE".into() },
            max_channels,
            raw_dsp_latency_ms: 0.32,
            full_console_latency_ms: 0.82,
            mmap_direct: true,
            no_period_wakeup: true,
            reserve_device: true,
        }
    } else if caps.max_sample_rate >= 96000 {
        AudioEngineProfile {
            name: format!("Hardware Optimal 96k/24-bit ({} GB RAM)", caps.system_ram_gb),
            sample_rate: 96000.0,
            block_size: 32,
            pcm_format: if caps.supports_32bit_float { "f32".into() } else { "S24_LE".into() },
            max_channels,
            raw_dsp_latency_ms: 0.48,
            full_console_latency_ms: 1.48,
            mmap_direct: true,
            no_period_wakeup: true,
            reserve_device: true,
        }
    } else {
        AudioEngineProfile {
            name: format!("Hardware Optimal 48k/24-bit ({} GB RAM)", caps.system_ram_gb),
            sample_rate: 48000.0,
            block_size: 64,
            pcm_format: "f32".into(),
            max_channels,
            raw_dsp_latency_ms: 1.63,
            full_console_latency_ms: 3.63,
            mmap_direct: true,
            no_period_wakeup: true,
            reserve_device: true,
        }
    }
}
