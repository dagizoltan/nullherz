# Nullherz Technical Debt & Stubs Report (Updated July 18, 2026)

This document tracks remaining stubs and prototype logic, verified against the code. Items are listed with file evidence so they can be re-checked cheaply. Full suite: **127/127 green** as of 2026-07-18.

---

## 1. Open Items (Verified in Code)

### Clock Sync (`nullherz-conductor/src/ptp_engine.rs`, `nullherz-traits`)
- **SO_TIMESTAMPING not wired into the sync engine**: the rewritten `PtpEngine` (typed SYNC/DELAY_REQ/DELAY_RESP protocol, measured path delay) timestamps arrivals with `ClockProvider::get_system_time_ns()`; `PtpClockProvider::recv_with_timestamp` (hardware RX timestamps) exists but reads from its own socket and is not yet integrated into the engine's recv path. Software timestamps bound accuracy to scheduler latency (~tens of µs).
- **`SystemClockProvider::synchronize_with_master` is a no-op** (`nullherz-traits/src/lib.rs`, "Placeholder for PTP sync logic") — only `PtpClockProvider` actually disciplines.
- **No Best-Master-Clock election**: master/slave roles are static constructor flags.

### Genetic Cloud (`nullherz-dna/src/lib.rs`)
- **Latent-space projection matrix is mocked** (`lib.rs` ~line 1412) for dimensionality reduction.
- **Identity pinning is TOFU**: peer keys are pinned trust-on-first-use with key-change rejection; there is no out-of-band verification or revocation. `libp2p` migration remains the Q3 directive.

### WASM Runtime (`fx-runtime/src/wasm_runtime.rs`)
- **Zero-copy SHM mapping is a placeholder** (~line 64): guest access to the shared command buffer currently goes through a copy; "true zero-copy mapping" is noted in-code as pending.

### Execution Plane
- **Engine metrics feedback loop** (`audio-core/src/engine/metrics.rs` ~line 84): internal telemetry pulse is a placeholder for Conductor feedback.
- **Spectral boundary handling** (`nullherz-processors/src/spectral.rs`): arbitrary non-power-of-two block sizes in the spectral domain still need hardening (no explicit TODO markers remain, but coverage is limited to sizes ≤ 1024).

### UI (`nullherz-inspector`)
- **Offline mock fallbacks**: Settings→Network, Settings→MIDI, and Genetic Cloud views intentionally present labeled mock devices/peers when nothing is detected. Acceptable for demos; should be gated out of any production build.
- **Session restore is disabled/mocked** (`views/settings/preferences.rs`).
- **Breeder View**: transfusion progress bar tracks active DNA blends but lacks sub-block pipeline progress telemetry.

### Execution Plane (found by the survival harness, July 18)
- **Period sizes above `MAX_BLOCK_SIZE` (256) crash the RT thread**: the graph indexes its internal `AudioBlock` buffers with period-global offsets, so the second sub-block of a 512-sample period overruns them (`graph/mod.rs` slice panic). Now clamped with a warning at `BackendManager::start`; the deeper fix (per-sub-block internal indexing or larger pool blocks) is open if >256 periods are ever needed.
- **Threaded backend is xrun-blind**: it software-clocks the callback and cannot detect budget overruns, so a survival PASS on Threaded is provisional (the harness prints a warning when peak block time exceeds the period budget). Real xrun accounting requires ALSA/PipeWire runs.
- **Startup block-time spike**: first blocks after deck load peaked at ~19 ms vs a 5.8 ms budget in the smoke run (steady-state mean 1.2 ms). Likely allocation/page-faulting on track load; worth profiling before the 1-hour hardware run.

### Lint Backlog
- **174 clippy style lints** (collapsible_if let-chains, auto-deref, type_complexity, needless_range_loop, missing `# Safety` docs). The CI clippy job is advisory until these reach zero, then flips to a hard gate. All RT-safety (disallowed-methods/types) lints are resolved or explicitly scoped.

---

## 2. Resolved Items (Hardened — kept for history)

### July 18, 2026 hardening pass
- **Inspector routing test**: [RESOLVED] `test_inspector_command_routing_to_conductor` now uses the Mock backend and poll-with-timeout instead of sleep-based sync; suite fully green.
- **PTP path delay**: [RESOLVED] `PtpEngine` rewritten with a typed SYNC/DELAY_REQ/DELAY_RESP protocol; round-trip is measured (offset-free four-timestamp computation), EMA-filtered (1/8), plausibility-clamped (≤100 ms), replacing the fixed 1 ms assumption. Legacy 8-byte SYNC still accepted.
- **PTP engine never ran on Linux**: [RESOLVED] `PtpClockProvider` and `PtpEngine` both bound `0.0.0.0:319`; the second bind failed with EADDRINUSE and was silently discarded. The engine socket now sets SO_REUSEADDR/SO_REUSEPORT.
- **Peer signature cache**: [RESOLVED] Replaced the write-only `peer_signatures` mock with `peer_keys` — per-peer ed25519 identities pinned trust-on-first-use via HANDSHAKE/IDENTITY, with key-change rejection; `request_dna` requires the DNA signer to match the pinned identity.
- **Private key disclosure (security)**: [RESOLVED] The DnaServer HANDSHAKE handler responded with the node's *private* signing key as its IDENTITY; it now sends only the derived public verifying key. Regression-tested.
- **parking_lot migration**: [RESOLVED] All remaining `std::sync::Mutex` usage in the orchestration/UI/network planes migrated to `parking_lot::Mutex` (192 lint hits); `tokio::sync::Mutex` async sites untouched.
- **`fallback_*.redb` litter**: [RESOLVED] Fallback library DBs now go to the system temp dir; root files deleted.
- **`alsa_test.rs`**: [RESOLVED] Moved to `crates/nullherz-backends/examples/alsa_dlopen_probe.rs` (CI-compiled).
- **distributed-sidecar workspace coverage**: [RESOLVED] Added to workspace members; latent rot (missing trait import, `AudioBlock` `_pad`, non-`Send` futures) fixed.
- **CI gate**: [ADDED] check with `-D warnings`, full test suite on Mock backend, advisory clippy, weekly Kani proofs.

### Earlier passes

- **Orchestrator Calibration**: [RESOLVED] Dynamic calculation based on engine sample rate implemented.
- **Remote Audio Send**: [RESOLVED] Refactored from per-block `tokio::spawn` to efficient batching.
- **Isolator Filters**: [OPTIMIZED] 4x unrolled kernels and exact Linkwitz-Riley coefficient generation.
- **Offline Rendering**: [RESOLVED] Replaced `unsafe` pointer hack with safe mutable access in `bounce.rs`.
- **DNA Mutation Targeting**: [RESOLVED] Replaced first-ID heuristic with precise `resource_id` resolution.
- **UI Placeholders**: [RESOLVED] Account and Metrics views now utilize live telemetry instead of mocks.
- **Waveform Rendering**: [OPTIMIZED] Precise LOD selection in `waveform_renderer.rs`; MIP-level generation in `audio-dsp/util.rs`.
- **DNA Transfusion Builder**: [RESOLVED] `DnaCommand::pack_transfusion` eliminates unsafe byte-packing in the Breeder view.
- **Decoupled Synchronization**: [RESOLVED] `parking_lot::Mutex` across UI and rendering components (no poisoning, RT-lint compliant).
- **Gossip Signatures**: [RESOLVED] `GOSSIP_SIGNED` ed25519 payload validation with local-network sync test suite (see open item above for the remaining peer-cache mock).
- **SHM Host Exports**: [RESOLVED] Direct memory-mapping getters (`get_shared_command_buffer_ptr`, etc.) integrated for guest SDK (zero-copy itself still open, see §1).
- **wasm_simd128 Kernels**: [RESOLVED] Implemented in `audio-dsp` (`simd_vec.rs`, spectral `complex_mul_accumulate_wasm_simd`).
- **OLA Time-Stretch Ratio**: [RESOLVED] Corrected ratio semantics; RMS transient detector added (July 2026).
- **Step-Sequencer Telemetry**: [RESOLVED] Refactored per-step playback telemetry with tests up to slot 512; `period_size` made configurable via `SystemConfig`.

---

## 3. Strategic Technical Debt

### Distributed Networking
- **Jitter Resilience**: [HARDENED] Jitter Buffer implements aggressive clock recovery; target-size invariance is Kani-proved.
- **RDMA Path**: [RESEARCH] Zero-copy RDMA return path (Protocol Type 7) for distributed AudioBlocks remains a long-term research goal.

### Intelligence Plane
- **DNA-Aware Sequencing**: [PROTOTYPE] `GeneticSequencer::evolve_pattern` exists as a heuristic kernel; real-time mutation of MIDI patterns based on Rhythmic DNA is the next step.
