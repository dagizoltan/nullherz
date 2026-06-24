pub mod timeline;
pub mod backend;
pub mod orchestrator;
pub mod engine_coordinator;
pub mod topology_manager;
pub mod transfusion_manager;
pub mod mixer_bridge;
pub mod sidecar_supervisor;
pub mod analysis_worker;

pub use orchestrator::Conductor;
pub use timeline::Timeline;
pub use backend::BackendManager;

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_mixer::MixerManager;

    #[test]
    fn test_conductor_mixer_integration() {
        let mut conductor = Conductor::new();
        conductor.setup_engine();

        let mut mixer = MixerManager::new();
        let commands = mixer.create_studio_strip("TestStrip", &[]);

        conductor.apply_mixer_commands(commands);

        conductor.start_backend(nullherz_backends::AudioBackendType::Mock).unwrap();

        let engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
        let _engine = engine_lock.as_ref().unwrap();

        // In MockBackend::start, we already call process_block once for verification.
        // We can check if the backend is running.
        assert!(conductor.engine_coordinator.backend_manager.backend.is_some());
    }
}
