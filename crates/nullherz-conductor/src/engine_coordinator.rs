use audio_core::engine::builder::EngineBuilder;
use crate::backend::BackendManager;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use audio_core::AudioProcessor;
use nullherz_traits::{Command, CommandProducer};

pub struct EngineCoordinator {
    pub backend_manager: BackendManager,
    pub garbage_consumer: Option<ipc_layer::Consumer<Box<dyn AudioProcessor>>>,
    pub overflow_garbage_consumer: Option<ipc_layer::Consumer<Box<dyn AudioProcessor>>>,
    pub bundle_garbage_consumer: Option<ipc_layer::Consumer<Vec<Command>>>,
    pub bundle_overflow_consumer: Option<ipc_layer::Consumer<Vec<Command>>>,
    pub health_signal: Option<Arc<AtomicBool>>,
    pub command_producer: Option<Box<dyn CommandProducer>>,
}

impl EngineCoordinator {
    pub fn new() -> Self {
        Self {
            backend_manager: BackendManager::default(),
            garbage_consumer: None,
            overflow_garbage_consumer: None,
            bundle_garbage_consumer: None,
            bundle_overflow_consumer: None,
            health_signal: None,
            command_producer: None,
        }
    }

    pub fn setup(&mut self) -> audio_core::engine::builder::EngineHandle {
        ipc_layer::SharedMemory::cleanup_stale_segments();

        let (engine, handle) = EngineBuilder::new()
            .with_command_buffer_size(1024)
            .build();

        self.health_signal = Some(handle.health_signal.clone());
        self.command_producer = Some(handle.command_producer.clone());
        *self.backend_manager.engine_handle.lock().unwrap() = Some(engine);

        handle
    }

    pub fn drain_garbage(&mut self) {
        if let Some(ref mut cons) = self.garbage_consumer {
            while let Some(proc) = cons.pop() { drop(proc); }
        }
        if let Some(ref mut cons) = self.overflow_garbage_consumer {
            while let Some(proc) = cons.pop() { drop(proc); }
        }
        if let Some(ref mut cons) = self.bundle_garbage_consumer {
            while let Some(bundle) = cons.pop() { drop(bundle); }
        }
        if let Some(ref mut cons) = self.bundle_overflow_consumer {
            while let Some(bundle) = cons.pop() { drop(bundle); }
        }
    }

    pub fn check_health(&mut self) -> bool {
        if let Some(ref signal) = self.health_signal {
            signal.swap(false, std::sync::atomic::Ordering::Relaxed)
        } else {
            false
        }
    }
}
