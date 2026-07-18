fn main() {
    unsafe {
        let lib = libc::dlopen(c"libasound.so.2".as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
        assert!(!lib.is_null());
        let sym = libc::dlsym(lib, c"snd_pcm_open".as_ptr());
        assert!(!sym.is_null());
        let snd_pcm_open: unsafe extern "C" fn(*mut *mut libc::c_void, *const i8, i32, i32) -> i32 = std::mem::transmute(sym);
        let mut pcm: *mut libc::c_void = std::ptr::null_mut();
        let name = std::ffi::CString::new("default").unwrap();
        println!("Calling snd_pcm_open...");
        let res = snd_pcm_open(&mut pcm, name.as_ptr(), 0, 0);
        println!("Result: {}", res);
    }
}
