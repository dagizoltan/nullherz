use std::sync::Arc;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use nullherz_traits::RenderingEngine;
use crate::AudioBackend;

pub struct MockBackend {
    pub process_count: Arc<AtomicU32>,
    pub is_running: bool,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            process_count: Arc::new(AtomicU32::new(0)),
            is_running: false,
        }
    }
}

impl AudioBackend for MockBackend {
    fn start(&mut self, engine: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>, period_size: u64) -> Result<(), String> {
        self.is_running = true;
        let count = self.process_count.clone();
        let engine_lock = engine.lock();
        if let Some(ref engine_arc) = *engine_lock {
            let inputs = [ &[][..]; 0 ];
            let mut out_data = vec![0.0f32; period_size as usize];
            let mut outputs = [ &mut out_data[..] ];
            let engine_ptr = Arc::as_ptr(engine_arc) as *mut dyn RenderingEngine;
            unsafe {
                (*engine_ptr).process_block(&inputs, &mut outputs, period_size as usize);
            }
            count.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    fn stop(&mut self) {
        self.is_running = false;
    }
}
