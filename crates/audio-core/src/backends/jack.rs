use crate::backends::AudioBackend;

pub struct JackBackend {
    client: *mut std::ffi::c_void,
    ports: Vec<*mut std::ffi::c_void>,
    engine: Option<crate::engine::AudioEngine>,
    lib: Option<JackLib>,
}

unsafe impl Send for JackBackend {}

impl JackBackend {
    pub fn new() -> Self { Self { client: std::ptr::null_mut(), ports: Vec::new(), engine: None, lib: None } }
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
            let lib = libc::dlopen(b"libjack.so.0\0".as_ptr() as *const _, libc::RTLD_NOW);
            if lib.is_null() { return Err("Could not load libjack.so.0".to_string()); }
            let load_sym = |name: &[u8]| {
                let sym = libc::dlsym(lib, name.as_ptr() as *const _);
                if sym.is_null() { None } else { Some(sym) }
            };
            Ok(Self {
                _handle: lib,
                jack_client_open: std::mem::transmute(load_sym(b"jack_client_open\0").ok_or("jack_client_open failed")?),
                jack_client_close: std::mem::transmute(load_sym(b"jack_client_close\0").ok_or("jack_client_close failed")?),
                jack_set_process_callback: std::mem::transmute(load_sym(b"jack_set_process_callback\0").ok_or("jack_set_process_callback failed")?),
                jack_activate: std::mem::transmute(load_sym(b"jack_activate\0").ok_or("jack_activate failed")?),
                jack_deactivate: std::mem::transmute(load_sym(b"jack_deactivate\0").ok_or("jack_deactivate failed")?),
                jack_port_register: std::mem::transmute(load_sym(b"jack_port_register\0").ok_or("jack_port_register failed")?),
                jack_port_get_buffer: std::mem::transmute(load_sym(b"jack_port_get_buffer\0").ok_or("jack_port_get_buffer failed")?),
            })
        }
    }
}

unsafe extern "C" fn jack_process_callback(nframes: u32, data: *mut std::ffi::c_void) -> i32 {
    let backend = &mut *(data as *mut JackBackend);
    let jack = match &backend.lib {
        Some(l) => l,
        None => return 0,
    };

    let mut out_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
    let num_ports = backend.ports.len().min(16);
    for i in 0..num_ports {
        out_ptrs[i] = (jack.jack_port_get_buffer)(backend.ports[i], nframes) as *mut f32;
    }

    if let Some(engine) = &mut backend.engine {
        let mut out_refs_storage: [&mut [f32]; 16] = std::array::from_fn(|i| {
            if i < num_ports {
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
            if self.lib.is_none() { self.lib = Some(JackLib::load()?); }
            let mut status = 0;

            self.client = (self.lib.as_ref().unwrap().jack_client_open)(b"nullherz\0".as_ptr() as *const i8, 0, &mut status);
            if self.client.is_null() { return Err("Failed to open JACK client".to_string()); }

            let out1 = (self.lib.as_ref().unwrap().jack_port_register)(self.client, b"out_1\0".as_ptr() as *const i8, b"32 bit float mono audio\0".as_ptr() as *const i8, 2, 0);
            let out2 = (self.lib.as_ref().unwrap().jack_port_register)(self.client, b"out_2\0".as_ptr() as *const i8, b"32 bit float mono audio\0".as_ptr() as *const i8, 2, 0);
            self.ports = vec![out1, out2];

            self.engine = Some(engine);
            let ptr = self as *mut _ as *mut _;
            (self.lib.as_ref().unwrap().jack_set_process_callback)(self.client, jack_process_callback, ptr);
            (self.lib.as_ref().unwrap().jack_activate)(self.client);
        }
        Ok(())
    }
    fn stop(&mut self) -> Option<crate::engine::AudioEngine> {
        unsafe {
            if !self.client.is_null() {
                let jack = self.lib.as_ref().unwrap();
                (jack.jack_deactivate)(self.client);
                (jack.jack_client_close)(self.client);
                self.client = std::ptr::null_mut();
            }
        }
        self.engine.take()
    }
}
