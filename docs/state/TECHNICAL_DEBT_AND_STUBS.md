# Nullherz Technical Debt & Stubs Log

**Author:** Senior Lead Audio & Rust Systems Architect
**Status:** PRODUCTION BETA
**Date:** July 2026 — §0 added 2026-08-02 from the [reverse-engineering audit](./REVERSE_ENGINEERING_EVALUATION.md)

This document lists the open technical debt, stubs, and prototype logic verified directly in the codebase. Identifying and cataloging these items with precise file paths allows the engineering team to address them systematically without architectural disruption.

---

## 0. Opened by the 2026-08-02 Audit

Ordered by consequence. Full evidence for each is in [REVERSE_ENGINEERING_EVALUATION.md](./REVERSE_ENGINEERING_EVALUATION.md).

### 0.1 `FloatX16`: two of three platform arms do not compile — **BLOCKING for any SIMD work**
- *Location*: `crates/audio-dsp/src/simd_vec.rs:58-104`; consumers at `oscillators.rs:242` and `spectral.rs:203`.
- *Detail*: `RUSTFLAGS="-C target-feature=+avx512f" cargo check -p audio-dsp` fails with 4 errors (`wide 0.7.30` has no `f32x16`). The `wasm32`+`simd128` arm fails with ~24. The root cause is a **shape mismatch between the type and its callers**: `FloatX16` splits three ways on `cfg`, while `oscillators.rs`/`spectral.rs` split two ways (`not(avx512f)`) and then read `.low`/`.high` — fields present only in the third arm. So the fallback representation leaks to consumers and is the only one that can compile.
- *Why it survived*: the reference machine has no AVX-512, and no CI job compiles either arm. A `cfg` arm no build configuration selects is never type-checked.
- *Fix*: decide whether `FloatX16` is a real capability. If yes, repair both arms **and add CI targets that build them**. If no, delete the arms and let the type be what it is (two `f32x8`s). Do not leave a documented capability that does not build.

### 0.2 No `[profile.release]`, no `.cargo/config.toml`, no runtime SIMD dispatch
- *Location*: workspace root (by absence).
- *Detail*: release builds are cargo defaults — `opt-level = 3`, **no LTO, 16 codegen units** — and target the x86-64 SSE2 baseline. `is_x86_feature_detected!` appears zero times in the tree, so there is no runtime dispatch to compensate. Cross-crate inlining is left on the table across the `audio-dsp` → `audio-core` boundary, which is the hot one.
- *Fix*: `lto = "thin"` + `codegen-units = 1` is mechanical and separable from the ISA question; measure it against `bench_console_block`. The ISA question needs its own decision (`target-cpu=native` is not shippable — `SIGILL` on older hardware).

### 0.3 The reachability allowlist has a hole, and already contains a false entry
- *Location*: `crates/nullherz-conductor/tests/reachability_gate_test.rs` — `known_unreachable()` and `test_unreachable_allowlist_has_no_stale_entries`.
- *Detail*: **StereoUtility is listed as unreachable but is instantiated on every deck** (`mixer/src/dj.rs:75`, type 160; confirmed against the real bootstrap). The gate cannot catch it: reachable processors `continue` before the allowlist is consulted, and the staleness guard only checks that a listed name is still *registered*, not still *unreachable*.
- *Fix*: assert the complement — every name in `known_unreachable()` must be **absent** from the instantiated set. Three lines, and it turns the list from prose into a contract. While there, split the list: "installed on demand" (KeySync, DnaMorph — swapped into deck insert slots when a latch engages) is a different state from "not yet wired", and merging them makes the count meaningless.

### 0.4 Three processor type ids are magic integers with no ABI constant
- *Location*: `crates/nullherz-processors/src/factory.rs:236,245,254`; `crates/nullherz-mixer/src/dj.rs:75`.
- *Detail*: StereoUtility (160), Compressor (170) and Analysis (180) are written as bare `ProcessorTypeId(n)` because `nullherz-traits/src/commands.rs` — the ABI crate — has no constant for them. The consequence is already live: `profile_console_nodes` renders 12 of 58 console nodes as `?` with no name, because its type table is built from the named constants.
- *Related*: `ProcessorTypeId::BIQUAD_EQ` (11) is a named constant with **no registered factory**; `registry.create(BIQUAD_EQ, ..)` returns `None`. It appears once, in `setup_honours_sample_rate_test.rs`, where a `let Some(..) else { continue }` guard means that one of the test's nine cases has never executed.
- *Fix*: add the three constants, use them at both definition and call sites, and either register a `BiquadEq` factory or drop the constant and the test row.

### 0.5 Stale doc-comment: `NODE_MAP_SLOTS` headroom
- *Location*: `crates/nullherz-traits/src/telemetry.rs:7-12`.
- *Detail*: the comment says "a 4-deck console registers 30 names". It registers **42** of 64 slots. The constant is still correctly sized, but the margin the comment exists to communicate is a third smaller than stated — and overflow drops an arbitrary subset, since the producer fills from a `HashMap`.

### 0.6 Sidecar / `fx-runtime` verification is the thinnest in the workspace
- *Detail*: `fx-runtime` (process host, cgroup limiter, WASM runtime) has **3 tests**; the eight sidecar binaries have **0** between them. That is the out-of-process failure surface — the path third-party plugins execute on — and it is the least covered code in a workspace with 404 tests.

---

## 1. Verified Core Technical Debt & Stubs

### 1.1 Clock Synchronization & PTP Engine
- **SO_TIMESTAMPING Engine Integration**:
  - *Location*: `crates/nullherz-conductor/src/ptp_engine.rs` and `crates/nullherz-traits/src/clock.rs`.
  - *Detail*: While `PtpClockProvider` implements high-precision raw packet timestamp extraction via `recv_with_timestamp` utilizing `SO_TIMESTAMPING` and `SCM_TIMESTAMPING` (`crates/nullherz-traits/src/clock.rs`), the main synchronization loop in `ptp_engine.rs` timestamps packet arrival via the standard software clock `get_system_time_ns()`. Integrating true hardware RX timestamps directly into the engine's receipt path remains an open goal.
- **System Clock Synchronize Placeholder**:
  - *Location*: `crates/nullherz-traits/src/clock.rs` — `SystemClockProvider::synchronize_with_master`.
  - *Detail*: This function is a no-op placeholder. Standard desktop/VM runs fallback entirely to software monotonic time discipline.
- **Best-Master-Clock (BMC) Election**:
  - *Location*: `crates/nullherz-conductor/src/ptp_engine.rs` — `PtpEngine::new`.
  - *Detail*: Node roles (master vs. slave) are hardcoded as configuration/constructor flags. There is no dynamic Best-Master-Clock algorithm (IEEE 1588 BMC) to automatically elect the highest-quality clock on the subnet.

### 1.2 WASM Sidecar Zero-Copy SHM Mapping
- **Zero-Copy SHM Guest Mapping**:
  - *Location*: `crates/fx-runtime/src/wasm_runtime.rs` (approx. line 64).
  - *Detail*: Guest access to the shared-memory command ring currently triggers a memory copy (`memcpy`) across host/guest boundaries. True zero-copy pointer mapping directly into the guest WASM linear address space remains a Q3 objective.

### 1.3 Execution Plane & Real-Time Safety Gaps
- **Spectral Domain Arbitrary Block Sizes**:
  - *Location*: `crates/nullherz-processors/src/spectral.rs`.
  - *Detail*: The spectral processing kernels are verified to support block sizes of power-of-two ≤ 1024. Arbitrary, non-power-of-two hardware buffer blocks require further buffer padding and overlap-add buffering wrappers to prevent filter leakage or slice overflows.
- **Spectral `set_ir` Allocation on RT Thread**:
  - *Location*: `crates/audio-dsp/src/spectral.rs` (approx. line 231).
  - *Detail*: The partition buffer allocations and FFT calculations are performed inside `apply_topology_mutation`. Although tolerable for short impulse responses, this should be pre-partitioned and packaged as a ready-made mutation payload on the Conductor side to completely shield the RT thread.
- **Retired Sample Buffer Drops**:
  - *Location*: `crates/audio-core/src/engine/resource_recycler.rs`.
  - *Detail*: When a sample buffer is replaced on a deck, the original `Arc<Vec<f32>>` is dropped on the RT thread if the sample registry does not retain a copy. While standard practice retains samples in the registry (reducing drop to a simple atomic decrement), a secondary lock-free garbage collection ring should be introduced to defer all buffer deallocations off-thread.
- **Threaded Audio Backend Xrun Blindness** — **RESOLVED, and the framing was wrong** (re-verified 2026-08-02):
  - *Location*: `crates/nullherz-backends/src/threaded.rs:99-105,128`.
  - *Detail*: The Threaded backend **does** now detect and count underruns: it compares each cycle's elapsed time against `period_ns(period_size, sample_rate)` and increments `xrun_counter` when the cycle overruns its period by more than 20%, surfacing the total through `AudioBackend::xruns()`.
  - *Correction to the original entry*: it also claimed PipeWire reports xruns. It does not — `PipewireBackend` and `JackBackend` both inherit the trait default `xruns() -> None`. **ALSA and Threaded are the only two that measure**, and they measure different things: ALSA counts real device xruns recovered on the audio thread, Threaded counts software deadline misses, which is the only underrun a sleep-paced software clock can observe. That distinction is exactly what `Option<u64>` exists to preserve.

### 1.4 Unwired Processor: Delay — **RESOLVED**
- **`DelayFactory` registered** at `crates/nullherz-processors/src/registry.rs:51` (verified 2026-07-28). It is reachable through `create_by_id`/`create_by_name`, and is declared in `known_unreachable()` as "available for FX chains; not in the default master chain" — a deliberate state, tracked by the reachability gate, rather than an accident.

### 1.5 Unwired Subsystem: Disk Streaming — **PARTLY RESOLVED**
- **`StreamingManager` is now constructed and called** (verified 2026-07-28): held as a field on `Conductor` (`orchestrator.rs:34`, built at `:173`/`:253`) and `start_stream` is invoked from `command_handler.rs:113`. The "never constructed, zero callers" finding below is obsolete.
- **Still open:** roadmap item 2.3 calls for a rewrite regardless — the current implementation downmixes to mono, pushes one sample at a time with 2 ms sleeps, and the `StreamingSampler` consumer remains in `known_unreachable()` (the console never instantiates that node type), so the wiring exists but no live graph exercises it. **The liveness bug described below was never fixed and applies the moment a real graph uses it.**
- *Original finding, retained for the liveness bug:*
  - *Location*: `crates/nullherz-conductor/src/streaming_manager.rs` (`StreamingManager`, `start_stream`/`stop_stream`); `crates/nullherz-processors/src/streaming_sampler.rs` (`StreamingSamplerProcessor`); `crates/nullherz-processors/src/registry.rs` (`StreamingSamplerFactory` registered).
  - *Detail*: The RT consumer `StreamingSamplerProcessor` is registered (reachable via `StreamingSamplerFactory`) and correctly outputs silence on ring-buffer underrun (no block/panic). But `StreamingManager` — the disk decoder + feeder that fills that ring — is **never constructed or held as a field anywhere**; `start_stream`/`stop_stream` have zero callers. So a `StreamingSampler` node has a ring nothing ever fills → it produces silence. The subsystem is half-wired dead code (cf. the Delay processor above).
  - *Latent bug (only if wired)*: both feeder/decoder threads stop via `Arc::strong_count(&ring) <= 1`, but `StreamingManager::start_stream` also inserts an `Arc` clone into `self.streams` (line 31). While that entry lives, the count can never reach 1, so the per-stream threads would **not terminate when the consumer releases its ring** — they'd run (feeder sleep-spinning on a full ring) until `stop_stream()` clears the entire map. Fix when wiring it: track streams so the liveness check excludes the registry's own `Arc` (e.g. compare against a known baseline count, or add explicit per-stream teardown), and set the feeder thread's priority to match its "high-priority" comment (today it is a plain `thread::spawn` at default priority).

### 1.6 User Interface (UI) Placeholders
- **Session Restoration Bypass**:
  - *Location*: `crates/nullherz-inspector/src/views/settings/preferences.rs`.
  - *Detail*: The session restoration checkbox is a non-functional preference, defaulting to a mock state.
- **Breeder Pipeline Telemetry**:
  - *Location*: `crates/nullherz-inspector/src/views/breeder.rs`.
  - *Detail*: The transfusion progress bar displays linear progress but lacks real-time sub-block DSP pipeline feedback metrics from the execution plane.

---

## 2. Resolved Architectural Hardenings (Kept for Context)

- **O(1) Sample Deck Loading**: Resolved track-load heap clones. `SamplerProcessor` has been refactored to adopt shared `Arc` containers instead of deep-cloning sample buffers, preventing large allocations on the RT thread hot-path.
- **PTP Path-Delay Calculation**: Refactored `PtpEngine` from a fixed 1 ms assumption to an active four-timestamp round-trip measurement with EMA smoothing and a 100 ms plausibility filter.
- **Database Mutex Contention**: Migrated track analysis saves to a batched, single-transaction database commit pattern inside `AnalysisWorker` (`crates/nullherz-conductor/src/analysis_worker.rs`), reducing lock contention on `library.redb`.
- **System-Wide `parking_lot` Migration**: Replaced standard library blocking mutexes with lightweight, non-poisoning `parking_lot::Mutex` across the UI, metrics, and orchestration layers to prevent priority inversion.
