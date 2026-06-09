use crate::backends::AudioBackend;

pub struct JackBackend {
    inner: Box<JackBackendInner>,
}

struct JackBackendInner {
    client: *mut std::ffi::c_void,
    ports: Vec<*mut std::ffi::c_void>,
    engine: Option<crate::engine::AudioEngine>,
    lib: Option<JackLib>,
}

unsafe impl Send for JackBackend {}
unsafe impl Send for JackBackendInner {}

impl Default for JackBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl JackBackend {
    pub fn new() -> Self {
        Self {
            inner: Box::new(JackBackendInner {
                client: std::ptr::null_mut(),
                ports: Vec::new(),
                engine: None,
                lib: None,
            })
        }
    }
}

struct JackLib {
    _handle: *mut std::ffi::c_void,
    jack_client_open: unsafe extern "C" fn(*const i8, i32, *mut i32) -> *mut std::ffi::c_void,
    jack_client_close: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
    jack_set_process_callback: unsafe extern "C" fn(*mut std::ffi::c_void, unsafe extern "C" fn(u32, *mut std::ffi::c_void) -> i32, *mut std::ffi::c_void) -> i32,
    jack_activate: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
    jack_deactivate: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
    jack_port_register: unsafe extern "C" fn(*mut std::ffi::c_void, *const i8, *const i8, u64, u64) -> *mut std::ffi::c_void,
    jack_port_get_buffer: unsafe extern "C" fn(*mut std::ffi::c_void, u32) -> *mut std::ffi::c_void,
}

impl JackLib {
    fn load() -> Result<Self, String> {
        unsafe {
            let lib = libc::dlopen(c"libjack.so.0".as_ptr(), libc::RTLD_NOW);
            if lib.is_null() { return Err("Could not load libjack.so.0".to_string()); }
            let load_sym = |name: &std::ffi::CStr| {
                let sym = libc::dlsym(lib, name.as_ptr());
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                _handle: lib,
                jack_client_open: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*const i8, i32, *mut i32) -> *mut std::ffi::c_void>(load_sym(c"jack_client_open").ok_or("jack_client_open failed")?),
                jack_client_close: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void) -> i32>(load_sym(c"jack_client_close").ok_or("jack_client_close failed")?),
                jack_set_process_callback: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, unsafe extern "C" fn(u32, *mut std::ffi::c_void) -> i32, *mut std::ffi::c_void) -> i32>(load_sym(c"jack_set_process_callback").ok_or("jack_set_process_callback failed")?),
                jack_activate: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void) -> i32>(load_sym(c"jack_activate").ok_or("jack_activate failed")?),
                jack_deactivate: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void) -> i32>(load_sym(c"jack_deactivate").ok_or("jack_deactivate failed")?),
                jack_port_register: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, *const i8, *const i8, u64, u64) -> *mut std::ffi::c_void>(load_sym(c"jack_port_register").ok_or("jack_port_register failed")?),
                jack_port_get_buffer: std::mem::transmute::<*mut libc::c_void, unsafe extern "C" fn(*mut std::ffi::c_void, u32) -> *mut std::ffi::c_void>(load_sym(c"jack_port_get_buffer").ok_or("jack_port_get_buffer failed")?),
            })
        }
    }
}

/// # Safety
/// This is an FFI callback for JACK. 'data' must be a valid pointer to 'JackBackendInner'.
unsafe extern "C" fn jack_process_callback(nframes: u32, data: *mut std::ffi::c_void) -> i32 {
    // SAFETY: data is a pointer to the backend inner state provided in jack_set_process_callback.
    let backend = unsafe { &mut *(data as *mut JackBackendInner) };
    let jack = match &backend.lib {
        Some(l) => l,
        None => return 0,
    };

    let mut out_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
    let num_ports = backend.ports.len().min(16);
    for (i, out_ptr) in out_ptrs.iter_mut().enumerate().take(num_ports) {
        // SAFETY: backend.ports[i] is a valid JACK port register and nframes is the callback length.
        *out_ptr = unsafe { (jack.jack_port_get_buffer)(backend.ports[i], nframes) } as *mut f32;
    }

    if let Some(engine) = &mut backend.engine {
        let mut out_refs_storage: [&mut [f32]; 16] = std::array::from_fn(|i| {
            if i < num_ports {
                // SAFETY: JACK guarantees that the buffer returned by jack_port_get_buffer is valid for nframes.
                unsafe { std::slice::from_raw_parts_mut(out_ptrs[i], nframes as usize) }
            } else {
                &mut []
            }
        });
        engine.process_block(&[], &mut out_refs_storage[..num_ports], nframes as usize);
    }
    0
}

impl AudioBackend for JackBackend {
    fn start(&mut self, engine: crate::engine::AudioEngine) -> Result<(), String> {
        unsafe {
            let inner = &mut *self.inner;
            if inner.lib.is_none() { inner.lib = Some(JackLib::load()?); }
            let mut status = 0;

            inner.client = (inner.lib.as_ref().unwrap().jack_client_open)(c"nullherz".as_ptr(), 0, &mut status);
            if inner.client.is_null() { return Err("Failed to open JACK client".to_string()); }

            let out1 = (inner.lib.as_ref().unwrap().jack_port_register)(inner.client, c"out_1".as_ptr(), c"32 bit float mono audio".as_ptr(), 2, 0);
            let out2 = (inner.lib.as_ref().unwrap().jack_port_register)(inner.client, c"out_2".as_ptr(), c"32 bit float mono audio".as_ptr(), 2, 0);
            inner.ports = vec![out1, out2];

            inner.engine = Some(engine);
            let ptr = inner as *mut _ as *mut _;
            (inner.lib.as_ref().unwrap().jack_set_process_callback)(inner.client, jack_process_callback, ptr);
            (inner.lib.as_ref().unwrap().jack_activate)(inner.client);
        }
        Ok(())
    }
    fn stop(&mut self) -> Option<crate::engine::AudioEngine> {
        unsafe {
            let inner = &mut *self.inner;
            if !inner.client.is_null() {
                let jack = inner.lib.as_ref().unwrap();
                (jack.jack_deactivate)(inner.client);
                (jack.jack_client_close)(inner.client);
                inner.client = std::ptr::null_mut();
            }
            inner.engine.take()
        }
    }
}
