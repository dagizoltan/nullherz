use audio_core::AudioEngine;
use audio_core::engine::EngineResources;

pub struct OfflineRenderer {
    engine: AudioEngine,
}

impl OfflineRenderer {
    pub fn new(engine: AudioEngine) -> Self {
        Self { engine }
    }

    pub fn create_engine(initial_graph: Box<dyn nullherz_traits::AudioProcessor>) -> AudioEngine {
        let (cmd_prod, cmd_cons) = ipc_layer::RingBuffer::new(256).split();
        let (garbage_prod, _garbage_cons) = ipc_layer::RingBuffer::new(256).split();
        let (telemetry_prod, _telemetry_cons) = ipc_layer::RingBuffer::new(256).split();

        let resources = EngineResources {
            command_consumer: Box::new(cmd_cons),
            command_producer: Box::new(cmd_prod),
            midi_consumer: None,
            bundle_consumer: None,
            topology_consumer: None,
            garbage_producer: garbage_prod,
            overflow_garbage_producer: None,
            bundle_garbage_producer: None,
            bundle_overflow_producer: None,
            telemetry_producer: Box::new(telemetry_prod),
        };

        let logger = std::sync::Arc::new(audio_core::rt_logging::RtLogger::new(1024));
        AudioEngine::new(resources, initial_graph, logger, audio_core::engine::processing_kernel::StandardKernel::default())
    }

    pub fn bounce_to_wav(&mut self, path: &str, duration_seconds: f32) -> std::io::Result<()> {
        let sample_rate = self.engine.target_sample_rate;
        let num_samples = (duration_seconds * sample_rate) as usize;
        let block_size = 1024;

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sample_rate as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(path, spec).map_err(|e| std::io::Error::other(e))?;

        let mut processed = 0;
        let mut left_out = vec![0.0f32; block_size];
        let mut right_out = vec![0.0f32; block_size];

        println!("Bounce: Starting high-fidelity export to {} ({} seconds)", path, duration_seconds);

        while processed < num_samples {
            let chunk = (num_samples - processed).min(block_size);

            // Re-bind slices to correct length
            let left_slice = &mut left_out[..chunk];
            let right_slice = &mut right_out[..chunk];

            let inputs: Vec<&[f32]> = vec![];
            let mut outputs = vec![left_slice, right_slice];

            self.engine.process_block(&inputs, &mut outputs, chunk);

            for i in 0..chunk {
                writer.write_sample(left_out[i]).map_err(|e| std::io::Error::other(e))?;
                writer.write_sample(right_out[i]).map_err(|e| std::io::Error::other(e))?;
            }

            processed += chunk;
            if processed % (sample_rate as usize) == 0 {
                println!("Bounce: Rendered {} seconds...", processed as f32 / sample_rate);
            }
        }

        writer.finalize().map_err(|e| std::io::Error::other(e))?;
        println!("Bounce: Export complete.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offline_renderer_creation() {
        let dummy_graph = Box::new(nullherz_processors::FallbackProcessor::new(0));
        let engine = OfflineRenderer::create_engine(dummy_graph);
        assert_eq!(engine.target_sample_rate, 44100.0);
    }
}
