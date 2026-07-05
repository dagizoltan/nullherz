use wasmtime::*;
use ipc_layer::{AudioBlock, ShmRingBuffer};
use nullherz_traits::Command;
use std::sync::Arc;

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
        linker.func_wrap("nullherz", "pop_command", |mut caller: Caller<'_, WasmState>| -> i32 {
             let state = caller.data_mut();
             unsafe {
                 if let Some(_cmd) = (*state.cmd_buffer).pop() {
                     // In a real implementation, we'd copy the command into WASM memory
                     1
                 } else {
                     0
                 }
             }
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

    pub fn process(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let func = self.host.instance.get_typed_func::<(), ()>(&mut self.host.store, "process")?;
        func.call(&mut self.host.store, ())?;
        Ok(())
    }
}
