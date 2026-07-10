use wasmtime::*;
use ipc_layer::{AudioBlock, ShmRingBuffer};
use nullherz_traits::Command;

pub struct WasmSidecarHost {
    pub engine: Engine,
    pub module: Module,
    pub store: Store<WasmState>,
    pub instance: Instance,
}

pub struct WasmState {
    pub cmd_buffer: *mut ShmRingBuffer<Command>,
    pub audio_inputs: Vec<*mut ShmRingBuffer<AudioBlock>>,
    pub audio_outputs: Vec<*mut ShmRingBuffer<AudioBlock>>,
}

unsafe impl Send for WasmState {}

impl WasmSidecarHost {
    pub fn new(wasm_path: &str, state: WasmState) -> Result<Self, Box<dyn std::error::Error>> {
        let engine = Engine::default();
        let module = Module::from_file(&engine, wasm_path)?;
        let mut linker = Linker::new(&engine);

        // Define host functions for SHM access
        linker.func_wrap("nullherz", "pop_command", |mut caller: Caller<'_, WasmState>, ptr: i32, max_len: i32| -> i32 {
             let state = caller.data_mut();
             unsafe {
                 if let Some(cmd) = (*state.cmd_buffer).pop() {
                     let mem = caller.get_export("memory").unwrap().into_memory().unwrap();

                     // Hardened: Zero-allocation serialization using a stack buffer.
                     let mut buf = [0u8; 512];
                     let mut cursor = std::io::Cursor::new(&mut buf[..]);
                     if bincode::serialize_into(&mut cursor, &cmd).is_ok() {
                         let len = cursor.position() as usize;
                         let data = &buf[..len];

                         if data.len() <= max_len as usize {
                             if mem.write(&mut caller, ptr as usize, data).is_ok() {
                                 return data.len() as i32;
                             }
                         }
                     }
                     -1 // Error: buffer too small or write failed
                 } else {
                     0 // No commands waiting
                 }
             }
        })?;

        linker.func_wrap("nullherz", "get_audio_input_ref", |caller: Caller<'_, WasmState>, channel: i32| -> i64 {
             let state = caller.data();
             if let Some(rb_ptr) = state.audio_inputs.get(channel as usize) {
                 unsafe {
                     // Stage 2: Return a pointer directly into the SHM RingBuffer's current head
                     // Note: This requires the guest to know the SHM layout.
                     // For now, we'll return the absolute address of the data segment.
                     let rb = &**rb_ptr;
                     let head = rb.head.load(std::sync::atomic::Ordering::Acquire);
                     let tail = rb.tail.load(std::sync::atomic::Ordering::Acquire);
                     if head != tail {
                         // This is a placeholder for true zero-copy mapping.
                         // In a production scenario, we'd map this page into the WASM instance.
                         return rb_ptr as *const _ as i64;
                     }
                 }
             }
             0
        })?;

        linker.func_wrap("nullherz", "get_audio_input", |mut caller: Caller<'_, WasmState>, channel: i32, ptr: i32| -> i32 {
             let block = {
                 let state = caller.data_mut();
                 if let Some(rb_ptr) = state.audio_inputs.get(channel as usize) {
                     unsafe { (**rb_ptr).pop() }
                 } else {
                     None
                 }
             };

             if let Some(block) = block {
                 let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                 let data = bytemuck::cast_slice(&block.data);
                 let _ = mem.write(&mut caller, ptr as usize, data);
                 return block.len as i32;
             }
             0
        })?;

        linker.func_wrap("nullherz", "set_audio_output", |mut caller: Caller<'_, WasmState>, channel: i32, ptr: i32, len: i32| -> i32 {
             let mut data = [0.0f32; 256];
             {
                 let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                 let _ = mem.read(&caller, ptr as usize, bytemuck::cast_slice_mut(&mut data));
             }

             let state = caller.data_mut();
             if let Some(rb_ptr) = state.audio_outputs.get(channel as usize) {
                 let block = AudioBlock {
                     data,
                     len: len as u32,
                     _pad: [0; 15],
                 };
                 unsafe {
                     if (**rb_ptr).push(block).is_ok() {
                         return 1;
                     }
                 }
             }
             0
        })?;

        let mut store = Store::new(&engine, state);
        let instance = linker.instantiate(&mut store, &module)?;

        Ok(Self { engine, module, store, instance })
    }
}

pub struct WasmSidecarRunner {
    host: WasmSidecarHost,
}

impl WasmSidecarRunner {
    pub fn new(wasm_path: &str, state: WasmState) -> Result<Self, Box<dyn std::error::Error>> {
        let host = WasmSidecarHost::new(wasm_path, state)?;
        Ok(Self { host })
    }

    /// Optimized process loop for WASM sidecars.
    /// Future R&D: Implement wasm_simd128 pathways for 4x performance boost in spectral kernels.
    pub fn process(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let func = self.host.instance.get_typed_func::<(), ()>(&mut self.host.store, "process")?;
        func.call(&mut self.host.store, ())?;
        Ok(())
    }
}
