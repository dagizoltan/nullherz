# Nullherz: 3-Month Strategic Roadmap

**Timeline:** July 2026 – September 2026
**Focus:** Stability, UI Maturity, and Market Readiness

---

## Month 1: Stabilization & The 5 Layers of Transfusion
*Goal: Finalize the primary control interfaces and ensure total signal reliability.*

*   **MIDI Mapping System [High Priority]:**
    *   Implement the sidecar-based MIDI bridge to support standard DJ controllers (Pioneer, Denon, Native Instruments).
    *   Develop a declarative MIDI mapping format (JSON/YAML) for custom user configurations.
*   **Intelligence Perfection:**
    *   Harden the `AnalysisWorker` BPM detection with additional histogram smoothing to eliminate "double/half bpm" errors.
    *   Stabilize Root Key detection and propagate results to the UI for harmonic mixing indicators.
*   **Safe-Mode & Recovery:**
    *   Implement "Soft Fallback" where a failing DSP sidecar is replaced by a low-overhead gain/bypass node instantly to maintain signal continuity.
    *   Add automated X-RUN detection and telemetry reporting to the Conductor.

---

## Month 2: UI Enrichment & Evolutionary Synthesis
*Goal: Restore legacy visualizations and implement advanced creative features.*

*   **Advanced Visualizers:**
    *   Re-integrate the high-precision Goniometer and Spectrum Analyzer as modular views in the Metrics tab.
    *   Optimize the Wide Oscillator monitor for lower GPU overhead using simplified vertex buffers.
*   **Harmonic Mixing (Key Sync):**
    *   Implement the `KeySync` processor (Planned in Feature Matrix).
    *   Enable real-time pitch shifting (+/- 12 semitones) without affecting playback tempo.
*   **Advanced Looping & Slicing:**
    *   Introduce "Beat Jump" functionality (1, 2, 4, 8, 16 beats) phase-locked to the transport.
    *   Implement a "Loop Slicer" mode in the Sampler view for instant MPC-style remixing.

---

## Month 3: Ecosystem & Scale (Infrastructure for Growth)
*Goal: Scale the library and prepare for distributed processing.*

*   **Library Optimization:**
    *   Benchmark and optimize `redb` queries for track collections exceeding 100,000 entries.
    *   Implement a "Crating" system for advanced playlist management and smart folders.
*   **Remote Sidecar Protocol:**
    *   Prototype network-transparent DSP distribution, allowing a second machine to handle heavy spectral processing via the IPC layer.
*   **Public Alpha Preparation:**
    *   Finalize the "Setup Wizard" for backend configuration (ALSA/JACK/PipeWire).
    *   Complete the Conformance Suite for all 1st-party processors to ensure 100% reset determinism.
*   **Documentation & SDK:**
    *   Formalize the Sidecar SDK documentation to allow 3rd-party developers to build Nullherz-native plugins in Rust.

---

## Milestone Summary
| Month | Primary Objective | Key Deliverable |
| :--- | :--- | :--- |
| **Month 1** | Control Reliability | **Universal MIDI Mapping Engine** |
| **Month 2** | Creative Maturity | **Harmonic Engine (Key Sync)** |
| **Month 3** | Scalability | **Ecosystem SDK & High-Scale Library** |

---

**Strategic Approval:** *Lead Nullherz Architect & Senior Producer*
