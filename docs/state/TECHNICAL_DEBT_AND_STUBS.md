# Nullherz Technical Debt & Stubs Log

**Author:** Senior Lead Audio & Rust Systems Architect
**Status:** PRODUCTION BETA
**Date:** July 2026

This document lists the open technical debt, stubs, and prototype logic verified directly in the codebase. Identifying and cataloging these items with precise file paths allows the engineering team to address them systematically without architectural disruption.

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
- **Threaded Audio Backend Xrun Blindness**:
  - *Location*: `crates/nullherz-backends/src/threaded.rs`.
  - *Detail*: The software fallback Threaded backend clocks callbacks using an interval sleep loop. It cannot programmatically detect or log hardware-level underruns (xruns) under adversarial scheduler loads, unlike the ALSA or PipeWire backends.

### 1.4 Unwired Processor: Delay
- **`DelayFactory` never registered**:
  - *Location*: `crates/nullherz-processors/src/factory.rs` (`DelayFactory`), `crates/nullherz-processors/src/registry.rs` (`ProcessorRegistry::new` / `register_defaults`).
  - *Detail*: `DelayProcessor` is a complete, unit-tested fractional Hermite-interpolated delay line (its direction bug was fixed in commit `4404aaf`, 2026-07-21), but `DelayFactory` is not among the 24 factories registered in `ProcessorRegistry`. The processor is therefore unreachable through the engine's `create_by_id`/`create_by_name` path — it is exercised only by its own `#[test]`s. Either register it (one line in `register_defaults`) so chorus/flanger/modulated-delay inserts can instantiate it, or move it behind an explicit feature so the dead-factory state is intentional. Until then, the recent DSP fix guards a code path no live graph reaches.

### 1.5 User Interface (UI) Placeholders
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
