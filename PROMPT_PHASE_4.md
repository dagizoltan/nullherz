### **Architectural Directive: Nullherz Phase 4 - Production Studio Hardening & Performance**

**Context:**
Nullherz has successfully stabilized its core execution plane with fixed-size array topologies, O(1) metadata transfers via `Arc<SampleMetadata>`, and an optimized processing kernel that minimizes sub-block fragmentation. The system is now ready to move beyond "DNA-centric" experimentation and toward professional-grade studio and performance features.

**Objective:**
Evolve the engine toward traditional high-performance DAW features, including sophisticated signal routing, automated latency compensation, and robust external plugin integration.

**Core Tasks:**

1.  **Dynamic Plugin Delay Compensation (PDC)**
    *   Implement a dynamic PDC algorithm in the `GraphCompiler`.
    *   Traverse the DAG and utilize the `latency_samples()` method from the `AudioProcessor` trait to calculate required look-ahead buffers for each node path.
    *   Update the `GraphExecutor` to insert ring-buffer based delays on "fast" paths to ensure phase-coherent summing at merge points.

2.  **Multi-Bus Studio Architecture**
    *   Refactor `MixerManager` to support a "Studio Layout" beyond the 4-deck DJ model.
    *   Introduce `Aux Bus` and `Master Bus` nodes with dedicated send/return routing.
    *   Harder the `SummingProcessor` with SIMD-optimized saturation kernels (tanh/soft-clip) to provide a "Musical Master" ceiling.

3.  **Sidecar IPC & SDK Standardisation**
    *   Harden the `fx-runtime` for non-Wasm (native process) sidecars.
    *   Implement a shared-memory (SHM) fast-path for MIDI events and parameter automation between the `Conductor` and external processes, reducing TCP/UDP overhead.
    *   Standardize the `sidecar-sdk` to support "Side-Chain Input" by allowing processors to request additional physical buffer assignments during registration.

4.  **High-Performance Disk Streaming**
    *   The current `SampleBuffer` is entirely memory-resident. Implement a `StreamingSamplerProcessor`.
    *   Utilize a background thread-pool in `nullherz-conductor` to manage ring-buffer pre-filling from disk (WAV/FLAC) based on current playback head positions.
    *   Ensure the RT-thread only interacts with pre-allocated memory-mapped regions or lock-free queues.

5.  **Traditional MIDI Engine Refinement**
    *   Implement a `MidiSequenceKernel` supporting standard `.mid` ingestion.
    *   Add a real-time "Quantize" transformation to the `SequencerProcessor` with adjustable swing and strength.

**Constraints:**
*   **RT-Hygiene:** Maintain absolute zero-allocation in the `audio-core`.
*   **Backwards Compatibility:** Ensure existing `MixerCommand` and `TopologyCommand` structures remain compatible.
*   **Resource Caps:** Respect the `MAX_NODES` (64) and `MAX_CHANNELS` (16) limits enforced by the new array-backed topology.

**Definition of Done:**
*   PDC verified by measuring zero-phase deviation between a latent path and a dry path in `integration_tests.rs`.
*   External sidecar running with < 1ms IPC jitter.
*   Multi-track WAV playback (4+ tracks) running without memory spikes or dropouts using the new streaming engine.
