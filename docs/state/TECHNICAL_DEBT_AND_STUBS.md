# Nullherz Technical Debt & Stubs Report (Updated July 17, 2026)

This document tracks remaining stubs and prototype logic, verified against the code during the July 17 reverse-engineering pass. Items are listed with file evidence so they can be re-checked cheaply.

---

## 1. Open Items (Verified in Code)

### Test Suite
- **`test_inspector_command_routing_to_conductor` fails deterministically** (`crates/nullherz-inspector/src/main.rs`, in-crate test). It synchronizes with a fixed 100 ms `sleep` while the in-process Conductor is still booting a real ALSA backend, scanning `tracks/`, and binding the DNA server on :9003 — so the `SetMasterDeck('C')` assertion races setup and loses. The command handler itself is correct (`command_handler.rs` `CoreCommand::SetMasterDeck`). Fix: replace sleep-based sync with a readiness signal (or poll with timeout) and use the Mock backend in tests. Until then, "100% test-suite passing" claims must exclude this test (workspace excluding inspector: 117/117 pass as of 2026-07-17).

### Clock Sync (`nullherz-conductor/src/ptp_engine.rs`, `nullherz-traits`)
- **Simplified PTP, not IEEE 1588**: master broadcasts a raw `u64` timestamp to `255.255.255.255:319` at 1 Hz; slaves apply a **fixed 1 ms wire-delay assumption**. There is no Delay_Req/Delay_Resp path-delay measurement. `nullherz-traits/src/lib.rs` still carries a "Placeholder for PTP sync logic" in the default `ClockProvider` path. The PI servo integral clamp is Kani-proved; the delay model is not yet real.

### Genetic Cloud (`nullherz-dna/src/lib.rs`)
- **Peer signature cache is mocked**: `CloudPeerSync.peer_signatures` is documented in-code as "Mock for cryptographic signatures". Payload signing/verification (`GOSSIP_SIGNED`, ed25519) is real, but per-peer trust bookkeeping is not production-grade. `libp2p` migration remains the Q3 directive.
- **Latent-space projection matrix is mocked** (`lib.rs` ~line 1412) for dimensionality reduction.

### WASM Runtime (`fx-runtime/src/wasm_runtime.rs`)
- **Zero-copy SHM mapping is a placeholder** (~line 64): guest access to the shared command buffer currently goes through a copy; "true zero-copy mapping" is noted in-code as pending.

### Execution Plane
- **Engine metrics feedback loop** (`audio-core/src/engine/metrics.rs` ~line 84): internal telemetry pulse is a placeholder for Conductor feedback.
- **Spectral boundary handling** (`nullherz-processors/src/spectral.rs`): arbitrary non-power-of-two block sizes in the spectral domain still need hardening (no explicit TODO markers remain, but coverage is limited to sizes ≤ 1024).

### UI (`nullherz-inspector`)
- **Offline mock fallbacks**: Settings→Network, Settings→MIDI, and Genetic Cloud views intentionally present labeled mock devices/peers when nothing is detected. Acceptable for demos; should be gated out of any production build.
- **Session restore is disabled/mocked** (`views/settings/preferences.rs`).
- **Breeder View**: transfusion progress bar tracks active DNA blends but lacks sub-block pipeline progress telemetry.

### Repo Hygiene
- **`fallback_*.redb` litter**: 12 transient database files (~1.5 MB each) from fallback/test runs sit in the repo root. They are safe to delete; the fallback DB path should point at a temp/cache dir and the pattern should be gitignored.
- **`alsa_test.rs`** (repo root): ad-hoc `dlopen` probe outside any crate; move into `nullherz-backends` tests or delete.
- **`sidecars/distributed-sidecar` is not a workspace member** (built independently); consider adding it to the root `Cargo.toml` members so `cargo check --workspace` covers it.

---

## 2. Resolved Items (Hardened — kept for history)

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
