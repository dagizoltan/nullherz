use std::sync::Arc;

pub struct OfflineRenderer {
    pub conductor: crate::orchestrator::Conductor,
}

impl OfflineRenderer {
    pub fn new(state: crate::persistence::ProjectState) -> Self {
        let mut conductor = crate::orchestrator::Conductor::new();
        conductor.setup_engine();
        conductor.apply_state(state);
        Self { conductor }
    }

    pub fn bounce_to_wav(&mut self, path: &str, duration_seconds: f32) -> std::io::Result<()> {
        let sample_rate = 44100.0; // Default or from conductor
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

            // 1. Tick conductor to apply commands/mutations
            self.conductor.tick();

            // 2. Process block via backend engine handle
            let mut engine_lock = self.conductor.engine_coordinator.backend_manager.engine_handle.lock();
            if let Some(ref mut engine_arc) = *engine_lock {
                // In offline mode, the conductor and engine are both owned by the OfflineRenderer.
                // Since we need &mut RenderingEngine to process_block, we try to get a mutable reference.
                // This is safe here because we are in a single-threaded offline rendering context.
                if let Some(engine) = Arc::get_mut(engine_arc) {
                    engine.process_block(&inputs, &mut outputs, chunk);
                } else {
                    // Fallback for cases where Arc is shared, though expected to be unique in this context.
                    let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn nullherz_traits::RenderingEngine;
                    unsafe { (*engine_ptr).process_block(&inputs, &mut outputs, chunk); }
                }
            }

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
        let state = crate::persistence::ProjectState::empty();
        let _renderer = OfflineRenderer::new(state);
    }
}
