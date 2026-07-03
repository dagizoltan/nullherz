# Next Session: Evolutionary Feedback & Distributed DSP

**Objective**: Close the feedback loop between analysis and generation, and prepare for distributed execution.

## Task 1: Real-time Analysis Integration
- **Evolution Monitor**: Connect the `BreederView` visualizers (Spectrum/Goniometer) to the `Telemetry` system so users can see the "Child" DNA forming in real-time.
- **Damping & Smoothing**: Implement a telemetry-rate smoothing filter for the visualizers to ensure 60fps UI fluidity regardless of engine block size.

## Task 2: Distributed Sidecar Protocol
- **Network Transparency**: Implement a TCP-based transport in `ipc-layer` that wraps the `ShmRingBuffer` protocol, allowing "Remote Sidecars" to run on separate hardware.
- **Discovery**: Implement a simple discovery mechanism for the `Conductor` to detect and attach remote DSP nodes.

## Task 3: Batch DNA Extraction & Matchmaking
- **Analysis Worker v2**: Update the `AnalysisWorker` to perform high-speed batch DNA extraction for large folders, utilizing multi-threading.
- **Genetic Matchmaker**: Implement a "Compatibility Index" that ranks potential "Parent" samples from the library based on spectral/rhythmic similarity to the currently playing track.

## Task 4: UI Hardening
- **Modal Sample Selection**: Replace the current simplified selection in `BreederView` with a full-featured modal sample browser linked to the new optimized `CRATES_TABLE`.

## Constraints
- Maintain "Law of Zero Allocation" in the execution plane.
- All network serialization must be RT-safe on the engine side (offload to Orchestration plane where possible).
