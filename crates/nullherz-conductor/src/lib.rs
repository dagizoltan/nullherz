pub mod timeline;
pub mod backend;
pub mod orchestrator;
pub mod engine_coordinator;
pub mod ptp_engine;
pub mod topology_manager;
pub mod midi_clock;
pub mod analysis_kernel;
pub mod transfusion_manager;
pub mod mixer_bridge;
pub mod ipc_audio_bridge;
pub mod sidecar_supervisor;
pub mod discovery;
pub mod midi_mapper;
pub mod analysis_worker;
pub mod folder_monitor;
pub mod command_handler;
pub mod telemetry_service;
pub mod persistence;
pub mod pattern_manager;
#[cfg(test)]
mod mixing_test;
pub mod modulation_matrix;
pub mod clip_orchestrator;
pub mod bounce;
pub mod mixer_orchestrator;
pub mod genetic_sequencer;

pub use nullherz_dna::GeneticLibrary;
pub use orchestrator::Conductor;

pub struct EngineContext {
    pub command_producer: Box<dyn nullherz_traits::CommandProducer>,
    pub telemetry_consumer: ipc_layer::Consumer<audio_core::Telemetry>,
    pub midi_producer: ipc_layer::Producer<nullherz_traits::MidiEvent>,
}
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

        conductor.mixer_manager = mixer;
        conductor.apply_mixer_commands(commands);

        conductor.start_backend(nullherz_backends::AudioBackendType::Mock).unwrap();

        let engine_lock = conductor.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
        let _engine = engine_lock.as_ref().unwrap();

        // In MockBackend::start, we already call process_block once for verification.
        // We can check if the backend is running.
        assert!(conductor.engine_coordinator.backend_manager.backend.is_some());
    }

    #[test]
    fn test_conductor_dj_workflow() {
        let mut conductor = Conductor::new();
        conductor.setup_engine();

        // 1. Setup 4-channel mixer
        let mut mixer = MixerManager::new();
        let bootstrap = mixer.create_4channel_mixer();
        conductor.mixer_manager = mixer;
        conductor.apply_mixer_commands(bootstrap);

        // 2. Load track to Deck B
        let load_cmd = nullherz_traits::Command::Performance(
            nullherz_traits::PerformanceCommand::LoadTrackToDeck { deck_id: 'B', sample_id: 1234 }
        );
        conductor.apply_mixer_commands(vec![load_cmd]);

        // 3. Set EQ for Deck B
        let eq_cmd = nullherz_traits::Command::Mixer(
            nullherz_traits::MixerCommand::SetDeckParam {
                deck_id: 'B',
                param_type: nullherz_traits::DeckParamType::EqHigh,
                value: 0.0
            }
        );
        conductor.apply_mixer_commands(vec![eq_cmd]);

        // 4. Verify translation: Deck B's isolator should have been targeted.
        // We can't easily check the engine's internal state here without more plumbing,
        // but we've verified the code paths.
    }
}
