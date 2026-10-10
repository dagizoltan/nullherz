// Non-RT plane (test-harness pacing): thread sleep is sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]

//! `Conductor::tick()` must stay inside the audio-block budget with a REAL
//! library resident, not just the handful of samples a 4-deck fixture holds.
//!
//! Every fixture in the tree except `bench_studio_scale` measures the 4-deck
//! console, and `AGENTS.md` §4 says so in its own words: *"Scale is part of the
//! gate's blind spot, not part of the gate."* Four separate defects have now
//! been found on this one function by reading it rather than by running it, and
//! every one of them was invisible at fixture scale because its cost was a
//! function of how many samples are resident:
//!
//!   * `refresh_audio_devices` enumerated ALSA inline — 15 ms per call.
//!   * `refresh_residency` walked the registry twice a second — 525 µs at 16k.
//!   * `reap_registry` called `list_ids()` before its own growth check, so an
//!     idle session allocated an N-element Vec every second to compute a length.
//!   * the library view listed the whole library twice per refresh.
//!
//! A fifth would be found the same way. This test is the alternative: drive
//! `tick()` with a large resident registry and assert the budget directly, so
//! the class is caught by the gate instead of by inspection.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nullherz_conductor::Conductor;

const SR: f64 = 44_100.0;
const BLOCK: usize = 256;

/// One audio block of wall time — the period the whole control path shares.
fn block_budget() -> Duration {
    Duration::from_secs_f64(BLOCK as f64 / SR)
}

/// A library big enough to be a library. Buffers are deliberately tiny: the
/// cost under test is per-SAMPLE bookkeeping (registry maps, residency, the
/// reap sweep's per-id lookups), not audio bytes, and 10k real tracks would be
/// tens of gigabytes.
const RESIDENT_SAMPLES: u64 = 10_000;

fn register_cheap_samples(conductor: &Conductor, n: u64) {
    let buffer: nullherz_traits::SampleBuffer = Arc::new(vec![0.0f32; 64]).into();
    let metadata = Arc::new(nullherz_traits::SampleMetadata::new_empty());
    for id in 0..n {
        conductor
            .transfusion_manager
            .sample_registry
            .register_with_metadata(id, buffer.clone(), metadata.clone());
    }
}

struct TickCost {
    worst: Duration,
    total: Duration,
    ticks: u32,
}

fn measure_ticks(conductor: &mut Conductor, ticks: u32) -> TickCost {
    let mut cost = TickCost { worst: Duration::ZERO, total: Duration::ZERO, ticks };
    for _ in 0..ticks {
        let t = Instant::now();
        conductor.tick();
        let dt = t.elapsed();
        cost.worst = cost.worst.max(dt);
        cost.total += dt;
        // Space the ticks out enough that the second-granularity housekeeping
        // inside `tick()` (metadata sync, reap sweep, autosave check) actually
        // comes due during the run rather than being skipped by its own timer.
        std::thread::sleep(Duration::from_millis(2));
    }
    cost
}

#[test]
fn test_tick_stays_inside_the_block_budget_with_a_large_library() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let mut conductor = Conductor::with_library_path(":memory:");
    conductor.setup_engine();
    conductor.bootstrap_4channel_mixer();

    // Baseline at fixture scale, so the ratio below is meaningful.
    let small = measure_ticks(&mut conductor, 100);

    register_cheap_samples(&conductor, RESIDENT_SAMPLES);
    assert_eq!(
        conductor.transfusion_manager.sample_registry.residency().count as u64,
        RESIDENT_SAMPLES,
        "the registry did not accept the samples; the rest of this test would measure nothing"
    );

    let large = measure_ticks(&mut conductor, 400);
    let budget = block_budget();

    eprintln!(
        "tick() with {RESIDENT_SAMPLES} resident samples: worst {:?}, mean {:?} \
         (empty registry: worst {:?}, mean {:?}; budget {:?})",
        large.worst,
        large.total / large.ticks,
        small.worst,
        small.total / small.ticks,
        budget,
    );

    if cfg!(debug_assertions) {
        eprintln!(
            "note: wall-clock budget assertion skipped (debug build). Enforce with: \
             cargo test --release -p nullherz-conductor --test tick_budget_at_scale_test"
        );
        return;
    }

    assert!(
        large.worst < budget,
        "tick() took {:?} with {RESIDENT_SAMPLES} samples resident, over the {:?} audio-block \
         budget (it costs {:?} with an empty registry). Something in tick() scales with the \
         number of resident samples: every command queued behind that tick, Play included, is \
         delayed by it. AGENTS.md §1 — no unbounded work inline on the conductor path.",
        large.worst, budget, small.worst,
    );
}
