//! Nothing expensive may run on the per-telemetry-frame path.
//!
//! `TelemetryService::update_timeline` is called once per audio block — about
//! 187 times a second at 48 kHz / 256, against a 5.3 ms budget per frame. This
//! project has now shipped the same bug on this exact function twice:
//!
//!   1. It called `library.get_track()` per deck to fill `waveform_peaks`.
//!      61 ms per call. The conductor could never drain its telemetry queue,
//!      it held the library mutex while doing so, and `LoadTrackToDeck` /
//!      `PlayDeck` queued behind it forever. Symptom: full-length tracks would
//!      not play while short demo WAVs did.
//!   2. It called `backend.enumerate_devices()` every frame. Once that stopped
//!      returning a hardcoded two-element vec and became a real driver query
//!      (dlopen + `snd_device_name_hint` + an ALSA config parse) it measured
//!      74.6 ms — roughly 14 seconds of work per second of audio. Same
//!      symptom: the decks stopped responding entirely.
//!
//! Both were invisible to every other test, because the audio graph was fine.
//! The failure was starvation of the thread that delivers commands to it.

use nullherz_conductor::orchestrator::Conductor;
use nullherz_traits::telemetry::Telemetry;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Backend that records how often the device list is queried.
struct CountingBackend {
    enumerations: Arc<AtomicUsize>,
}

impl nullherz_backends::AudioBackend for CountingBackend {
    fn start(
        &mut self,
        _engine: Arc<parking_lot::Mutex<Option<Arc<dyn nullherz_traits::RenderingEngine>>>>,
        _period_size: u64,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop(&mut self) {}
    fn enumerate_devices(&self) -> Vec<String> {
        // Stands in for the 74.6 ms real query.
        self.enumerations.fetch_add(1, Ordering::SeqCst);
        vec!["default".to_string(), "hw:0,0".to_string()]
    }
    fn xruns(&self) -> Option<u64> { Some(0) }
}

#[test]
fn test_device_enumeration_does_not_run_per_telemetry_frame() {
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();

    let enumerations = Arc::new(AtomicUsize::new(0));
    conductor.engine_coordinator.backend_manager.backend =
        Some(Box::new(CountingBackend { enumerations: enumerations.clone() }));

    // One tick may legitimately refresh the cache.
    conductor.tick();
    let after_tick = enumerations.load(Ordering::SeqCst);

    // A second of telemetry at 48 kHz / 256.
    let mut tel = Telemetry::default();
    for _ in 0..187 {
        conductor.update_timeline(&mut tel);
    }

    let during_frames = enumerations.load(Ordering::SeqCst) - after_tick;
    assert_eq!(
        during_frames, 0,
        "enumerate_devices() ran {during_frames} time(s) across one second of telemetry \
         frames. It is a driver query costing ~74 ms; at this rate the conductor thread \
         cannot drain its queue and deck commands never execute."
    );
}

#[test]
fn test_repeated_ticks_do_not_rescan_devices_every_time() {
    // The cache must also hold across ticks — `tick()` runs far faster than
    // hardware changes. Refreshing there instead of per frame is only a fix if
    // it is actually rate-limited.
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();

    let enumerations = Arc::new(AtomicUsize::new(0));
    conductor.engine_coordinator.backend_manager.backend =
        Some(Box::new(CountingBackend { enumerations: enumerations.clone() }));

    for _ in 0..50 {
        conductor.tick();
    }

    let n = enumerations.load(Ordering::SeqCst);
    assert!(
        n <= 1,
        "50 ticks triggered {n} device scans; the rescan timer is not limiting anything"
    );
}

#[test]
fn test_the_device_list_still_reaches_telemetry() {
    // Caching must not silently drop the feature. The settings view populates
    // its device picker from this.
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.engine_coordinator.backend_manager.backend =
        Some(Box::new(CountingBackend { enumerations: Arc::new(AtomicUsize::new(0)) }));

    conductor.tick(); // populates the cache
    let mut tel = Telemetry::default();
    conductor.update_timeline(&mut tel);

    let first = String::from_utf8_lossy(&tel.audio_devices[0].name)
        .trim_matches(char::from(0))
        .to_string();
    assert_eq!(
        first, "default",
        "the cached device list did not reach telemetry; the picker would be empty"
    );
}

/// Release-only, deliberately.
///
/// An absolute wall-clock bound in an unoptimised build measures rustc's output
/// quality, not this code — a debug build runs the same work 10-30x slower, so
/// any threshold loose enough to pass there is too loose to catch the 12x and
/// 14x overruns this exists for. The call-COUNTING test above carries the real
/// guarantee and is profile-independent; this is the backstop for expensive
/// work that is not a device query.
#[cfg(not(debug_assertions))]
#[test]
fn test_a_second_of_telemetry_fits_well_inside_its_budget() {
    // Backstop for the general class rather than this one call. Deliberately
    // loose: the bugs above were 12x and 14x over budget, so a 10x margin still
    // catches them while leaving room for a slow CI box.
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();
    conductor.tick();

    let mut tel = Telemetry::default();
    // Warm any lazily-built state so the first call does not dominate.
    for _ in 0..10 {
        conductor.update_timeline(&mut tel);
    }

    let frames = 187;
    let start = std::time::Instant::now();
    for _ in 0..frames {
        conductor.update_timeline(&mut tel);
    }
    let elapsed = start.elapsed();

    // One second of audio must take far less than one second to describe.
    assert!(
        elapsed < std::time::Duration::from_millis(100),
        "one second of telemetry frames took {elapsed:?} to produce. The audio thread \
         generates them in 1.0 s, so the conductor is falling behind and deck commands \
         will stall behind the backlog."
    );
}
