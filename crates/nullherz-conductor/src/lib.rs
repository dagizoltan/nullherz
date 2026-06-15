pub mod timeline;
pub mod backend;
pub mod orchestrator;

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
        let commands = mixer.create_studio_strip("TestStrip", &[]).unwrap();

        conductor.apply_mixer_commands(commands);

        let mut engine_lock = conductor.backend_manager.engine_handle.lock().unwrap();
        let engine = engine_lock.as_mut().unwrap();

        let mut outputs = [[0.0f32; 128], [0.0f32; 128]];
        let (ch1, ch2) = outputs.split_at_mut(1);
        let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];

        engine.process_block(&[], &mut out_refs, 128);
    }
}
