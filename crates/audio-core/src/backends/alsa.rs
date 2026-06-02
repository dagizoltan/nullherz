use std::thread;
use std::sync::atomic::Ordering;
use crate::engine::AudioEngine;
use crate::backends::AudioBackend;

struct AlsaLib {
    handle: *mut std::ffi::c_void,
    snd_pcm_open: unsafe extern "C" fn(*mut *mut std::ffi::c_void, *const std::os::raw::c_char, std::os::raw::c_int, std::os::raw::c_int) -> std::os::raw::c_int,
    snd_pcm_set_params: unsafe extern "C" fn(*mut std::ffi::c_void, std::os::raw::c_int, std::os::raw::c_int, std::os::raw::c_uint, std::os::raw::c_uint, std::os::raw::c_int, std::os::raw::c_uint) -> std::os::raw::c_int,
    snd_pcm_writei: unsafe extern "C" fn(*mut std::ffi::c_void, *const std::ffi::c_void, std::os::raw::c_ulong) -> isize,
    snd_pcm_close: unsafe extern "C" fn(*mut std::ffi::c_void) -> std::os::raw::c_int,
}
unsafe impl Send for AlsaLib {}

impl AlsaLib {
    fn load() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(b"libasound.so.2\0".as_ptr() as *const _, libc::RTLD_NOW);
            if lib.is_null() { return Err("Could not load libasound.so.2".to_string()); }
            let load_sym = |name: &[u8]| {
                let sym = libc::dlsym(lib, name.as_ptr() as *const _);
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                handle: lib,
                snd_pcm_open: std::mem::transmute(load_sym(b"snd_pcm_open\0").ok_or("sym failed")?),
                snd_pcm_set_params: std::mem::transmute(load_sym(b"snd_pcm_set_params\0").ok_or("sym failed")?),
                snd_pcm_writei: std::mem::transmute(load_sym(b"snd_pcm_writei\0").ok_or("sym failed")?),
                snd_pcm_close: std::mem::transmute(load_sym(b"snd_pcm_close\0").ok_or("sym failed")?),
            })
        }
    }
}
impl Drop for AlsaLib { fn drop(&mut self) { unsafe { libc::dlclose(self.handle); } } }

pub struct AlsaBackend {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<thread::JoinHandle<Option<AudioEngine>>>,
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
            let _ = ipc_layer::set_rt_priority(90);
            unsafe {
                let mut pcm: *mut std::ffi::c_void = std::ptr::null_mut();
                let name = std::ffi::CString::new("default").unwrap();
                if (alsa.snd_pcm_open)(&mut pcm, name.as_ptr(), 0, 0) != 0 { return None; }
                if (alsa.snd_pcm_set_params)(pcm, 2, 3, 2, 44100, 1, 5000) != 0 { (alsa.snd_pcm_close)(pcm); return None; }
                let mut outputs_raw = [[0.0f32; 128]; 2];
                let mut interleaved = [0i16; 256];
                while running.load(Ordering::SeqCst) {
                    let (ch1, ch2) = outputs_raw.split_at_mut(1);
                    let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];
                    engine.process_block(&[], &mut out_refs, 128);
                    for i in 0..128 {
                        let sample_l = (outputs_raw[0][i] * 32767.0).clamp(-32768.0, 32767.0);
                        let sample_r = (outputs_raw[1][i] * 32767.0).clamp(-32768.0, 32767.0);
                        interleaved[i*2] = sample_l as i16;
                        interleaved[i*2+1] = sample_r as i16;
                    }
                    (alsa.snd_pcm_writei)(pcm, interleaved.as_ptr() as *const _, 128);
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
