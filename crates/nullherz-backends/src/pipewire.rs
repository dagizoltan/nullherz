use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use nullherz_traits::RenderingEngine;
use crate::AudioBackend;

struct PwLib {
    _handle: *mut std::ffi::c_void,
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
    _pw_stream_update_params: unsafe extern "C" fn(*mut std::ffi::c_void, *mut *const std::ffi::c_void, u32) -> i32,
    pw_stream_dequeue_buffer: unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void,
    pw_stream_queue_buffer: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32,
    pw_stream_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pw_context_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pw_thread_loop_destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
}

impl Drop for PwLib {
    fn drop(&mut self) {
        unsafe { libc::dlclose(self._handle); }
    }
}

impl PwLib {
    fn load() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(c"libpipewire-0.3.so.0".as_ptr(), libc::RTLD_NOW);
            if lib.is_null() { return Err("Could not load libpipewire-0.3.so.0".to_string()); }
            let load_sym = |name: &std::ffi::CStr| {
                let sym = libc::dlsym(lib, name.as_ptr());
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                _handle: lib,
                pw_init: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut i32, *mut *mut *mut i8)>(load_sym(c"pw_init").ok_or("pw_init failed")?),
                pw_thread_loop_new: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*const i8, *const std::ffi::c_void) -> *mut std::ffi::c_void>(load_sym(c"pw_thread_loop_new").ok_or("pw_thread_loop_new failed")?),
                pw_thread_loop_start: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void) -> i32>(load_sym(c"pw_thread_loop_start").ok_or("pw_thread_loop_start failed")?),
                pw_thread_loop_stop: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void)>(load_sym(c"pw_thread_loop_stop").ok_or("pw_thread_loop_stop failed")?),
                pw_thread_loop_get_loop: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void>(load_sym(c"pw_thread_loop_get_loop").ok_or("pw_thread_loop_get_loop failed")?),
                pw_context_new: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void>(load_sym(c"pw_context_new").ok_or("pw_context_new failed")?),
                pw_core_connect: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void>(load_sym(c"pw_core_connect").ok_or("pw_core_connect failed")?),
                pw_stream_new: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *const i8, *mut std::ffi::c_void) -> *mut std::ffi::c_void>(load_sym(c"pw_stream_new").ok_or("pw_stream_new failed")?),
                pw_stream_add_listener: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void, *const libc::c_void, *mut libc::c_void)>(load_sym(c"pw_stream_add_listener").ok_or("pw_stream_add_listener failed")?),
                pw_stream_connect: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, i32, u32, u32, *const std::ffi::c_void, u32) -> i32>(load_sym(c"pw_stream_connect").ok_or("pw_stream_connect failed")?),
                _pw_stream_update_params: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut *const std::ffi::c_void, u32) -> i32>(load_sym(c"pw_stream_update_params").ok_or("pw_stream_update_params failed")?),
                pw_stream_dequeue_buffer: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void>(load_sym(c"pw_stream_dequeue_buffer").ok_or("pw_stream_dequeue_buffer failed")?),
                pw_stream_queue_buffer: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32>(load_sym(c"pw_stream_queue_buffer").ok_or("pw_stream_queue_buffer failed")?),
                pw_stream_destroy: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void)>(load_sym(c"pw_stream_destroy").ok_or("pw_stream_destroy failed")?),
                pw_context_destroy: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void)>(load_sym(c"pw_context_destroy").ok_or("pw_context_destroy failed")?),
                pw_thread_loop_destroy: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void)>(load_sym(c"pw_thread_loop_destroy").ok_or("pw_thread_loop_destroy failed")?),
            })
        }
    }
}

pub struct PipewireBackend {
    inner: Box<PipewireBackendInner>,
}

pub struct PipewireBackendInner {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread_loop: *mut std::ffi::c_void,
    context: *mut std::ffi::c_void,
    stream: *mut std::ffi::c_void,
    engine_handle: Option<Arc<Mutex<Option<Box<dyn RenderingEngine>>>>>,
    lib: Option<PwLib>,
    events: Option<Box<PwStreamEvents>>,
    listener: [u64; 8],
}

unsafe impl Send for PipewireBackend {}
unsafe impl Send for PipewireBackendInner {}

impl Default for PipewireBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PipewireBackend {
    pub fn new() -> Self {
        Self {
            inner: Box::new(PipewireBackendInner {
                running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                thread_loop: std::ptr::null_mut(),
                context: std::ptr::null_mut(),
                stream: std::ptr::null_mut(),
                engine_handle: None,
                lib: None,
                events: None,
                listener: [0; 8],
            })
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

#[repr(C)]
pub struct SpaBuffer {
    pub n_metas: u32,
    pub metas: *mut std::ffi::c_void,
    pub n_datas: u32,
    pub datas: *mut SpaData,
}

#[repr(C)]
pub struct SpaData {
    pub type_: u32,
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

const SPA_TYPE_OBJECT: u32 = 3;
const SPA_TYPE_INT: u32 = 4;
const SPA_TYPE_ID: u32 = 11;

const SPA_PARAM_ENUM_FORMAT: u32 = 1;
const SPA_FORMAT_MEDIA_TYPE: u32 = 1;
const SPA_FORMAT_MEDIA_SUBTYPE: u32 = 2;
const SPA_FORMAT_FORMAT: u32 = 3;
const SPA_FORMAT_RATE: u32 = 4;
const SPA_FORMAT_CHANNELS: u32 = 5;

const SPA_MEDIA_TYPE_AUDIO: u32 = 1;
const SPA_MEDIA_SUBTYPE_RAW: u32 = 1;
const SPA_AUDIO_FORMAT_F32: u32 = 3;

pub struct SpaPodBuilder {
    pub data: Vec<u32>,
}

impl Default for SpaPodBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SpaPodBuilder {
    pub fn new() -> Self {
        Self { data: Vec::with_capacity(64) }
    }

    pub fn begin_object(&mut self, type_: u32, id: u32) -> usize {
        let offset = self.data.len();
        self.data.push(0); // size placeholder
        self.data.push(SPA_TYPE_OBJECT);
        self.data.push(type_);
        self.data.push(id);
        offset
    }

    pub fn end_object(&mut self, offset: usize) {
        let size = (self.data.len() - offset - 2) * 4;
        self.data[offset] = size as u32;
    }

    pub fn add_prop_id(&mut self, key: u32, value: u32) {
        self.data.push(key);
        self.data.push(0); // flags
        self.data.push(4); // size
        self.data.push(SPA_TYPE_ID); // type
        self.data.push(value);
        self.data.push(0); // padding to 8 bytes (total 6 words = 24 bytes)
    }

    pub fn add_prop_int(&mut self, key: u32, value: u32) {
        self.data.push(key);
        self.data.push(0); // flags
        self.data.push(4); // size
        self.data.push(SPA_TYPE_INT); // type
        self.data.push(value);
        self.data.push(0); // padding to 8 bytes (total 6 words = 24 bytes)
    }
}

unsafe extern "C" fn pw_process_callback(data: *mut std::ffi::c_void) {
    let backend = unsafe { &mut *(data as *mut PipewireBackendInner) };
    let Some(pw) = &backend.lib else { return };

    let buffer = unsafe { (pw.pw_stream_dequeue_buffer)(backend.stream) };
    if buffer.is_null() { return; }

    #[repr(C)]
    struct PwBuffer {
        buffer: *mut SpaBuffer,
    }
    let pw_buf = unsafe { &*(buffer as *const PwBuffer) };
    let spa_buf = unsafe { &*pw_buf.buffer };

    let mut num_samples = 128;
    if spa_buf.n_datas > 0 {
        let first_data = unsafe { &*spa_buf.datas.add(0) };
        let mut size_bytes = first_data.maxsize;
        if !first_data.chunk.is_null() {
            let chunk_size = unsafe { (*first_data.chunk).size };
            if chunk_size > 0 {
                size_bytes = chunk_size;
            }
        }
        num_samples = (size_bytes as usize / 4).min(ipc_layer::MAX_BLOCK_SIZE);
    }
    let num_channels = spa_buf.n_datas.min(16) as usize;
    let mut out_refs_storage: [&mut [f32]; 16] = std::array::from_fn(|_| &mut [][..]);

    for (i, out_ref) in out_refs_storage.iter_mut().enumerate().take(num_channels) {
        let data = unsafe { &*spa_buf.datas.add(i) };
        if !data.data.is_null() {
            *out_ref = unsafe {
                std::slice::from_raw_parts_mut(data.data as *mut f32, num_samples)
            };
        }
        if !data.chunk.is_null() {
            unsafe {
                (*data.chunk).offset = 0;
                (*data.chunk).size = (num_samples * 4) as u32;
                (*data.chunk).stride = 4;
            }
        }
    }

    if let Some(ref handle) = backend.engine_handle {
        #[allow(clippy::collapsible_if)]
        if let Some(ref mut engine) = *handle.lock().unwrap() {
            engine.process_block(&[], &mut out_refs_storage[..num_channels], num_samples);
        }
    }

    unsafe { (pw.pw_stream_queue_buffer)(backend.stream, buffer); }
}

unsafe extern "C" fn pw_param_changed(_data: *mut std::ffi::c_void, id: u32, _param: *const std::ffi::c_void) {
    if id != 2 { } // SPA_PARAM_Props
}

impl AudioBackend for PipewireBackend {
    fn start(&mut self, engine_handle: Arc<Mutex<Option<Box<dyn RenderingEngine>>>>) -> Result<(), String> {
        unsafe {
            let inner = &mut *self.inner;
            if inner.lib.is_none() { inner.lib = Some(PwLib::load()?); }

            let mut target_rate = 44100u32;
            {
                if let Some(ref mut engine) = *engine_handle.lock().unwrap() {
                    target_rate = engine.target_sample_rate() as u32;
                    engine.set_config(nullherz_traits::AudioConfig { sample_rate: target_rate as f32, block_size: 128 });
                }
            }

            inner.engine_handle = Some(engine_handle);
            inner.running.store(true, Ordering::SeqCst);

            let pw = inner.lib.as_ref().unwrap();
            (pw.pw_init)(std::ptr::null_mut(), std::ptr::null_mut());
            inner.thread_loop = (pw.pw_thread_loop_new)(c"nullherz-loop".as_ptr(), std::ptr::null_mut());
            let loop_ptr = (pw.pw_thread_loop_get_loop)(inner.thread_loop);
            inner.context = (pw.pw_context_new)(loop_ptr, std::ptr::null_mut(), 0);
            let _core = (pw.pw_core_connect)(inner.context, std::ptr::null_mut(), 0);

            inner.stream = (pw.pw_stream_new)(inner.context, c"nullherz-stream".as_ptr(), std::ptr::null_mut());

            let mut builder = SpaPodBuilder::new();
            let obj_offset = builder.begin_object(SPA_PARAM_ENUM_FORMAT, SPA_PARAM_ENUM_FORMAT);
            builder.add_prop_id(SPA_FORMAT_MEDIA_TYPE, SPA_MEDIA_TYPE_AUDIO);
            builder.add_prop_id(SPA_FORMAT_MEDIA_SUBTYPE, SPA_MEDIA_SUBTYPE_RAW);
            builder.add_prop_id(SPA_FORMAT_FORMAT, SPA_AUDIO_FORMAT_F32);
            builder.add_prop_int(SPA_FORMAT_RATE, target_rate);
            builder.add_prop_int(SPA_FORMAT_CHANNELS, 2);
            builder.end_object(obj_offset);

            let format_ptr = builder.data.as_ptr() as *const std::ffi::c_void;
            let params = [format_ptr];

            inner.events = Some(Box::new(PwStreamEvents {
                version: 3, // PW_VERSION_STREAM_EVENTS
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

            let ev_ptr = inner.events.as_ref().unwrap().as_ref() as *const _ as *const _;
            let inner_ptr = inner as *mut _ as *mut _;
            let pw = inner.lib.as_ref().unwrap();
            (pw.pw_stream_add_listener)(inner.stream, inner.listener.as_mut_ptr() as *mut _, ev_ptr, inner_ptr);

            (pw.pw_stream_connect)(inner.stream, 1, 0xffffffff, 0x3, params.as_ptr() as *const _, 1);
            (pw.pw_thread_loop_start)(inner.thread_loop);
        }
        Ok(())
    }
    fn stop(&mut self) {
        unsafe {
            let inner = &mut *self.inner;
            inner.running.store(false, Ordering::SeqCst);
            if let Some(pw) = &inner.lib {
                if !inner.thread_loop.is_null() {
                    (pw.pw_thread_loop_stop)(inner.thread_loop);
                }
                if !inner.stream.is_null() {
                    (pw.pw_stream_destroy)(inner.stream);
                    inner.stream = std::ptr::null_mut();
                }
                if !inner.context.is_null() {
                    (pw.pw_context_destroy)(inner.context);
                    inner.context = std::ptr::null_mut();
                }
                if !inner.thread_loop.is_null() {
                    (pw.pw_thread_loop_destroy)(inner.thread_loop);
                    inner.thread_loop = std::ptr::null_mut();
                }
            }
        }
    }
}
