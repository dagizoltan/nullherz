use std::thread;
use std::sync::atomic::Ordering;
use crate::engine::AudioEngine;
use crate::backends::AudioBackend;

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
}
unsafe impl Send for AlsaLib {}

impl AlsaLib {
    fn load() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(c"libasound.so.2".as_ptr(), libc::RTLD_NOW);
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
                snd_pcm_writei: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *const libc::c_void, u64) -> isize>(load_sym(c"snd_pcm_writei").ok_or("sym failed")?),
                snd_pcm_recover: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, i32, i32) -> i32>(load_sym(c"snd_pcm_recover").ok_or("sym failed")?),
                snd_pcm_close: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void) -> i32>(load_sym(c"snd_pcm_close").ok_or("sym failed")?),
                snd_pcm_prepare: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void) -> i32>(load_sym(c"snd_pcm_prepare").ok_or("sym failed")?),
            })
        }
    }
}
impl Drop for AlsaLib { fn drop(&mut self) { unsafe { libc::dlclose(self.handle); } } }

pub struct AlsaBackend {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<Option<AudioEngine>>>,
}

impl Default for AlsaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AlsaBackend {
    pub fn new() -> Self { Self { running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), handle: None } }
}
impl AudioBackend for AlsaBackend {
    fn start(&mut self, mut engine: AudioEngine) -> Result<(), String> {
        let alsa = AlsaLib::load()?;
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let handle = thread::spawn(move || {
            crate::setup_rt_thread(90, Some(0)); // Pin main RT thread to core 0
            unsafe {
                let mut pcm: *mut std::ffi::c_void = std::ptr::null_mut();
                let name = std::ffi::CString::new("default").unwrap();
                if (alsa.snd_pcm_open)(&mut pcm, name.as_ptr(), 0, 0) != 0 { return None; }

                const SND_PCM_ACCESS_RW_INTERLEAVED: i32 = 3;
                const SND_PCM_FORMAT_S16_LE: i32 = 2;
                const SND_PCM_FORMAT_FLOAT_LE: i32 = 14;

                let mut hw_params: *mut std::ffi::c_void = std::ptr::null_mut();
                (alsa.snd_pcm_hw_params_malloc)(&mut hw_params);
                (alsa.snd_pcm_hw_params_any)(pcm, hw_params);
                (alsa.snd_pcm_hw_params_set_access)(pcm, hw_params, SND_PCM_ACCESS_RW_INTERLEAVED);

                // Format MUST be set before rate/period to satisfy ALSA constraints properly
                let mut is_float = true;
                if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, SND_PCM_FORMAT_FLOAT_LE) != 0 {
                    is_float = false;
                    if (alsa.snd_pcm_hw_params_set_format)(pcm, hw_params, SND_PCM_FORMAT_S16_LE) != 0 {
                        (alsa.snd_pcm_hw_params_free)(hw_params);
                        (alsa.snd_pcm_close)(pcm);
                        return None;
                    }
                }

                (alsa.snd_pcm_hw_params_set_channels)(pcm, hw_params, 2);

                let mut rate = engine.target_sample_rate as u32;
                (alsa.snd_pcm_hw_params_set_rate_near)(pcm, hw_params, &mut rate, std::ptr::null_mut());

                if (rate as f32 - engine.target_sample_rate).abs() > 0.1 {
                    eprintln!("ALSA: Failed to negotiate target rate {}. Got {}.", engine.target_sample_rate, rate);
                    (alsa.snd_pcm_hw_params_free)(hw_params);
                    (alsa.snd_pcm_close)(pcm);
                    return None;
                }

                let mut period_size = 128u64;
                let mut dir = 0;
                (alsa.snd_pcm_hw_params_set_period_size_near)(pcm, hw_params, &mut period_size, &mut dir);

                // Constrain period to engine max block size to prevent overflow
                let mut max_period = ipc_layer::MAX_BLOCK_SIZE as u64;
                (alsa.snd_pcm_hw_params_set_period_size_max)(pcm, hw_params, &mut max_period, &mut dir);

                let mut buffer_size = period_size * 4;
                (alsa.snd_pcm_hw_params_set_buffer_size_near)(pcm, hw_params, &mut buffer_size);

                if (alsa.snd_pcm_hw_params)(pcm, hw_params) != 0 {
                    (alsa.snd_pcm_hw_params_free)(hw_params);
                    (alsa.snd_pcm_close)(pcm);
                    return None;
                }
                (alsa.snd_pcm_hw_params_free)(hw_params);
                (alsa.snd_pcm_prepare)(pcm);

                engine.set_config(crate::AudioConfig {
                    sample_rate: rate as f32,
                    block_size: period_size as usize,
                });

                let mut outputs_raw = [[0.0f32; ipc_layer::MAX_BLOCK_SIZE]; 2];
                let mut interleaved_f32 = [0.0f32; ipc_layer::MAX_BLOCK_SIZE * 2];
                let mut interleaved_s16 = [0i16; ipc_layer::MAX_BLOCK_SIZE * 2];

                let actual_period = period_size as usize;
                while running.load(Ordering::SeqCst) {
                    let (ch1, ch2) = outputs_raw.split_at_mut(1);
                    let mut out_refs = [&mut ch1[0][..actual_period], &mut ch2[0][..actual_period]];
                    engine.process_block(&[], &mut out_refs, actual_period);

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
                        if written == -32 { // EPIPE (Xrun)
                            engine.xrun_counter().fetch_add(1, Ordering::Relaxed);
                        }
                        (alsa.snd_pcm_recover)(pcm, written as i32, 1);
                        (alsa.snd_pcm_prepare)(pcm);
                    }
                }
                (alsa.snd_pcm_close)(pcm);
            }
            Some(engine)
        });
        self.handle = Some(handle);
        Ok(())
    }
    fn stop(&mut self) -> Option<AudioEngine> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap_or(None)
        } else {
            None
        }
    }
}
