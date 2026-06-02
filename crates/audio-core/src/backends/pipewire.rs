use std::sync::atomic::Ordering;
use crate::engine::AudioEngine;
use crate::backends::AudioBackend;

struct PwLib {
    handle: *mut std::ffi::c_void,
    pw_init: unsafe extern "C" fn(*mut i32, *mut *mut *mut i8),
    pw_thread_loop_new: unsafe extern "C" fn(*const i8, *const std::ffi::c_void) -> *mut std::ffi::c_void,
    pw_thread_loop_start: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
    pw_thread_loop_stop: unsafe extern "C" fn(*mut std::ffi::c_void),
    pw_thread_loop_get_loop: unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void,
    pw_context_new: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void,
    pw_core_connect: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void,
    pw_stream_new: unsafe extern "C" fn(*mut std::ffi::c_void, *const i8, *mut std::ffi::c_void) -> *mut std::ffi::c_void,
    pw_stream_add_listener: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *const std::ffi::c_void, *mut std::ffi::c_void),
    pw_stream_connect: unsafe extern "C" fn(*mut std::ffi::c_void, i32, u32, u32, *const std::ffi::c_void, u32) -> i32,
    pw_stream_update_params: unsafe extern "C" fn(*mut std::ffi::c_void, *mut *const std::ffi::c_void, u32) -> i32,
    pw_stream_dequeue_buffer: unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void,
    pw_stream_queue_buffer: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    pw_stream_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pw_context_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pw_thread_loop_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
}

impl PwLib {
    fn load() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(b"libpipewire-0.3.so.0\0".as_ptr() as *const _, libc::RTLD_NOW);
            if lib.is_null() { return Err("Could not load libpipewire-0.3.so.0".to_string()); }
            let load_sym = |name: &[u8]| {
                let sym = libc::dlsym(lib, name.as_ptr() as *const _);
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                handle: lib,
                pw_init: std::mem::transmute(load_sym(b"pw_init\0").ok_or("pw_init failed")?),
                pw_thread_loop_new: std::mem::transmute(load_sym(b"pw_thread_loop_new\0").ok_or("pw_thread_loop_new failed")?),
                pw_thread_loop_start: std::mem::transmute(load_sym(b"pw_thread_loop_start\0").ok_or("pw_thread_loop_start failed")?),
                pw_thread_loop_stop: std::mem::transmute(load_sym(b"pw_thread_loop_stop\0").ok_or("pw_thread_loop_stop failed")?),
                pw_thread_loop_get_loop: std::mem::transmute(load_sym(b"pw_thread_loop_get_loop\0").ok_or("pw_thread_loop_get_loop failed")?),
                pw_context_new: std::mem::transmute(load_sym(b"pw_context_new\0").ok_or("pw_context_new failed")?),
                pw_core_connect: std::mem::transmute(load_sym(b"pw_core_connect\0").ok_or("pw_core_connect failed")?),
                pw_stream_new: std::mem::transmute(load_sym(b"pw_stream_new\0").ok_or("pw_stream_new failed")?),
                pw_stream_add_listener: std::mem::transmute(load_sym(b"pw_stream_add_listener\0").ok_or("pw_stream_add_listener failed")?),
                pw_stream_connect: std::mem::transmute(load_sym(b"pw_stream_connect\0").ok_or("pw_stream_connect failed")?),
                pw_stream_update_params: std::mem::transmute(load_sym(b"pw_stream_update_params\0").ok_or("pw_stream_update_params failed")?),
                pw_stream_dequeue_buffer: std::mem::transmute(load_sym(b"pw_stream_dequeue_buffer\0").ok_or("pw_stream_dequeue_buffer failed")?),
                pw_stream_queue_buffer: std::mem::transmute(load_sym(b"pw_stream_queue_buffer\0").ok_or("pw_stream_queue_buffer failed")?),
                pw_stream_destroy: std::mem::transmute(load_sym(b"pw_stream_destroy\0").ok_or("pw_stream_destroy failed")?),
                pw_context_destroy: std::mem::transmute(load_sym(b"pw_context_destroy\0").ok_or("pw_context_destroy failed")?),
                pw_thread_loop_destroy: std::mem::transmute(load_sym(b"pw_thread_loop_destroy\0").ok_or("pw_thread_loop_destroy failed")?),
            })
        }
    }
}

pub struct PipewireBackend {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread_loop: *mut std::ffi::c_void,
    context: *mut std::ffi::c_void,
    stream: *mut std::ffi::c_void,
    engine: Option<AudioEngine>,
    lib: Option<PwLib>,
    events: Option<Box<PwStreamEvents>>,
}

unsafe impl Send for PipewireBackend {}

impl PipewireBackend {
    pub fn new() -> Self {
        Self {
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            thread_loop: std::ptr::null_mut(),
            context: std::ptr::null_mut(),
            stream: std::ptr::null_mut(),
            engine: None,
            lib: None,
            events: None,
        }
    }
}

#[repr(C)]
pub struct PwStreamEvents {
    pub version: u32,
    pub destroy: Option<unsafe extern "C" fn(data: *mut std::ffi::c_void)>,
    pub state_changed: Option<unsafe extern "C" fn(data: *mut std::ffi::c_void, old: i32, state: i32, error: *const i8)>,
    pub control_info: Option<unsafe extern "C" fn(data: *mut std::ffi::c_void, id: u32, control: *mut std::ffi::c_void)>,
    pub io_changed: Option<unsafe extern "C" fn(data: *mut std::ffi::c_void, id: u32, area: *mut std::ffi::c_void, size: u32)>,
    pub param_changed: Option<unsafe extern "C" fn(data: *mut std::ffi::c_void, id: u32, param: *const std::ffi::c_void)>,
    pub add_buffer: Option<unsafe extern "C" fn(data: *mut std::ffi::c_void, buffer: *mut std::ffi::c_void)>,
    pub remove_buffer: Option<unsafe extern "C" fn(data: *mut std::ffi::c_void, buffer: *mut std::ffi::c_void)>,
    pub process: Option<unsafe extern "C" fn(data: *mut std::ffi::c_void)>,
    pub drained: Option<unsafe extern "C" fn(data: *mut std::ffi::c_void)>,
}

unsafe extern "C" fn pw_process_callback(data: *mut std::ffi::c_void) {
    let backend = &mut *(data as *mut PipewireBackend);
    let pw = match &backend.lib {
        Some(l) => l,
        None => return,
    };

    let buffer = (pw.pw_stream_dequeue_buffer)(backend.stream);
    if buffer.is_null() { return; }

    #[repr(C)]
    struct PwBuffer {
        buffer: *mut std::ffi::c_void,
        _other: [u64; 4],
    }
    let pw_buf = &*(buffer as *const PwBuffer);
    #[repr(C)]
    struct SpaBuffer {
        n_metas: u32,
        metas: *mut std::ffi::c_void,
        n_datas: u32,
        datas: *mut SpaData,
    }
    #[repr(C)]
    struct SpaData {
        _type: u32,
        flags: u32,
        fd: i64,
        mapoffset: u32,
        maxsize: u32,
        data: *mut std::ffi::c_void,
        chunk: *mut std::ffi::c_void,
    }
    let spa_buf = unsafe { &*(pw_buf.buffer as *const SpaBuffer) };

    let num_samples = 128; // Hard engine constraint
    if spa_buf.n_datas >= 2 {
        let data0 = unsafe { &*spa_buf.datas.add(0) };
        let data1 = unsafe { &*spa_buf.datas.add(1) };
        let ch0 = unsafe { std::slice::from_raw_parts_mut(data0.data as *mut f32, num_samples) };
        let ch1 = unsafe { std::slice::from_raw_parts_mut(data1.data as *mut f32, num_samples) };
        let mut out_refs = [ch0, ch1];

        if let Some(engine) = &mut backend.engine {
            engine.process_block(&[], &mut out_refs, num_samples);
        }
    } else if spa_buf.n_datas == 1 {
        let data0 = unsafe { &*spa_buf.datas };
        let ch0 = unsafe { std::slice::from_raw_parts_mut(data0.data as *mut f32, num_samples) };
        let mut out_refs = [ch0];

        if let Some(engine) = &mut backend.engine {
            engine.process_block(&[], &mut out_refs, num_samples);
        }
    }

    (pw.pw_stream_queue_buffer)(backend.stream, buffer);
}

unsafe extern "C" fn pw_param_changed(data: *mut std::ffi::c_void, id: u32, _param: *const std::ffi::c_void) {
    if id != 2 { return; } // SPA_PARAM_Props
    let _ = ipc_layer::set_rt_priority(90);
}

impl AudioBackend for PipewireBackend {
    fn start(&mut self, engine: AudioEngine) -> Result<(), String> {
        unsafe {
            if self.lib.is_none() { self.lib = Some(PwLib::load()?); }
            self.engine = Some(engine);
            self.running.store(true, Ordering::SeqCst);

            let pw = self.lib.as_ref().unwrap();
            (pw.pw_init)(std::ptr::null_mut(), std::ptr::null_mut());
            self.thread_loop = (pw.pw_thread_loop_new)(b"nullherz-loop\0".as_ptr() as *const i8, std::ptr::null_mut());
            let loop_ptr = (pw.pw_thread_loop_get_loop)(self.thread_loop);
            self.context = (pw.pw_context_new)(loop_ptr, std::ptr::null_mut(), 0);
            let _core = (pw.pw_core_connect)(self.context, std::ptr::null_mut(), 0);

            self.stream = (pw.pw_stream_new)(self.context, b"nullherz-stream\0".as_ptr() as *const i8, std::ptr::null_mut());

            let format_pod: [u32; 10] = [
                3, // SPA_TYPE_OBJECT_Format
                40, // size
                1, // SPA_PARAM_EnumFormat
                1, // media type (audio)
                1, // media subtype (raw)
                1, // format (F32)
                44100, // rate
                2, // channels
                0, 0, // padding
            ];
            let format_ptr = format_pod.as_ptr() as *const std::ffi::c_void;
            let params = [format_ptr];

            self.events = Some(Box::new(PwStreamEvents {
                version: 1,
                destroy: None,
                state_changed: None,
                control_info: None,
                io_changed: None,
                param_changed: Some(pw_param_changed),
                add_buffer: None,
                remove_buffer: None,
                process: Some(pw_process_callback),
                drained: None,
            }));

            let ev_ptr = self.events.as_ref().unwrap().as_ref() as *const _ as *const _;
            let self_ptr = self as *mut _ as *mut _;
            let pw = self.lib.as_ref().unwrap();
            (pw.pw_stream_add_listener)(self.stream, std::ptr::null_mut(), ev_ptr, self_ptr);
            (pw.pw_stream_connect)(self.stream, 1, 0xffffffff, 0x1, params.as_ptr() as *const _, 1);
            (pw.pw_thread_loop_start)(self.thread_loop);
        }
        Ok(())
    }
    fn stop(&mut self) -> Option<AudioEngine> {
        self.running.store(false, Ordering::SeqCst);
        unsafe {
            if let Some(pw) = &self.lib {
                if !self.thread_loop.is_null() {
                    (pw.pw_thread_loop_stop)(self.thread_loop);
                }
                if !self.stream.is_null() {
                    (pw.pw_stream_destroy)(self.stream);
                    self.stream = std::ptr::null_mut();
                }
                if !self.context.is_null() {
                    (pw.pw_context_destroy)(self.context);
                    self.context = std::ptr::null_mut();
                }
                if !self.thread_loop.is_null() {
                    (pw.pw_thread_loop_destroy)(self.thread_loop);
                    self.thread_loop = std::ptr::null_mut();
                }
            }
        }
        self.engine.take()
    }
}
