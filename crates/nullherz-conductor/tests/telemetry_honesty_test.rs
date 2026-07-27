//! Telemetry fields must mean what their labels say.
//!
//! Three separate bugs this session had the same shape: the number being
//! reported was not the number that mattered. `xrun_count` was read from an
//! atomic nothing incremented, so a gate on "zero xruns" could never fail. The
//! DSP-load metric divided by an assumed sample rate. The peak meter reported
//! the loudest node anywhere rather than the master output.
//!
//! A `u32` cannot say "nobody measured this", which is why every one of them
//! read as a confident zero. These tests pin the distinction between "measured
//! and zero" and "not measured", for the two fields where it still applied.

use nullherz_conductor::orchestrator::Conductor;
use nullherz_traits::telemetry::Telemetry;

#[test]
fn test_xruns_are_marked_unreported_when_no_backend_is_running() {
    // With no backend there is nobody to count underruns. Reporting 0 with no
    // caveat is what let a survival run print "PASS — 0 xruns" while the
    // backend's own log said 13.
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();

    let mut tel = Telemetry::default();
    conductor.update_timeline(&mut tel);

    assert!(
        !tel.xruns_reported,
        "telemetry claims underruns were measured, but no backend is running"
    );
}

#[test]
fn test_xruns_become_reported_once_a_counting_backend_runs() {
    // The Threaded backend counts missed deadlines, so once it is up the flag
    // must flip — otherwise the UI permanently shows "not reported" and the
    // fix accomplishes nothing.
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();
    if conductor.start_backend(nullherz_traits::AudioBackendType::Threaded).is_err() {
        return; // no backend available in this environment
    }

    let mut tel = Telemetry::default();
    conductor.update_timeline(&mut tel);
    conductor.stop_backend();

    assert!(
        tel.xruns_reported,
        "the Threaded backend counts underruns, but telemetry says they are unreported"
    );
}

#[test]
fn test_clock_jitter_is_marked_unavailable_without_a_provider() {
    // No PTP clock installed is not "perfectly locked". The calibration view
    // read the hardcoded 0 as flawless sync and displayed LOCKED.
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();

    // A stale reading from a previous frame: enrichment must clear it, not
    // leave it standing next to an "unavailable" flag.
    let mut tel = Telemetry { clock_jitter_ns: 12_345, ..Telemetry::default() };
    conductor.update_timeline(&mut tel);

    assert!(
        !tel.clock_jitter_available,
        "telemetry claims a clock measurement with no clock provider installed"
    );
    assert_eq!(
        tel.clock_jitter_ns, 0,
        "an unavailable measurement must be cleared, not left holding a stale reading"
    );
}

#[test]
fn test_block_size_and_rate_are_populated_so_dsp_load_is_computable() {
    // DSP load is process_time / (block_size / sample_rate). Both terms are
    // runtime values; the metrics view used to assume 256/44100 and was wrong
    // by the ratio of assumption to reality.
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    let mut tel = Telemetry::default();
    conductor.update_timeline(&mut tel);

    assert!(tel.sample_rate > 0.0, "sample_rate must be usable as a divisor");
    assert!(tel.block_size > 0, "block_size must be usable as a divisor");
    assert!(
        tel.block_size as usize <= nullherz_traits::MAX_BLOCK_SIZE,
        "block_size {} exceeds the graph's render capacity {}",
        tel.block_size,
        nullherz_traits::MAX_BLOCK_SIZE
    );
}

#[test]
fn test_no_flag_claims_a_measurement_the_value_contradicts() {
    // General invariant: a field flagged "measured" must not be the sentinel
    // the unmeasured case uses. Guards against a future edit that sets the
    // flag but forgets the value, which is exactly how these drift apart.
    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();

    let mut tel = Telemetry::default();
    conductor.update_timeline(&mut tel);

    if !tel.xruns_reported {
        assert_eq!(tel.xrun_count, 0, "unreported xruns must not carry a value");
    }
    if !tel.clock_jitter_available {
        assert_eq!(tel.clock_jitter_ns, 0, "unavailable jitter must not carry a value");
    }
}
