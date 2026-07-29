use std::sync::Arc;

pub struct OfflineRenderer {
    pub conductor: crate::orchestrator::Conductor,
}

impl OfflineRenderer {
    /// Render against the real library. Production entry point.
    pub fn new(state: crate::persistence::ProjectState) -> Self {
        Self::with_library_path(state, "library.redb")
    }

    /// Same, with the library path injected.
    ///
    /// Exists so tests can pass `":memory:"`. `new()` opens `library.redb` in the
    /// working directory — a real library on a developer machine — and redb's
    /// exclusive lock means a second opener silently gets a private fallback
    /// instead of an error, so a test using `new()` sees the real library or an
    /// empty one depending on who won the race.
    pub fn with_library_path(state: crate::persistence::ProjectState, library_path: &str) -> Self {
        let mut conductor = crate::orchestrator::Conductor::with_library_path(library_path);
        conductor.setup_engine();
        conductor.apply_state(state);
        Self { conductor }
    }

    pub fn bounce_to_wav(&mut self, path: &str, duration_seconds: f32) -> std::io::Result<()> {
        // Must be the rate the engine actually renders at, not a guess. The
        // WAV header is a claim about the samples that follow it: render at
        // 48 kHz and stamp 44100 and every player transposes the export down a
        // tone and stretches it — the file is wrong on disk, silently, forever.
        let sample_rate = {
            let lock = self.conductor.engine_coordinator.backend_manager.engine_handle.lock();
            lock.as_ref()
                .map(|e| e.target_sample_rate())
                .unwrap_or(nullherz_traits::DEFAULT_SAMPLE_RATE)
        };
        let num_samples = (duration_seconds * sample_rate) as usize;

        // Offline rendering answers to no device period, so take the largest
        // block the graph supports — fewer, larger blocks means less per-block
        // overhead for the same audio. Measured with bench_console_block on the
        // 4-deck console: 374 ns/sample at 1024 vs 441 at 256, a 15% saving.
        //
        // CAVEAT: that benchmark also showed the output peak varying slightly
        // with block size (0.5170 at 256 -> 0.5203 at 1024). The likely cause is
        // benign — events quantize to block boundaries, so a bigger block starts
        // them on a coarser grid — but it has NOT been confirmed, and if some
        // stage turns out to update per-block rather than per-sample then an
        // export here is not sample-identical to what was heard. Revisit this
        // choice when that is settled.
        //
        // Critically, this has to be *declared*. Node buffers are sized by
        // `set_config`, so rendering 1024 frames into a graph last configured
        // for a 256-frame device period reads past the end of every one of
        // them. That was a live panic on any path that bounced after the
        // backend had negotiated a period smaller than 1024 — which is the
        // normal case, since a 1024-frame period is ~21 ms of latency.
        let block_size = nullherz_traits::MAX_BLOCK_SIZE;
        {
            let mut engine_lock = self.conductor.engine_coordinator.backend_manager.engine_handle.lock();
            if let Some(ref mut engine_arc) = *engine_lock {
                let config = nullherz_traits::AudioConfig { sample_rate, block_size };
                match Arc::get_mut(engine_arc) {
                    Some(engine) => engine.set_config(config),
                    None => {
                        let ptr = Arc::as_ptr(engine_arc) as *mut dyn nullherz_traits::RenderingEngine;
                        unsafe { (*ptr).set_config(config) };
                    }
                }
            }
        }

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sample_rate as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(path, spec).map_err(std::io::Error::other)?;

        let mut processed = 0;
        let mut reported_seconds = 0usize;
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
                writer.write_sample(left_out[i]).map_err(std::io::Error::other)?;
                writer.write_sample(right_out[i]).map_err(std::io::Error::other)?;
            }

            processed += chunk;
            // Report on second boundaries crossed, not on exact multiples: with
            // a 1024-frame block and a 96 kHz rate, `processed` is never an
            // exact multiple of the rate, so the old test printed nothing at
            // all for the entire render.
            let seconds_done = processed / sample_rate.max(1.0) as usize;
            if seconds_done > reported_seconds {
                reported_seconds = seconds_done;
                println!("Bounce: Rendered {} seconds...", seconds_done);
            }
        }

        writer.finalize().map_err(std::io::Error::other)?;
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
        // NOT `new()`: that opens the developer's real library.redb in the
        // working directory (this test was creating a 3.6 MB one in the crate
        // directory on every run).
        let _renderer = OfflineRenderer::with_library_path(state, ":memory:");
    }
}
