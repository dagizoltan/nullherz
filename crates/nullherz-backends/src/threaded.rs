use audio_core::AudioEngine;
use crate::AudioBackend;
use std::thread;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct ThreadedBackend {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Default for ThreadedBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadedBackend {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

impl AudioBackend for ThreadedBackend {
    fn start(&mut self, engine_handle: Arc<Mutex<Option<AudioEngine>>>) -> Result<(), String> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let handle = thread::spawn(move || {
            audio_core::setup_rt_thread(90, Some(0));
            {
                if let Some(ref mut engine) = *engine_handle.lock().unwrap() {
                    engine.set_config(nullherz_traits::AudioConfig {
                        sample_rate: 44100.0,
                        block_size: 128,
                    });
                }
            }

            let mut outputs_raw = [[0.0f32; 128]; 2];
            while running.load(Ordering::SeqCst) {
                if let Some(ref mut engine) = *engine_handle.lock().unwrap() {
                    let (ch1, ch2) = outputs_raw.split_at_mut(1);
                    let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];
                    engine.process_block(&[], &mut out_refs, 128);
                }
                // Simulate audio hardware clock
                thread::sleep(std::time::Duration::from_nanos(2902494)); // 128 samples at 44.1kHz
            }
        });
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
