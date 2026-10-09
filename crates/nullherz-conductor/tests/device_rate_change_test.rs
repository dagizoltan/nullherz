// Non-RT plane (backend restart settle delay): thread sleep is sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]

//! Changing the sample rate must change what the DEVICE does, not only what the
//! engine believes.
//!
//! `ConfigureAudioEngine` used to call `engine.set_config()` and stop there. The
//! running device keeps clocking at the rate it opened with, and nothing else
//! reopened it — `switch_backend`'s only other caller is the output-device
//! selector, and `start_backend` runs once at startup. So selecting 48 kHz on a
//! device running at 44.1 kHz left the engine rendering for 48 kHz into hardware
//! consuming 44,100 frames a second, and every track played at 44100/48000 =
//! 0.919x: about 1.5 semitones flat, with the opposite shift when lowering the
//! rate. `sync_session_rate()` could not detect it, because it reads the
//! ENGINE's target rate — it propagated the new belief rather than the conflict.
//!
//! Note why the existing suite was green through this: tests run on the Mock and
//! Threaded backends, which have no independent hardware clock. The threaded
//! backend paces itself from the engine's own rate, so engine and "device" agree
//! by construction and the disagreement is unreachable. Only a real device can
//! contradict the engine, which is why this file asserts the MECHANISM (the
//! device is reopened, and the device's answer is what the session adopts)
//! rather than trying to measure pitch.

use nullherz_conductor::orchestrator::Conductor;
use nullherz_traits::{Command, CoreCommand, RenderingEngine};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Stands in for a running device, and counts its own teardown.
struct CountingBackend {
    stops: Arc<AtomicUsize>,
}

impl nullherz_backends::AudioBackend for CountingBackend {
    fn start(
        &mut self,
        _engine: Arc<parking_lot::Mutex<Option<Arc<dyn RenderingEngine>>>>,
        _period_size: u64,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop(&mut self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
    fn enumerate_devices(&self) -> Vec<String> {
        vec!["default".to_string()]
    }
}

fn engine_rate(conductor: &Conductor) -> f32 {
    let lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
    lock.as_ref().map(|e| e.target_sample_rate()).unwrap_or(0.0)
}

/// Force the engine to report `rate`, the way a backend does once the device has
/// answered. This is the only way a test can make the device contradict the
/// request, since `BackendFactory::create` decides which backend a restart
/// builds and a test cannot inject one there.
fn force_engine_rate(conductor: &Conductor, rate: f32, block: usize) {
    let lock = conductor.engine_coordinator.backend_manager.engine_handle.lock();
    if let Some(arc) = lock.as_ref() {
        let ptr = Arc::as_ptr(arc) as *mut dyn RenderingEngine;
        unsafe {
            (*ptr).set_config(nullherz_traits::AudioConfig { sample_rate: rate, block_size: block });
        }
    }
}

#[test]
fn test_a_rate_change_reopens_the_device() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();

    let stops = Arc::new(AtomicUsize::new(0));
    conductor.engine_coordinator.backend_manager.backend =
        Some(Box::new(CountingBackend { stops: stops.clone() }));
    // A device is running, and the conductor knows which kind it is — that is
    // what tells the handler there is something to reopen.
    conductor.engine_coordinator.backend_manager.active_type =
        Some(nullherz_traits::AudioBackendType::Mock);

    conductor.apply_mixer_commands(vec![Command::Core(CoreCommand::ConfigureAudioEngine {
        sample_rate: 48_000.0,
        block_size: 256,
    })]);

    let n = stops.load(Ordering::SeqCst);
    assert!(
        n >= 1,
        "the running device was never torn down on a rate change ({n} stop(s)). The engine now \
         believes it is at 48 kHz while the hardware still clocks at whatever rate it opened \
         with, so every track plays transposed by the ratio between them."
    );

    assert_eq!(
        engine_rate(&conductor), 48_000.0,
        "the engine did not adopt the configured rate"
    );
    assert_eq!(
        conductor.mixer_bridge.timeline.sample_rate, 48_000.0,
        "the timeline still converts samples to musical time at the old rate"
    );
    assert_eq!(
        conductor.topology_manager.current_sample_rate, 48_000.0,
        "nodes built after this change would be constructed for the old rate"
    );
}

#[test]
fn test_no_backend_running_means_nothing_to_reopen() {
    // Configuring the engine before a device is attached must still work: it is
    // how the startup path sets the rate the first `start()` will request.
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    assert!(conductor.engine_coordinator.backend_manager.active_type.is_none());

    conductor.apply_mixer_commands(vec![Command::Core(CoreCommand::ConfigureAudioEngine {
        sample_rate: 96_000.0,
        block_size: 128,
    })]);

    assert_eq!(engine_rate(&conductor), 96_000.0);
    assert_eq!(conductor.mixer_bridge.timeline.sample_rate, 96_000.0);
    assert_eq!(conductor.period_size, 128);
}

/// The device's answer wins over the request.
///
/// ALSA negotiates with `snd_pcm_hw_params_set_rate_near`, so a 96 kHz request
/// on a 48 kHz-only interface comes back as 48 kHz. Stamping the REQUEST into
/// the session would recreate the original defect in a subtler form: the engine
/// would render for 96 kHz into a device running at 48 kHz. The backend
/// publishes what it actually negotiated, and the session has to follow that.
#[test]
fn test_the_session_follows_the_rate_the_device_negotiated() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();

    // The session asked for 96 kHz...
    conductor.apply_mixer_commands(vec![Command::Core(CoreCommand::ConfigureAudioEngine {
        sample_rate: 96_000.0,
        block_size: 256,
    })]);
    assert_eq!(conductor.topology_manager.current_sample_rate, 96_000.0);

    // ...and the device answered 48 kHz, the way a backend reports back from its
    // audio thread once the hardware has spoken.
    force_engine_rate(&conductor, 48_000.0, 256);
    conductor.tick();

    assert_eq!(
        conductor.topology_manager.current_sample_rate, 48_000.0,
        "the session kept the REQUESTED rate after the device substituted a different one; \
         every node built from here would be constructed for a rate the hardware is not running"
    );
    assert_eq!(
        conductor.mixer_bridge.timeline.sample_rate, 48_000.0,
        "the timeline kept the requested rate, so musical time would drift against the device"
    );
    assert_eq!(
        conductor.transfusion_manager.device_sample_rate(), 48_000,
        "captures would be stamped with a rate the device never ran at"
    );
}
