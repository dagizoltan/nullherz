//! CPU affinity must be opt-in, bounds-checked, and honest about what it buys.
//!
//! Pinning is the most cargo-culted knob in realtime audio. It helps only when
//! the kernel actually reserved the CPU (`isolcpus`/`nohz_full`) and when the
//! chosen CPU's SMT sibling is also kept clear. On a machine meeting neither
//! condition — the common case — a pin restricts our own thread and reserves
//! nothing, and on an SMT machine it can place the audio thread on the same
//! physical core as a DSP worker, which is worse than leaving it alone.
//!
//! So the contract under test is not "pinning works" but "pinning does nothing
//! unless asked, refuses impossible requests, and states its own limits".

/// `NULLHERZ_AUDIO_CPU` is process-wide, so any test that sets it races every
/// test that reads it. This serialises them.
///
/// The default-off test passed three consecutive runs before this existed and
/// then failed in the full suite — three passes is not evidence about a race,
/// only about how often it happens to lose. Preventing the interleaving is the
/// only fix that holds.
fn env_guard() -> parking_lot::MutexGuard<'static, ()> {
    // parking_lot per the workspace lint: no poisoning, so a panicking test
    // cannot leave the gate permanently unusable for the rest.
    static GATE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    GATE.lock()
}

#[test]
fn test_affinity_is_off_unless_requested() {
    let _g = env_guard();
    // Guard the default. If this ever starts returning Some() with the variable
    // unset, every user silently gets a pin chosen by us rather than by them.
    if std::env::var("NULLHERZ_AUDIO_CPU").is_err() {
        assert!(
            ipc_layer::apply_audio_thread_affinity().is_none(),
            "audio thread affinity applied without NULLHERZ_AUDIO_CPU being set"
        );
    }
}

#[test]
fn test_out_of_range_cpu_is_refused_not_clamped() {
    let _g = env_guard();
    // Clamping would silently pin to some other CPU than the one asked for,
    // which is the kind of "helpful" behaviour that makes a misconfiguration
    // impossible to notice. Refusing and saying so is the correct response.
    let total = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    // SAFETY-adjacent: this test sets a process-wide env var. It is the only
    // test that touches NULLHERZ_AUDIO_CPU with an out-of-range value.
    unsafe { std::env::set_var("NULLHERZ_AUDIO_CPU", (total + 100).to_string()) };
    let note = ipc_layer::apply_audio_thread_affinity();
    unsafe { std::env::remove_var("NULLHERZ_AUDIO_CPU") };

    let note = note.expect("an out-of-range request must report something, not pass silently");
    assert!(
        note.contains("not pinning"),
        "out-of-range CPU must be refused; got {note:?}"
    );
    assert!(
        note.contains(&total.to_string()),
        "the refusal must say how many CPUs the machine actually has; got {note:?}"
    );
}

#[test]
fn test_non_numeric_cpu_is_refused() {
    let _g = env_guard();
    unsafe { std::env::set_var("NULLHERZ_AUDIO_CPU", "first") };
    let note = ipc_layer::apply_audio_thread_affinity();
    unsafe { std::env::remove_var("NULLHERZ_AUDIO_CPU") };

    let note = note.expect("a malformed request must report something");
    assert!(
        note.contains("not pinning"),
        "a non-numeric CPU must be refused rather than defaulting to 0; got {note:?}"
    );
}

#[test]
fn test_siblings_are_reported_and_exclude_self() {
    // The sibling list is what makes a pin decision informed rather than a
    // guess. It must never contain the CPU itself, or a caller excluding
    // "siblings" from the worker pool would exclude the audio CPU too and then
    // wonder why nothing runs there.
    let total = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    for cpu in 0..total {
        let sibs = ipc_layer::cpu_siblings(cpu);
        assert!(
            !sibs.contains(&cpu),
            "cpu_siblings({cpu}) includes {cpu} itself: {sibs:?}"
        );
        for &s in &sibs {
            assert!(s < total, "cpu_siblings({cpu}) returned out-of-range CPU {s}");
        }
    }
}

#[test]
fn test_sibling_relation_is_symmetric() {
    // If A lists B as a sibling, B must list A. An asymmetric parse would make
    // "keep workers off the audio core's sibling" work in one direction only.
    let total = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    for cpu in 0..total {
        for s in ipc_layer::cpu_siblings(cpu) {
            assert!(
                ipc_layer::cpu_siblings(s).contains(&cpu),
                "sibling relation is asymmetric: {cpu} lists {s}, but {s} does not list {cpu}"
            );
        }
    }
}

#[test]
fn test_isolation_detection_agrees_with_the_kernel() {
    // Only assert the read is consistent with the files it claims to read —
    // this machine's boot parameters are not the test's business.
    #[cfg(target_os = "linux")]
    {
        let isolated = std::fs::read_to_string("/sys/devices/system/cpu/isolated")
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let nohz = std::fs::read_to_string("/sys/devices/system/cpu/nohz_full")
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        assert_eq!(
            ipc_layer::has_isolated_cpus(),
            isolated || nohz,
            "has_isolated_cpus() disagrees with /sys — the pinning advice would be wrong"
        );
    }
}
