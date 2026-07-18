// Non-RT plane (software-clocked backend loop): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
use nullherz_traits::RenderingEngine;
use crate::AudioBackend;
use std::thread;
use std::sync::Arc;
use parking_lot::Mutex;
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

/// Nanoseconds one period occupies at the given sample rate — the software
/// clock the threaded backend paces itself with.
pub fn period_ns(period_size: u64, sample_rate: f64) -> u64 {
    ((period_size as f64) / sample_rate * 1_000_000_000.0) as u64
}

/// One backend cycle: render a period into `outputs_raw` through the shared
/// engine handle (if an engine is installed) and report the sample rate to
/// pace by. Extracted from the thread loop so it is testable without threads,
/// sleeps, or audio hardware.
pub fn run_cycle(
    engine_handle: &Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>,
    outputs_raw: &mut [Vec<f32>],
    period_size: u64,
) -> f64 {
    let mut sample_rate = 44100.0f64;
    if let Some(ref engine_arc) = *engine_handle.lock() {
        sample_rate = engine_arc.target_sample_rate() as f64;
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
    sample_rate
}

impl AudioBackend for ThreadedBackend {
    fn start(&mut self, engine_handle: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>, period_size: u64) -> Result<(), String> {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let handle = thread::spawn(move || {
            ipc_layer::setup_rt_thread(90, Some(0));
            {
                if let Some(ref engine_arc) = *engine_handle.lock() {
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
                let sample_rate = run_cycle(&engine_handle, &mut outputs_raw, period_size);
                // Simulate audio hardware clock
                thread::sleep(std::time::Duration::from_nanos(period_ns(period_size, sample_rate)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    /// Counts process calls and writes a marker so tests can see real output.
    struct FakeEngine {
        calls: Arc<AtomicU32>,
        rate: f32,
        marker: f32,
    }
    impl RenderingEngine for FakeEngine {
        fn process_block(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], num_samples: usize) {
            self.calls.fetch_add(1, Ordering::SeqCst);
            for out in outputs.iter_mut() {
                let n = num_samples.min(out.len());
                out[..n].fill(self.marker);
            }
        }
        fn set_config(&mut self, _config: nullherz_traits::AudioConfig) {}
        fn target_sample_rate(&self) -> f32 { self.rate }
        fn pull_all_snapshots(&self, _target: &mut Vec<(u64, Arc<Vec<f32>>)>) {}
        fn list_children(&self) -> Vec<&dyn nullherz_traits::AudioProcessor> { Vec::new() }
    }

    fn handle_with(engine: Option<FakeEngine>) -> (Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>, Arc<AtomicU32>) {
        let calls = engine.as_ref().map(|e| e.calls.clone()).unwrap_or_default();
        let inner: Option<Arc<dyn RenderingEngine>> = engine.map(|e| Arc::new(e) as Arc<dyn RenderingEngine>);
        (Arc::new(Mutex::new(inner)), calls)
    }

    fn fake(rate: f32, marker: f32) -> FakeEngine {
        FakeEngine { calls: Arc::new(AtomicU32::new(0)), rate, marker }
    }

    #[test]
    fn test_period_ns_matches_hardware_clock_math() {
        assert_eq!(period_ns(44100, 44100.0), 1_000_000_000, "one second of samples = one second");
        assert_eq!(period_ns(256, 44100.0), 5_804_988, "256 @ 44.1kHz ≈ 5.8 ms");
        assert_eq!(period_ns(128, 48000.0), 2_666_666, "128 @ 48kHz ≈ 2.67 ms");
    }

    #[test]
    fn test_run_cycle_renders_and_reports_engine_rate() {
        let (handle, calls) = handle_with(Some(fake(48000.0, 0.7)));
        let mut outputs = vec![vec![0.0f32; 64]; 4];

        let rate = run_cycle(&handle, &mut outputs, 64);

        assert_eq!(rate, 48000.0, "pacing must follow the engine's rate, not a hardcoded one");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        for ch in &outputs {
            assert!(ch.iter().all(|&v| v == 0.7), "all four channels rendered");
        }
    }

    #[test]
    fn test_run_cycle_without_engine_is_silent_and_defaults_rate() {
        let (handle, _) = handle_with(None);
        let mut outputs = vec![vec![0.0f32; 64]; 4];

        let rate = run_cycle(&handle, &mut outputs, 64);

        assert_eq!(rate, 44100.0, "no engine -> default pacing so the loop still sleeps sanely");
        assert!(outputs.iter().all(|ch| ch.iter().all(|&v| v == 0.0)), "no engine -> untouched buffers");
    }

    #[test]
    fn test_engine_hot_swap_through_shared_handle() {
        // The whole backend-switching design rests on installing an engine into
        // the shared handle after the loop is already running.
        let (handle, _) = handle_with(None);
        let mut outputs = vec![vec![0.0f32; 32]; 4];

        run_cycle(&handle, &mut outputs, 32);
        assert!(outputs[0].iter().all(|&v| v == 0.0));

        let engine = fake(44100.0, 1.0);
        let calls = engine.calls.clone();
        *handle.lock() = Some(Arc::new(engine));

        run_cycle(&handle, &mut outputs, 32);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "engine installed mid-flight must be picked up");
        assert!(outputs[0].iter().all(|&v| v == 1.0));
    }

    #[test]
    fn test_threaded_backend_lifecycle_runs_and_stops() {
        let (handle, calls) = handle_with(Some(fake(44100.0, 0.5)));
        let mut backend = ThreadedBackend::new();
        backend.start(handle, 64).unwrap();

        // 64 samples @ 44.1kHz ≈ 1.45ms/cycle; wait for a few cycles.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while calls.load(Ordering::SeqCst) < 3 && std::time::Instant::now() < deadline {
            thread::sleep(std::time::Duration::from_millis(5));
        }
        let seen = calls.load(Ordering::SeqCst);
        assert!(seen >= 3, "backend thread must cycle continuously (saw {})", seen);

        backend.stop();
        let after_stop = calls.load(Ordering::SeqCst);
        thread::sleep(std::time::Duration::from_millis(30));
        assert!(
            calls.load(Ordering::SeqCst) <= after_stop + 1,
            "stop() must halt the render loop (joined thread cannot keep rendering)"
        );
    }
}
