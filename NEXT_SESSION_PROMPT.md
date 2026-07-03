# Nullherz Stage 2 Session Prompt: Genetic Transfusion & Ecosystem Scaling

**Role**: Senior Audio & Rust Systems Architect
**Context**: We have successfully completed the architectural hardening and optimization phase (Alpha Baseline). The engine is now 16-wide SIMD (AVX-512) capable, hardened against NaN/Denormal instability, and features a decoupled Universal MIDI Bridge via Shared Memory.

**Current Technical Baseline**:
1.  **Triple-Plane Isolation**: Orchestration (`nullherz-conductor`), Protocol (`ipc-layer`), and Execution (`audio-core`) are strictly separated.
2.  **Law of Zero Allocation**: The execution path is heap-allocation-free, lock-free, and syscall-free (codified in `AGENTS.md`).
3.  **SIMD Foundation**: `FloatX16` abstraction provides portable 16-wide paths for all core DSP kernels.
4.  **Hardware Ready**: `nullherz-midi` sidecar and `nullherz-setup` wizard enable hardware integration and discovery.
5.  **Signal Resilience**: Universal `is_finite` checks and `-300dB` denormal flushes are active.

**Objective: Move from "Hardened Engine" to "Evolutionary Instrument"**

### Task 1: Stage 2 AnaWaves "Personality Inheritance"
Implement the core logic for the `DnaCommand` recently added to `nullherz-traits`.
*   **Genetic Interpolation**: In `nullherz-dna`, create a high-performance kernel that can interpolate between two `SoundDNA` bit-layouts at audio-rate or block-rate.
*   **Trait Propagation**: Update the `PersonalityInheritanceProcessor` (ProcessorTypeId 150) to consume `DnaCommand` payloads and apply spectral/rhythmic traits to the signal path in real-time.

### Task 2: Advanced Sidecar Sandboxing & SDK
*   **Cgroup Enforcement**: Refine the `SidecarSupervisor` in `fx-runtime` to actively monitor and report cgroup memory pressure events (OOM) via the telemetry stream.
*   **SDK Formalization**: Create a standard `sidecar-sdk` example for a "Spectral Transfuser" plugin that utilizes the new 16-wide SIMD utilities and the `DnaCommand` schema.

### Task 3: UI-Driven Evolution (The "Breeder" View)
*   **Breeding Interface**: In `nullherz-inspector`, implement a new "DNA Breeder" view. This should allow users to select two "Parent" samples from the registry and use a 2D Macro-XY pad to "Transfuse" their genetic traits.
*   **Real-time Visualization**: Use the restored Spectrum and Goniometer metrics to show the "Genetic Drift" of the signal as transfusion bias is adjusted.

### Task 4: High-Scale Library Management
*   **redb Optimization**: Benchmark the `LibraryDatabase` for 100k+ entries. Implement the "Crating" system (smart folders/playlists) mentioned in the 3-month roadmap.

**Critical Constraints**:
*   Maintain strict RT-safety (no allocations in the hot path).
*   Ensure all new DSP logic utilizes the `FloatX16` SIMD path where possible.
*   Adhere to the governance rules defined in `AGENTS.md`.

**Required Reading**: `AGENTS.md`, `SYSTEM_STATUS_REPORT.md`, `TECHNICAL_OPTIMIZATION_LOG.md`.
