use nullherz_traits::RenderingEngine;
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
    fn start(&mut self, engine_handle: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>, period_size: u64) -> Result<(), String> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let handle = thread::spawn(move || {
            ipc_layer::setup_rt_thread(90, Some(0));
            {
                if let Some(ref engine_arc) = *engine_handle.lock().unwrap() {
                    let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn RenderingEngine;
                    unsafe {
                        (*engine_ptr).set_config(nullherz_traits::AudioConfig {
                            sample_rate: 44100.0,
                            block_size: period_size as usize,
                        });
                    }
                }
            }

            let mut outputs_raw = vec![vec![0.0f32; period_size as usize]; 4];
            while running.load(Ordering::SeqCst) {
                if let Some(ref engine_arc) = *engine_handle.lock().unwrap() {
                    let (out0, rest) = outputs_raw.split_at_mut(1);
                    let (out1, rest) = rest.split_at_mut(1);
                    let (out2, out3) = rest.split_at_mut(1);
                    let mut out_refs: [&mut [f32]; 4] = [
                        &mut out0[0][..],
                        &mut out1[0][..],
                        &mut out2[0][..],
                        &mut out3[0][..],
                    ];
                    let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn RenderingEngine;
                    unsafe {
                        (*engine_ptr).process_block(&[], &mut out_refs, period_size as usize);
                    }
                }
                // Simulate audio hardware clock
                let sleep_ns = ((period_size as f64) / 44100.0 * 1_000_000_000.0) as u64;
                thread::sleep(std::time::Duration::from_nanos(sleep_ns));
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
