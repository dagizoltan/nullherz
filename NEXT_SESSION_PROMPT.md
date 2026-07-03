# Next Session: Evolutionary Feedback & Distributed DSP

**Objective**: Close the feedback loop between analysis and generation, and prepare for distributed execution.

## Task 1: Real-time Evolution Analysis (The Feedback Loop)
- **Evolution Monitor**: Connect the `BreederView` visualizers (Spectrum/Goniometer) to the `Telemetry` system so users can see the "Child" DNA forming in real-time.
- **Damping & Smoothing**: Implement a telemetry-rate smoothing filter (EMA) for the visualizers to ensure 60fps UI fluidity regardless of engine block size.
- **DNA Extraction**: Ensure the "Child" DNA is analyzed at the engine level (via `TelemetryFinalizer`) and reported back to the UI.

## Task 2: Distributed Sidecar Protocol (Network DSP)
- **Network Transparency**: Implement the `RemoteSidecarManager` logic in `SidecarSupervisor` to handle network-transparent command broadcasting.
- **TCP Bridge**: Use the `TcpIpcConsumer` from `ipc-layer` to allow remote sidecars to run on separate hardware, offloading heavy spectral morphing.
- **Discovery**: Finalize the UDP discovery mechanism for the `Conductor` to detect and attach remote DSP nodes automatically.

## Task 3: Genetic Matchmaker & Batch Analysis
- **Analysis Worker v2**: Update the `AnalysisWorker` to perform high-speed batch DNA extraction for large folders using `rayon` for multi-threading.
- **Compatibility Index**: Implement the "Compatibility Index" logic in `Matchmaker` and display suggestions in the `BreederView` based on genetic similarity.

## Task 4: Universal MIDI Mapping & Clock Sync
- **MIDI Mapping**: Finalize the JSON-based declarative MIDI mapping engine in `MidiMapper`.
- **Clock Synchronization**: Implement MIDI Clock slave functionality in the `Conductor` to synchronize the internal `Transport` with external hardware.

## Constraints & Architectural Invariants
- **Triple-Plane Isolation**: Maintain strict separation between Orchestration, Protocol, and Execution.
- **Law of Zero Allocation**: No heap allocations, locks, or blocking syscalls in the `audio-core` execution path.
- **SIMD Alignment**: All audio buffers and DNA energy maps must remain 64-byte aligned for AVX-512 optimization.
- **Sample Accuracy**: Ensure all commands are timestamped relative to `Transport.absolute_samples`.
