use std::sync::{Arc, Mutex};
use nullherz_traits::RenderingEngine;
use crate::AudioBackend;

pub struct OfflineBackend {
    pub output_path: String,
}

impl OfflineBackend {
    pub fn new(path: &str) -> Self {
        Self { output_path: path.to_string() }
    }
}

impl AudioBackend for OfflineBackend {
    fn start(&mut self, engine_handle: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>) -> Result<(), String> {
        let mut engine_lock = engine_handle.lock().unwrap();
        let engine = engine_lock.as_mut().ok_or("Engine not initialized")?;

        let sample_rate = engine.target_sample_rate() as u32;
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(&self.output_path, spec).map_err(|e| e.to_string())?;

        // Fast-as-possible render loop
        // For Alpha prototype, we render 10 seconds of audio
        let total_samples = sample_rate * 10;
        let block_size = 128;
        let mut processed = 0;

        let mut outputs_raw = [[0.0f32; 128]; 2];

        println!("Conductor: Starting offline render to {}...", self.output_path);

        while processed < total_samples {
            let (ch0, rest) = outputs_raw.split_at_mut(1);
            let (ch1, _) = rest.split_at_mut(1);
            let mut out_refs: [&mut [f32]; 2] = [
                &mut ch0[0],
                &mut ch1[0],
            ];

            let engine_ptr = Arc::as_ptr(engine) as *mut dyn RenderingEngine;
            unsafe {
                (*engine_ptr).process_block(&[], &mut out_refs, block_size);
            }

            for i in 0..block_size {
                writer.write_sample(outputs_raw[0][i]).unwrap();
                writer.write_sample(outputs_raw[1][i]).unwrap();
            }

            processed += block_size as u32;
        }

        writer.finalize().map_err(|e| e.to_string())?;
        println!("Conductor: Offline render complete.");

        Ok(())
    }

    fn stop(&mut self) {}
}
