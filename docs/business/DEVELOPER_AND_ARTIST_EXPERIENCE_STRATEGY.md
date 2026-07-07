# Nullherz: Developer & Artist Experience (DX/AX) Strategy

**Focus:** Reducing Friction and Elevating the Creative Flow.

---

## 1. Artist Experience (AX): The "Seed to Bloom" Workflow
To win the market, Nullherz must provide an immediate creative "high."
*   **The First 10 Minutes:** A new user should be able to:
    1.  Load a "Seed" (any .wav).
    2.  Capture an "AnaWave" (Instant transfusion to the registry).
    3.  See the sound "Bloom" (Rhythmic DNA applied to a granular synth).
*   **Strategy:** Implement a "Quick Transfuse" button on every deck that bypasses complex menu-diving.

---

## 2. DNA Schema Versioning (SemVer for Sound)
As the `SpectralPersonality` and `RhythmicDNA` schemas evolve, old samples must still sound correct.
*   **Versioned Traits:** Sound DNA includes a `schema_version` field (currently v6) to track genetic evolution.
*   **Backwards Compatibility:** [PARTIAL] The system is designed to support "Legacy Transfusion" kernels; however, as of 2026-07-07, only the v6 kernel is active. Future versions will require explicit legacy kernel paths to ensure bit-identical reproduction.

---

## 3. The "Black Box" Flight Recorder (RT-Debugging)
Audio glitches (X-RUNS) are notoriously hard to debug.
*   **Glitch Capture:** When an X-RUN is detected, the Conductor will dump the last 512ms of telemetry, command bus history, and CPU heatmap to a `glitch_report.json`.
*   **Developer Value:** This allows Sidecar developers to see exactly which parameter update or topology shift triggered the engine's instability.

---

## 4. Visual Language Specification
The "Industrial Steel" aesthetic is a core part of the brand.
*   **Design Tokens:** We will formalize a set of color palettes (e.g., `SignalGlow`, `BrushedAluminum`, `WarningAmber`) and stroke weights.
*   **Technical Transparency:** Every UI element should reveal its underlying technical state (e.g., VUs showing both peak and RMS, knobs showing modulation depth rings).

---

## 5. Summary of Experience Hardenings
1.  **Instant Transfuse:** Native support in the `MixerBridge` for 1-click capture.
2.  **Telemetry Black-Box:** Integration with the `EngineMetrics` for glitch persistence.
3.  **UI Design Guide:** A living document in `nullherz-ui-hal` defining the industrial look.

---

**Experience Invariant:** *Technical complexity must be available, but creative momentum must be effortless.*
