use crate::engine::AudioEngine;
use crate::graph::SpaBuffer;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::thread;

pub trait AudioBackend {
    fn start(&mut self, engine: AudioEngine) -> Result<(), String>;
    fn stop(&mut self) -> Option<AudioEngine>;
}

pub struct ThreadedBackend {
    handle: Option<thread::JoinHandle<Option<AudioEngine>>>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl ThreadedBackend {
    pub fn new() -> Self { Self { handle: None, running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)) } }
}
impl AudioBackend for ThreadedBackend {
    fn start(&mut self, mut engine: AudioEngine) -> Result<(), String> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let handle = thread::spawn(move || {
            let mut outputs_raw = [[0.0f32; 128]; 2];
            let interval = Duration::from_secs_f64(128.0 / 44100.0);
            while running.load(Ordering::SeqCst) {
                let start = std::time::Instant::now();
                let (ch1, ch2) = outputs_raw.split_at_mut(1);
                let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];
                engine.process_block(&[], &mut out_refs, 128);
                let elapsed = start.elapsed();
                if elapsed < interval { thread::sleep(interval - elapsed); }
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
            unsafe {
                let mut pcm: *mut std::ffi::c_void = std::ptr::null_mut();
                let name = std::ffi::CString::new("default").unwrap();
                if (alsa.snd_pcm_open)(&mut pcm, name.as_ptr(), 0, 0) != 0 { return Some(engine); }
                if (alsa.snd_pcm_set_params)(pcm, 2, 3, 2, 44100, 1, 5000) != 0 { (alsa.snd_pcm_close)(pcm); return Some(engine); }
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
                Some(engine)
            }
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

pub struct PipewireBackend {
    pub(crate) running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) loop_ptr: *mut std::ffi::c_void,
    pub(crate) stream_ptr: *mut std::ffi::c_void,
    pub(crate) context_ptr: *mut std::ffi::c_void,
    pub(crate) pw: Option<PwLib>,
    pub(crate) user_data: *mut PwUserData,
}

unsafe impl Send for PipewireBackend {}

impl PipewireBackend {
    pub fn new() -> Self {
        Self {
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            loop_ptr: std::ptr::null_mut(),
            stream_ptr: std::ptr::null_mut(),
            context_ptr: std::ptr::null_mut(),
            pw: None,
            user_data: std::ptr::null_mut(),
        }
    }
}

#[repr(C)]
pub struct SpaData {
    pub type_id: u32,
    pub flags: u32,
    pub fd: i64,
    pub mapoffset: u32,
    pub maxsize: u32,
    pub data: *mut std::ffi::c_void,
    pub chunk: *mut SpaChunk,
}

#[repr(C)]
pub struct SpaChunk {
    pub offset: u32,
    pub size: u32,
    pub stride: i32,
    pub flags: u32,
}

#[repr(C)]
pub struct PwStreamEvents {
    pub version: u32,
    pub destroy: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    pub state_changed: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32, *const i8)>,
    pub control_info: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, *const std::ffi::c_void)>,
    pub io_changed: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, *mut std::ffi::c_void, u32)>,
    pub param_changed: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32, *const std::ffi::c_void)>,
    pub add_buffer: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void)>,
    pub remove_buffer: Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void)>,
    pub process: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    pub drained: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
}

pub(crate) struct PwLib {
    pub(crate) _handle: *mut std::ffi::c_void,
    pub(crate) pw_init: unsafe extern "C" fn(*mut i32, *mut *mut *mut i8),
    pub(crate) pw_thread_loop_new: unsafe extern "C" fn(*const i8, *const std::ffi::c_void) -> *mut std::ffi::c_void,
    pub(crate) pw_thread_loop_start: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
    pub(crate) pw_thread_loop_stop: unsafe extern "C" fn(*mut std::ffi::c_void),
    pub(crate) pw_thread_loop_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pub(crate) pw_context_new: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void,
    pub(crate) pw_context_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pub(crate) pw_core_connect: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void,
    pub(crate) pw_stream_new: unsafe extern "C" fn(*mut std::ffi::c_void, *const i8, *mut std::ffi::c_void) -> *mut std::ffi::c_void,
    pub(crate) pw_stream_add_listener: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *const PwStreamEvents, *mut std::ffi::c_void),
    pub(crate) pw_stream_connect: unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32, u32, *const *const std::ffi::c_void, u32) -> i32,
    pub(crate) pw_stream_dequeue_buffer: unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut SpaBuffer,
    pub(crate) pw_stream_queue_buffer: unsafe extern "C" fn(*mut std::ffi::c_void, *mut SpaBuffer) -> i32,
    pub(crate) pw_stream_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
}

impl PwLib {
    pub(crate) fn load() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(b"libpipewire-0.3.so.0\0".as_ptr() as *const _, libc::RTLD_NOW);
            if lib.is_null() { return Err("Could not load libpipewire-0.3.so.0".to_string()); }
            let load_sym = |name: &[u8]| {
                let sym = libc::dlsym(lib, name.as_ptr() as *const _);
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                _handle: lib,
                pw_init: std::mem::transmute(load_sym(b"pw_init\0").ok_or("pw_init failed")?),
                pw_thread_loop_new: std::mem::transmute(load_sym(b"pw_thread_loop_new\0").ok_or("pw_thread_loop_new failed")?),
                pw_thread_loop_start: std::mem::transmute(load_sym(b"pw_thread_loop_start\0").ok_or("pw_thread_loop_start failed")?),
                pw_thread_loop_stop: std::mem::transmute(load_sym(b"pw_thread_loop_stop\0").ok_or("pw_thread_loop_stop failed")?),
                pw_thread_loop_destroy: std::mem::transmute(load_sym(b"pw_thread_loop_destroy\0").ok_or("pw_thread_loop_destroy failed")?),
                pw_context_new: std::mem::transmute(load_sym(b"pw_context_new\0").ok_or("pw_context_new failed")?),
                pw_context_destroy: std::mem::transmute(load_sym(b"pw_context_destroy\0").ok_or("pw_context_destroy failed")?),
                pw_core_connect: std::mem::transmute(load_sym(b"pw_core_connect\0").ok_or("pw_core_connect failed")?),
                pw_stream_new: std::mem::transmute(load_sym(b"pw_stream_new\0").ok_or("pw_stream_new failed")?),
                pw_stream_add_listener: std::mem::transmute(load_sym(b"pw_stream_add_listener\0").ok_or("pw_stream_add_listener failed")?),
                pw_stream_connect: std::mem::transmute(load_sym(b"pw_stream_connect\0").ok_or("pw_stream_connect failed")?),
                pw_stream_dequeue_buffer: std::mem::transmute(load_sym(b"pw_stream_dequeue_buffer\0").ok_or("pw_stream_dequeue_buffer failed")?),
                pw_stream_queue_buffer: std::mem::transmute(load_sym(b"pw_stream_queue_buffer\0").ok_or("pw_stream_queue_buffer failed")?),
                pw_stream_destroy: std::mem::transmute(load_sym(b"pw_stream_destroy\0").ok_or("pw_stream_destroy failed")?),
            })
        }
    }
}

pub(crate) struct PwUserData {
    pub(crate) engine: AudioEngine,
    pub(crate) pw: PwLib,
    pub(crate) stream_ptr: *mut std::ffi::c_void,
}

unsafe extern "C" fn on_stream_destroy(_data: *mut std::ffi::c_void) {}

unsafe extern "C" fn on_stream_process(data: *mut std::ffi::c_void) {
    let ud = &mut *(data as *mut PwUserData);
    let buffer = (ud.pw.pw_stream_dequeue_buffer)(ud.stream_ptr);
    if buffer.is_null() { return; }
    let spa_buf = &*buffer;
    if spa_buf.n_datas > 0 {
        let data = &*spa_buf.datas;
        if !data.data.is_null() {
            let num_samples = 128;
            let target = std::slice::from_raw_parts_mut(data.data as *mut f32, num_samples * 2);
            let mut ch1 = [0.0f32; 128];
            let mut ch2 = [0.0f32; 128];
            {
                let mut out_refs = [&mut ch1[..], &mut ch2[..]];
                ud.engine.process_block(&[], &mut out_refs, num_samples);
            }
            for i in 0..num_samples {
                target[i*2] = ch1[i];
                target[i*2+1] = ch2[i];
            }
            (*data.chunk).size = (num_samples * 2 * 4) as u32;
            (*data.chunk).offset = 0;
            (*data.chunk).stride = 8;
        }
    }
    (ud.pw.pw_stream_queue_buffer)(ud.stream_ptr, buffer);
}

impl AudioBackend for PipewireBackend {
    fn start(&mut self, engine: AudioEngine) -> Result<(), String> {
        let pw = PwLib::load()?;
        self.running.store(true, Ordering::SeqCst);
        unsafe {
            (pw.pw_init)(std::ptr::null_mut(), std::ptr::null_mut());
            self.loop_ptr = (pw.pw_thread_loop_new)(b"nullherz-loop\0".as_ptr() as *const i8, std::ptr::null_mut());
            self.context_ptr = (pw.pw_context_new)(self.loop_ptr, std::ptr::null_mut(), 0);
            self.stream_ptr = (pw.pw_stream_new)(self.context_ptr, b"nullherz-stream\0".as_ptr() as *const i8, std::ptr::null_mut());
            let user_data = Box::into_raw(Box::new(PwUserData { engine, pw: PwLib::load()?, stream_ptr: self.stream_ptr }));
            self.user_data = user_data;
            let events = Box::into_raw(Box::new(PwStreamEvents {
                version: 1,
                destroy: Some(on_stream_destroy),
                state_changed: None,
                control_info: None,
                io_changed: None,
                param_changed: None,
                add_buffer: None,
                remove_buffer: None,
                process: Some(on_stream_process),
                drained: None,
            }));
            (pw.pw_stream_add_listener)(self.stream_ptr, std::ptr::null_mut(), events as *const _, user_data as *mut _);
            (pw.pw_thread_loop_start)(self.loop_ptr);
        }
        self.pw = Some(pw);
        Ok(())
    }
    fn stop(&mut self) -> Option<AudioEngine> {
        self.running.store(false, Ordering::SeqCst);
        let mut engine = None;
        if let Some(pw) = &self.pw {
            unsafe {
                if !self.loop_ptr.is_null() { (pw.pw_thread_loop_stop)(self.loop_ptr); }
                if !self.user_data.is_null() {
                    let ud = Box::from_raw(self.user_data);
                    engine = Some(ud.engine);
                }
                if !self.stream_ptr.is_null() { (pw.pw_stream_destroy)(self.stream_ptr); }
                if !self.context_ptr.is_null() { (pw.pw_context_destroy)(self.context_ptr); }
                if !self.loop_ptr.is_null() { (pw.pw_thread_loop_destroy)(self.loop_ptr); }
            }
        }
        engine
    }
}
