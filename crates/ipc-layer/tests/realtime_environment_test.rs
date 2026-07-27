//! Memory locking must actually lock memory, and the environment report must
//! reflect the machine it runs on.
//!
//! `mlockall` returning 0 is not proof: these read back `VmLck` from
//! `/proc/self/status` so the test fails if pages are not genuinely resident and
//! pinned. Without that, a silent regression (wrong flags, a stubbed
//! implementation) would leave the audio thread swappable while the startup log
//! claimed otherwise — which is worse than not trying, because it removes the
//! warning too.

#[test]
#[cfg(target_os = "linux")]
fn test_lock_memory_actually_locks_pages() {
    fn vmlck_kib() -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("VmLck:"))
                    .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
            })
            .unwrap_or(0)
    }

    let before = vmlck_kib();

    match ipc_layer::lock_memory() {
        Ok(()) => {
            let after = vmlck_kib();
            assert!(
                after > before,
                "lock_memory() reported success but VmLck did not grow \
                 ({before} KiB -> {after} KiB) — pages are not actually locked"
            );
            // Sanity: a process that has locked its address space holds a
            // non-trivial amount. Anything under a megabyte means something is
            // wrong with the flags rather than with the limit.
            assert!(
                after >= 1024,
                "VmLck is only {after} KiB after mlockall — MCL_CURRENT does not \
                 appear to have taken effect"
            );
        }
        Err(e) => {
            // Legitimate on a machine with a low RLIMIT_MEMLOCK. The contract is
            // that it fails loudly rather than pretending, so assert the limit
            // really is the reason.
            let limit = ipc_layer::memlock_limit().unwrap_or(0);
            assert!(
                limit != u64::MAX && limit < 64 * 1024 * 1024,
                "lock_memory() failed with '{e}' but RLIMIT_MEMLOCK is {limit} \
                 bytes, which should have been enough — the failure is not a \
                 limit problem"
            );
        }
    }
}

#[test]
fn test_memlock_limit_is_readable() {
    // The environment report leans on this; if it silently returns None the
    // "raise your memlock limit" advice never appears.
    #[cfg(target_os = "linux")]
    assert!(
        ipc_layer::memlock_limit().is_some(),
        "RLIMIT_MEMLOCK could not be read, so the startup report cannot advise on it"
    );
}

#[test]
fn test_environment_report_is_specific_and_actionable() {
    // Whatever this machine's configuration, every warning must name the problem
    // and what to do about it — a warning nobody can act on is noise, and noise
    // is how the real ones get ignored.
    for w in ipc_layer::realtime_environment_warnings() {
        assert!(
            w.len() > 40,
            "realtime warning is too terse to act on: {w:?}"
        );
        let actionable = w.contains("performance")
            || w.contains("limits.d")
            || w.contains("memory locking");
        assert!(
            actionable,
            "realtime warning states a problem without a remedy: {w:?}"
        );
    }
}

#[test]
fn test_governor_is_reported_when_the_platform_exposes_one() {
    // Not asserting a particular governor — that is the user's system setting.
    // Asserting only that when the kernel exposes one, we can read it, so the
    // powersave warning is not silently skipped.
    #[cfg(target_os = "linux")]
    if std::path::Path::new("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").exists() {
        let g = ipc_layer::cpu_governor();
        assert!(g.is_some(), "governor file exists but cpu_governor() returned None");
        assert!(!g.unwrap().is_empty(), "governor read back empty");
    }
}
