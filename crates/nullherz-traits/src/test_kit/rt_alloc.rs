//! An allocation guard that makes the Law of Zero Allocation checkable.
//!
//! # Why this exists
//!
//! `AGENTS.md` makes "no heap allocation on the RT path" the system's first
//! invariant. Nothing enforced it. `ConformanceSuite::verify_zero_allocation`
//! was a stub that returned `Ok(())` with a comment saying hooking the
//! allocator had been left for later, and `docs/archive/SYSTEM_STATUS.md`
//! recorded the result as "[VERIFIED] ... via the gauntlet conformance test
//! suite" — a suite that had no callers. So the invariant was checked by code
//! review alone, and a violation duly went in: `TopologyCoordinator::commit`
//! called `GraphCompiler::compile` on the audio thread, boxing ~373 KB per
//! commit.
//!
//! # How it works
//!
//! [`CountingAllocator`] wraps any allocator and, while a thread is ARMED,
//! counts every `alloc`/`realloc` on that thread. Arming is a thread-local
//! `Cell<bool>`, so the guard costs one predictable branch per allocation and
//! observes only the thread under test.
//!
//! # Installing it
//!
//! A global allocator is per-BINARY, so each test binary that wants the check
//! opts in with one line:
//!
//! ```ignore
//! #[global_allocator]
//! static ALLOC: nullherz_traits::test_kit::rt_alloc::CountingAllocator<std::alloc::System> =
//!     nullherz_traits::test_kit::rt_alloc::CountingAllocator::new(std::alloc::System);
//! ```
//!
//! # The thing that makes it honest
//!
//! [`is_installed`] actually allocates and checks the counter moved. Without
//! it, a test binary that forgot the `#[global_allocator]` line would count
//! zero allocations and report a clean pass — reproducing, exactly, the
//! silent-success failure this module exists to end. Callers must treat
//! "not installed" as a failure, not as a pass.

use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;

thread_local! {
    /// Count allocations on this thread while set.
    static ARMED: Cell<bool> = const { Cell::new(false) };
    /// Allocations seen on this thread since the last [`arm`].
    static COUNT: Cell<usize> = const { Cell::new(0) };
    /// Largest single allocation seen, in bytes — a 373 KB box and a 16-byte
    /// one are very different findings on an RT thread.
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

/// Wraps an allocator and counts allocations made by armed threads.
///
/// `dealloc` is deliberately NOT counted. Freeing on the audio thread is its
/// own hazard (a multi-megabyte `free` is a dropout) but it is not what this
/// counter is for, and counting it would make every `Vec` temporary in a
/// non-RT setup path look like two findings instead of one.
pub struct CountingAllocator<A> {
    inner: A,
}

impl<A> CountingAllocator<A> {
    pub const fn new(inner: A) -> Self {
        Self { inner }
    }
}

#[inline(always)]
fn record(size: usize) {
    // `ARMED.try_with` rather than `with`: during thread teardown the TLS may
    // already be destroyed, and panicking inside the allocator is unrecoverable.
    let _ = ARMED.try_with(|armed| {
        if armed.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
            let _ = LARGEST.try_with(|l| {
                if size > l.get() {
                    l.set(size);
                }
            });
        }
    });
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { self.inner.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.inner.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { self.inner.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        unsafe { self.inner.realloc(ptr, layout, new_size) }
    }
}

/// What an armed region observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllocReport {
    pub count: usize,
    pub largest_bytes: usize,
}

impl AllocReport {
    pub fn is_clean(&self) -> bool {
        self.count == 0
    }
}

impl std::fmt::Display for AllocReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.count == 0 {
            write!(f, "no allocations")
        } else {
            write!(f, "{} allocation(s), largest {} bytes", self.count, self.largest_bytes)
        }
    }
}

/// Start counting on this thread, resetting the counters.
pub fn arm() {
    COUNT.with(|c| c.set(0));
    LARGEST.with(|l| l.set(0));
    ARMED.with(|a| a.set(true));
}

/// Stop counting and report what happened since [`arm`].
pub fn disarm() -> AllocReport {
    ARMED.with(|a| a.set(false));
    AllocReport {
        count: COUNT.with(|c| c.get()),
        largest_bytes: LARGEST.with(|l| l.get()),
    }
}

/// Run `f` with the counter armed, and report.
pub fn measure<F: FnOnce()>(f: F) -> AllocReport {
    arm();
    f();
    disarm()
}

/// Is a [`CountingAllocator`] actually the global allocator in this binary?
///
/// Proven by allocating and checking the counter moved, not assumed. A test
/// that skips this and finds zero allocations has learned nothing: an
/// uninstalled guard and a clean RT path produce the identical result.
pub fn is_installed() -> bool {
    let report = measure(|| {
        // `Box` rather than `Vec::new()`: an empty Vec does not allocate.
        let probe = Box::new([0u8; 64]);
        std::hint::black_box(&probe);
    });
    report.count > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // The crate's own test binary installs the guard, so these exercise it for
    // real rather than asserting the shape of an uninstalled no-op.
    #[global_allocator]
    static ALLOC: CountingAllocator<std::alloc::System> =
        CountingAllocator::new(std::alloc::System);

    #[test]
    fn guard_reports_itself_installed() {
        assert!(is_installed(), "the test binary declares the CountingAllocator");
    }

    #[test]
    fn counts_allocations_only_while_armed() {
        let before = measure(|| {});
        assert!(before.is_clean(), "an empty region allocates nothing, got {before}");

        let during = measure(|| {
            let v: Vec<u8> = Vec::with_capacity(4096);
            std::hint::black_box(&v);
        });
        assert_eq!(during.count, 1, "one Vec::with_capacity is one allocation");
        assert!(during.largest_bytes >= 4096, "largest should be the 4 KB request, got {during}");

        // Unarmed work must not leak into the next measurement.
        let v: Vec<u8> = Vec::with_capacity(8192);
        std::hint::black_box(&v);
        let after = measure(|| {});
        assert!(after.is_clean(), "allocations outside an armed region must not count, got {after}");
    }

    #[test]
    fn arithmetic_does_not_allocate() {
        let report = measure(|| {
            let mut acc = 0.0f32;
            for i in 0..1024 {
                acc += (i as f32).sin();
            }
            std::hint::black_box(acc);
        });
        assert!(report.is_clean(), "pure float work must not allocate, got {report}");
    }
}
