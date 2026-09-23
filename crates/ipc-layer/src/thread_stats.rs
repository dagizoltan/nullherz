//! Kernel-side counters for the audio thread, read from `/proc`.
//!
//! # Why this exists
//!
//! The console's real-time ceiling is not DSP cost. Measured on the survival
//! harness, mean block time is **135 us against a 2666 us budget — 5%** — while
//! the peak is **2945 us, 22x the mean**, and that peak arrives regardless of
//! block size. Period size only decides whether the budget can absorb it. So the
//! question that matters is not "how fast is our audio code" but "what stalls
//! this thread for 3 ms", and nothing in the tree could answer it: the harness
//! reported a number with no cause attached.
//!
//! Three candidate causes, and they need different fixes:
//!
//!   * **page faults** — `mlockall` is `MCL_CURRENT` only (deliberately: see
//!     `lock_memory`, where `MCL_FUTURE` turned `RLIMIT_MEMLOCK` into a
//!     process-wide allocation ceiling and killed library scans), so everything
//!     allocated after startup faults lazily. Fix is a memory architecture
//!     change — a pre-faulted, locked arena for the audio path only.
//!   * **preemption** — the thread is descheduled. Fix is `isolcpus` /
//!     `nohz_full` / IRQ affinity, a kernel command line and a reboot.
//!   * **neither** — it is our own code, and the fix is a profiler.
//!
//! `minflt` distinguishes the first, `nonvoluntary_ctxt_switches` the second.
//! One run now answers a question that was otherwise going to be guessed at.
//!
//! # Why from `/proc` and not from the audio thread
//!
//! `getrusage(RUSAGE_THREAD)` per block would be the obvious way and it is the
//! wrong one: it puts a syscall in the hot path to measure whether the hot path
//! is being interrupted by the kernel. At a 135 us mean that is ~1% of budget
//! spent by the instrument, and it perturbs exactly the thing under test.
//!
//! Reading `/proc/self/task/<tid>/…` from the MONITOR thread costs the audio
//! thread nothing. The price is resolution — the harness polls at 16 ms, so a
//! 3 ms stall is located to within a poll window, not to a block. That is
//! sufficient: the question is which CATEGORY of event it is, and a fault storm
//! or a preemption shows up as a jump of thousands against a quiet baseline.
//! Attributing it to an individual block is a later problem, and one worth
//! solving only after the category is known.

use std::sync::atomic::{AtomicU64, Ordering};

/// The audio thread's kernel task id, published by [`crate::set_rt_priority`].
///
/// 0 means "not yet known": either the audio thread has not started or it never
/// asked for real-time priority. A reader must treat 0 as absence rather than as
/// a task id, which is what [`audio_thread_tid`] is for.
static AUDIO_THREAD_TID: AtomicU64 = AtomicU64::new(0);

/// Record the calling thread as the audio thread.
///
/// Called from `set_rt_priority`, which runs ON the audio thread, and
/// unconditionally — before the promotion is known to have succeeded. A thread
/// that asked for real-time and was refused is still the thread whose stalls we
/// care about, and on a machine without `RLIMIT_RTPRIO` or RTKit that is the
/// only case there is.
pub(crate) fn publish_audio_thread_tid() {
    let tid = unsafe { libc::syscall(libc::SYS_gettid) } as u64;
    // Relaxed: a single publication read much later by one monitor thread. The
    // value is a task id, not a pointer, and nothing is ordered against it.
    AUDIO_THREAD_TID.store(tid, Ordering::Relaxed);
}

/// The audio thread's task id, or `None` if it has not identified itself yet.
pub fn audio_thread_tid() -> Option<u64> {
    match AUDIO_THREAD_TID.load(Ordering::Relaxed) {
        0 => None,
        tid => Some(tid),
    }
}

/// The instrument's own cost, in minor faults per sample: `read_thread_counters`
/// reads two `/proc` files with `read_to_string`, which allocates, which faults.
/// Measured at 2. A delta below this explains nothing; judge readings against it
/// rather than against zero.
pub const ATTRIBUTION_FLOOR_FAULTS: u64 = 16;

/// Kernel counters for one thread. Monotonic since thread start, so a stall is a
/// DIFFERENCE between two samples, never a single reading.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ThreadCounters {
    /// Minor faults: a page was mapped without touching disk. This is the
    /// lazy-faulting cost — a growing heap under `MCL_CURRENT` pays it on first
    /// touch, and the kernel may have to find and zero a page to do it.
    pub minor_faults: u64,
    /// Major faults: a page came from disk or swap. On a locked, swap-free audio
    /// path this should be flat at zero; anything else is a serious finding.
    pub major_faults: u64,
    /// Involuntary context switches: the thread was descheduled while it still
    /// had work. Under SCHED_FIFO this should be near zero, and a jump means
    /// something outranked the audio thread or an interrupt ran long.
    pub involuntary_switches: u64,
    /// Voluntary context switches: the thread blocked. Expected once per block
    /// (waiting on the device), so this is the baseline that says the sampler is
    /// working at all.
    pub voluntary_switches: u64,
}

impl ThreadCounters {
    /// `self - earlier`, saturating. Counters are monotonic, so a negative
    /// result would mean the thread was replaced between samples; clamping to
    /// zero reports "no evidence" rather than an underflowed u64, which would
    /// read as billions of faults and look like the smoking gun.
    pub fn since(&self, earlier: &ThreadCounters) -> ThreadCounters {
        ThreadCounters {
            minor_faults: self.minor_faults.saturating_sub(earlier.minor_faults),
            major_faults: self.major_faults.saturating_sub(earlier.major_faults),
            involuntary_switches: self.involuntary_switches.saturating_sub(earlier.involuntary_switches),
            voluntary_switches: self.voluntary_switches.saturating_sub(earlier.voluntary_switches),
        }
    }

    /// Whether anything here could explain a multi-millisecond stall.
    pub fn is_quiet(&self) -> bool {
        self.minor_faults == 0 && self.major_faults == 0 && self.involuntary_switches == 0
    }
}

/// The CPUs a thread is actually allowed to run on, as the kernel reports them
/// (e.g. `"10-11"`).
///
/// The only way to confirm pinning took effect. `sched_setaffinity` returning Ok
/// means the call succeeded, not that the mask is what you intended — a typo in
/// `NULLHERZ_AUDIO_CPUS`, or a cpu id the machine does not have, produces a mask
/// nobody looked at. Reading it back from `/proc` closes that loop.
pub fn thread_cpu_affinity(tid: u64) -> Option<String> {
    let status = std::fs::read_to_string(format!("/proc/self/task/{tid}/status")).ok()?;
    status
        .lines()
        .find(|l| l.starts_with("Cpus_allowed_list:"))?
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

/// Read one thread's counters. `None` if the thread is gone or `/proc` is
/// unavailable (a non-Linux host, or a container that hides it).
///
/// Two files, because the kernel splits them: faults live in `stat` and context
/// switches in `status`. Both are read as whole small files, which is one
/// syscall each and no allocation on any thread that matters.
pub fn read_thread_counters(tid: u64) -> Option<ThreadCounters> {
    let stat = std::fs::read_to_string(format!("/proc/self/task/{tid}/stat")).ok()?;
    let status = std::fs::read_to_string(format!("/proc/self/task/{tid}/status")).ok()?;
    Some(ThreadCounters {
        minor_faults: stat_field(&stat, 10)?,
        major_faults: stat_field(&stat, 12)?,
        involuntary_switches: status_field(&status, "nonvoluntary_ctxt_switches:")?,
        voluntary_switches: status_field(&status, "voluntary_ctxt_switches:")?,
    })
}

/// Field `n` (1-based, as `proc(5)` numbers them) of a `/proc/…/stat` line.
///
/// Splitting the whole line on whitespace is WRONG and is the classic bug here:
/// field 2 is the executable name in parentheses and may itself contain spaces
/// and parentheses, so every field after it shifts by an unpredictable amount.
/// Parsing resumes after the LAST `)` in the line, where field 3 begins.
fn stat_field(stat: &str, n: usize) -> Option<u64> {
    debug_assert!(n >= 3, "fields 1 and 2 are before the comm delimiter");
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm.split_whitespace().nth(n - 3)?.parse().ok()
}

/// The number on a `key: value` line of `/proc/…/status`.
///
/// Matched with the trailing colon included, because `voluntary_ctxt_switches:`
/// is a suffix of `nonvoluntary_ctxt_switches:` — a bare `starts_with` on the
/// shorter key would never reach it, and a `contains` would return whichever
/// line came first.
fn status_field(status: &str, key: &str) -> Option<u64> {
    status
        .lines()
        .find(|l| l.starts_with(key))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The delimiter bug this parser exists to avoid: a thread name containing
    /// spaces and parentheses shifts every field after it.
    #[test]
    fn stat_fields_survive_a_hostile_thread_name() {
        // field:      1   2                    3 4 5 6 7 8 9 10  11 12
        let line = "4242 (evil ) name (x) ) S 1 1 1 0 -1 4194304 1234 0 56 0 rest";
        assert_eq!(stat_field(line, 10), Some(1234), "minflt");
        assert_eq!(stat_field(line, 12), Some(56), "majflt");
    }

    /// `voluntary_ctxt_switches:` is a suffix of the nonvoluntary key. Reading
    /// the wrong one would attribute preemption to blocking, or vice versa —
    /// which is the entire distinction this module was built to make.
    #[test]
    fn the_two_switch_counters_are_not_confused() {
        let status = "Name:\taudio\nvoluntary_ctxt_switches:\t900\nnonvoluntary_ctxt_switches:\t7\n";
        assert_eq!(status_field(status, "voluntary_ctxt_switches:"), Some(900));
        assert_eq!(status_field(status, "nonvoluntary_ctxt_switches:"), Some(7));
    }

    /// The counter must MOVE when the thing it counts happens. A parser that
    /// returned a constant would pass any assertion about a single reading.
    ///
    /// Deliberately not asserting a non-zero baseline: these are PER-THREAD
    /// counters, and a freshly spawned test thread legitimately reads zero
    /// across all four — the thread that allocated its stack and page tables was
    /// its parent. That zero is correct data, not a broken parser, and asserting
    /// against it is what this test did on its first outing.
    #[test]
    #[cfg(target_os = "linux")]
    fn faulting_pages_moves_the_fault_counter() {
        let tid = unsafe { libc::syscall(libc::SYS_gettid) } as u64;
        let before = read_thread_counters(tid).expect("/proc/self/task must be readable on Linux");

        // 64 MB touched one byte per page: ~16k first-touch minor faults, the
        // same mechanism a growing decode heap pays under MCL_CURRENT.
        let mut big = vec![0u8; 64 * 1024 * 1024];
        for i in (0..big.len()).step_by(4096) {
            big[i] = 1;
        }
        std::hint::black_box(&big);

        let delta = read_thread_counters(tid).unwrap().since(&before);
        assert!(
            delta.minor_faults > 1000,
            "touching 64 MB must register thousands of minor faults on THIS thread, saw {} \
             (before {:?})",
            delta.minor_faults, before
        );
        assert!(!delta.is_quiet(), "a fault storm is not a quiet window");
        std::mem::drop(big);
    }

    /// The instrument's own noise floor. An idle window must read as ~nothing,
    /// or the instrument cannot exonerate anything and every stall looks
    /// explained.
    ///
    /// It is NOT zero: `read_thread_counters` calls `read_to_string` twice, which
    /// allocates, which faults. Measured at 2 minor faults per sample. So the
    /// floor is single digits and a reading must be judged against that, not
    /// against zero — the same discipline `audio_dsp::measurement::analyser_floor`
    /// exists to enforce one plane down. A fault storm worth 3 ms is thousands,
    /// three orders clear of this.
    #[test]
    #[cfg(target_os = "linux")]
    fn an_idle_window_reads_near_the_instrument_floor() {
        let tid = unsafe { libc::syscall(libc::SYS_gettid) } as u64;
        let before = read_thread_counters(tid).unwrap();
        let delta = read_thread_counters(tid).unwrap().since(&before);
        assert!(
            delta.minor_faults < ATTRIBUTION_FLOOR_FAULTS,
            "an idle window read {} minor faults; the instrument's own cost should be single \
             digits, and if it is not then nothing can be distinguished from it",
            delta.minor_faults
        );
        assert_eq!(delta.major_faults, 0, "reading /proc must not touch disk");
    }

    #[test]
    fn deltas_never_underflow() {
        let hi = ThreadCounters { minor_faults: 5, ..Default::default() };
        let lo = ThreadCounters { minor_faults: 9, ..Default::default() };
        assert_eq!(hi.since(&lo).minor_faults, 0, "a backwards delta reports no evidence, not u64::MAX");
    }

    /// Absence must be distinguishable from a task id.
    #[test]
    fn an_unpublished_tid_reads_as_none() {
        // The global is process-wide and another test may have published it, so
        // this asserts the mapping rather than the current value.
        assert_eq!(super::AUDIO_THREAD_TID.load(Ordering::Relaxed) == 0, audio_thread_tid().is_none());
    }
}
