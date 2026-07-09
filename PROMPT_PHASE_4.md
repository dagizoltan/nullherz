### **Architectural Directive: Nullherz Phase 4 - Advanced Synthesis & Cryptographic Hardening**

**Context:**
The Nullherz system has transitioned to a fixed-size, array-backed topology system with a decoupled, trait-based Sample Registry. Real-time safety is enforced via `Arc<SampleMetadata>` for zero-copy transfers and an optimized `StandardKernel` that batches simultaneous commands to minimize sub-block fragmentation. Node indices are standardized as `u32`, and the processor factory system is capability-aware.

**Objective:**
Proceed with the implementation of Stage 7 synthesis features, harden the federated genetic cloud with cryptographic verification, and restore logic coverage for the DNA intelligent curation layer.

**Core Tasks:**

1.  **Stage 7: Frequency-Domain Spectral Morphing**
    *   Implement the `Phase Vocoder` logic within `ProcessorGraph::process_parallel`.
    *   Utilize the pre-allocated SIMD-aligned FFT buffers to perform magnitude-spectrum blending between the `old_path_buffers` and active `buffers` during structural topology swaps.
    *   Ensure the morphing logic respects the `morph_duration_samples` parameter and provides seamless timbre transitions without phase-cancellation artifacts.

2.  **Cryptographic Cloud Hardening**
    *   The `DiscoveryService` currently marks `NEW_DNA` announcements as an audit risk. Implement Ed25519 signing for these messages.
    *   Harder the Gossip protocol: Ensure peers verify the signature of the serialized DNA against the `signer_public_key` before persisting it to the `LibraryDatabase`.
    *   Transition the Gossip pull mechanism to a fully non-blocking task using the existing `tokio` integration in `nullherz-conductor`.

3.  **DNA Logic Restoration & SIMD Kernels**
    *   Restore and update the `test_smart_crate_filtering` and `test_smart_crate_genre_and_bpm` tests in `nullherz-dna`. These were sidelined during the `Arc` refactor and require boilerplate updates for the new fixed-size metadata structures.
    *   Expand the `audio-dsp` Wasm-SIMD pathways. Identify scalar loops in the `AnalysisKernel` (specifically Formant Peak detection) and implement 4-wide or 8-wide SIMD kernels to reduce analysis latency.

4.  **Hardware-Aligned Telemetry**
    *   Optimize the `TelemetryProducer` path. Instead of `AtomicU64` arrays, investigate the use of a lock-free circular buffer for high-resolution cycle-count history to support "Execution Plane Jitter" visualization in the Inspector UI.

**Constraints:**
*   **RT-Hygiene:** Absolute zero allocations on the RT thread. Use the `assert_rt_safe!` macro.
*   **ABI Stability:** Maintain the `u32` index standard for all `TopologyCommand` and `MixerCommand` variants.
*   **Memory Pressure:** Continue using the `Box<[T; MAX_NODES]>` pattern for large arrays in the `GraphCompiler` to prevent stack overflows.

**Definition of Done:**
*   Successful 1024-point FFT-based spectral morph verified in a test case.
*   Signed DNA exchange verified between two mock discovery peers.
*   Restored logic tests in `nullherz-dna` passing with 100% coverage of the `GeneticLibrary` query logic.
