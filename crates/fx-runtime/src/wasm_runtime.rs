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
                     let memory_slice = mem.data_mut(&mut caller);
                     let start = ptr as usize;
                     let max_end = start + max_len as usize;

                     if max_end <= memory_slice.len() {
                         let target_slice = &mut memory_slice[start..max_end];
                         let mut cursor = std::io::Cursor::new(target_slice);
                         if bincode::serialize_into(&mut cursor, &cmd).is_ok() {
                             return cursor.position() as i32;
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
             if let Some(&rb_ptr) = state.audio_inputs.get(channel as usize) {
                 unsafe {
                     let rb = &*rb_ptr;
                     let head = rb.head.load(std::sync::atomic::Ordering::Acquire);
                     let tail = rb.tail.load(std::sync::atomic::Ordering::Acquire);
                     if head != tail {
                         return rb_ptr as *const _ as i64;
                     }
                 }
             }
             0
        })?;

        // Zero-copy direct pointer mapping to shared memory for commands and audio blocks
        linker.func_wrap("nullherz", "get_shared_command_buffer_ptr", |caller: Caller<'_, WasmState>| -> i64 {
             let state = caller.data();
             state.cmd_buffer as i64
        })?;

        linker.func_wrap("nullherz", "get_shared_audio_input_ptr", |caller: Caller<'_, WasmState>, channel: i32| -> i64 {
             let state = caller.data();
             if let Some(&rb_ptr) = state.audio_inputs.get(channel as usize) {
                 rb_ptr as i64
             } else {
                 0
             }
        })?;

        linker.func_wrap("nullherz", "get_shared_audio_output_ptr", |caller: Caller<'_, WasmState>, channel: i32| -> i64 {
             let state = caller.data();
             if let Some(&rb_ptr) = state.audio_outputs.get(channel as usize) {
                 rb_ptr as i64
             } else {
                 0
             }
        })?;

        linker.func_wrap("nullherz", "get_audio_input", |mut caller: Caller<'_, WasmState>, channel: i32, ptr: i32| -> i32 {
             let state = caller.data_mut();
             let block = if let Some(&rb_ptr) = state.audio_inputs.get(channel as usize) {
                 unsafe { (*rb_ptr).pop() }
             } else {
                 None
             };

             if let Some(block) = block {
                 let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                 let data_bytes = bytemuck::cast_slice(&block.data);
                 let start = ptr as usize;
                 let end = start + data_bytes.len();

                 let memory_slice = mem.data_mut(&mut caller);
                 if end <= memory_slice.len() {
                     memory_slice[start..end].copy_from_slice(data_bytes);
                     return block.len as i32;
                 }
             }
             0
        })?;

        linker.func_wrap("nullherz", "set_audio_output", |mut caller: Caller<'_, WasmState>, channel: i32, ptr: i32, len: i32| -> i32 {
             let state = caller.data_mut();
             if let Some(&rb_ptr) = state.audio_outputs.get(channel as usize) {
                 let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                 let start = ptr as usize;
                 let byte_len = 256 * std::mem::size_of::<f32>();
                 let end = start + byte_len;

                 let memory_slice = mem.data(&caller);
                 if end <= memory_slice.len() {
                     let mut data = [0.0f32; 256];
                     let dest_bytes = bytemuck::cast_slice_mut(&mut data);
                     dest_bytes.copy_from_slice(&memory_slice[start..end]);

                     let block = AudioBlock {
                         data,
                         len: len as u32,
                         _pad: [0; 15],
                     };
                     unsafe {
                         if (*rb_ptr).push(block).is_ok() {
                             return 1;
                         }
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
