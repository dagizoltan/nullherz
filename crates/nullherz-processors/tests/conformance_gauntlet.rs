//! The Gauntlet, actually running.
//!
//! `AGENTS.md` §4 requires every processor to pass `GauntletRunner` — NaN
//! ingestion and buffer-size oscillation — before a DSP change is submitted,
//! and `ARCHITECTURE.md` describes it as covering "every registered processor".
//! It had **zero callers in the workspace**. Neither of the two existing
//! conformance test files invoked it, so neither of those two checks had ever
//! executed against anything.
//!
//! Same for the Law of Zero Allocation: `verify_zero_allocation` was a stub
//! returning `Ok(())`, while `docs/archive/SYSTEM_STATUS.md` recorded the
//! invariant as "[VERIFIED] ... via the gauntlet conformance test suite".
//!
//! This file is where both claims become checks. The allocation guard is a
//! per-binary global allocator, which is why it lives in its own test target
//! rather than being bolted onto `conformance.rs`.

use nullherz_processors::ProcessorRegistry;
use nullherz_traits::test_kit::{gauntlet::GauntletRunner, rt_alloc, StabilityTester};

/// Installing this is what gives `verify_zero_allocation` something to observe.
/// Without it the check reports "not installed" and fails loudly, rather than
/// counting zero allocations and reporting a clean pass.
#[global_allocator]
static ALLOC: rt_alloc::CountingAllocator<std::alloc::System> =
    rt_alloc::CountingAllocator::new(std::alloc::System);

const SR: f32 = 44_100.0;

#[test]
fn allocation_guard_is_actually_installed() {
    assert!(
        rt_alloc::is_installed(),
        "the #[global_allocator] above is not in force; every zero-allocation \
         result in this binary would be vacuous"
    );
}

/// NaN ingestion and buffer-size oscillation, on every registered processor.
#[test]
fn every_processor_survives_the_gauntlet() {
    let registry = ProcessorRegistry::new();
    let mut failures = Vec::new();

    for (id, name) in registry.list_available_processors() {
        let Some(mut proc) = registry.create_by_id(id, 0, SR) else {
            failures.push(format!("{name} (id {id}): registry could not construct it"));
            continue;
        };
        if let Err(e) = GauntletRunner::run_stress_tests(proc.as_mut()) {
            failures.push(format!("{name} (id {id}): {e}"));
        }
    }

    assert!(failures.is_empty(), "gauntlet failures:\n  {}", failures.join("\n  "));
}

/// `process()` must not allocate on a steady-state block.
///
/// Reported as a list rather than the first failure: when a previously
/// unchecked invariant is switched on, one processor at a time is the slowest
/// possible way to find out how much work there is.
#[test]
fn no_processor_allocates_on_the_audio_path() {
    assert!(rt_alloc::is_installed(), "allocation guard not installed");

    let registry = ProcessorRegistry::new();
    let mut offenders = Vec::new();

    for (id, name) in registry.list_available_processors() {
        let Some(mut proc) = registry.create_by_id(id, 0, SR) else { continue };
        if let Err(e) = StabilityTester::verify_zero_allocation(proc.as_mut()) {
            offenders.push(format!("{name} (id {id}): {e}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "processors allocating on the RT path:\n  {}",
        offenders.join("\n  ")
    );
}

/// A processor that allocates must be CAUGHT. Without this, a guard that
/// silently stopped working would look identical to a clean codebase — which
/// is precisely the failure mode the stub had.
#[test]
fn the_guard_catches_a_known_allocator() {
    use nullherz_traits::{
        AudioProcessor, MidiResponder, ProcessContext, SignalProcessor, SnapshotProvider,
    };

    /// Allocates one Vec per block, exactly like the defect this test exists
    /// to detect.
    struct Offender;
    impl SignalProcessor for Offender {
        fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _c: &mut ProcessContext) {
            let scratch: Vec<f32> = inputs.first().map(|i| i.to_vec()).unwrap_or_default();
            if let Some(out) = outputs.first_mut() {
                for (o, s) in out.iter_mut().zip(scratch.iter()) {
                    *o = *s;
                }
            }
        }
    }
    impl MidiResponder for Offender {}
    impl SnapshotProvider for Offender {}
    impl AudioProcessor for Offender {
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    let mut offender = Offender;
    let err = StabilityTester::verify_zero_allocation(&mut offender)
        .expect_err("a processor allocating a Vec per block must be reported");
    assert!(err.contains("allocated"), "unexpected message: {err}");
}
