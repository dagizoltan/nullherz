use crate::engine::AudioEngine;
use crate::backends::AudioBackend;
use std::thread;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub struct ThreadedBackend {
    handle: Option<thread::JoinHandle<Option<AudioEngine>>>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl ThreadedBackend {
    pub fn new() -> Self { Self { handle: None, running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)) } }
}
impl AudioBackend for ThreadedBackend {
    fn start(&mut self, mut engine: AudioEngine) -> Result<(), String> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let handle = thread::spawn(move || {
            crate::setup_rt_thread(90, Some(0));
            engine.set_config(crate::AudioConfig {
                sample_rate: 44100.0,
                block_size: ipc_layer::MAX_BLOCK_SIZE,
            });
            let mut outputs_raw = [[0.0f32; ipc_layer::MAX_BLOCK_SIZE]; 2];
            let interval = Duration::from_secs_f64(ipc_layer::MAX_BLOCK_SIZE as f64 / 44100.0);
            while running.load(Ordering::SeqCst) {
                let start = std::time::Instant::now();
                let (ch1, ch2) = outputs_raw.split_at_mut(1);
                let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];
                engine.process_block(&[], &mut out_refs, ipc_layer::MAX_BLOCK_SIZE);
                let elapsed = start.elapsed();
                if elapsed < interval {
                    thread::sleep(interval - elapsed);
                } else {
                    engine.xrun_counter().fetch_add(1, Ordering::Relaxed);
                }
            }
            Some(engine)
        });
        self.handle = Some(handle);
        Ok(())
    }
    fn stop(&mut self) -> Option<AudioEngine> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap_or(None)
        } else {
            None
        }
    }
}
