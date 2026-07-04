# Next Session: Ecosystem Expansion & Genetic Sequencer

**Objective**: Leverage the hardened distributed protocol and matchmaking API to build a multi-node DSP ecosystem and genetic arrangement logic.

## Task 1: Genetic Sequencer Evolution
- **DNA-Aware Patterns**: Implement a "Genetic Sequencer" in `nullherz-conductor` that mutates and evolves MIDI patterns based on the loaded sample's Rhythmic DNA (using the 64-step onset mask and micro-timing profile).
- **Rhythmic Transfusion**: Allow one sequencer track to "inherit" the groove of another track via the Breeder UI.

## Task 2: High-Frequency Resource Sync
- **Binary Mirroring**: Finalize the `ensure_sample_mirrored` pipeline by implementing the receiver side in the `distributed-sidecar` to store and register incoming binary blobs in a local `SampleRegistry`.
- **DNA Binary Protocol**: Implement an efficient binary serialization for `SoundDNA` and `TimestampedCommand` to replace JSON in high-throughput network scenarios.

## Task 3: Distributed Summing & Routing
- **Audio Return Path**: Implement the logic for remote sidecars to stream processed `AudioBlock` data back to the conductor for mixing.
- **Remote Topology**: Extend Kahn's algorithm to account for network nodes, inserting "Network Send/Receive" proxy nodes automatically into the execution graph.

## Task 4: UI Refinement
- **Interactive Topology**: Implement the "Edge Connections" editor in the `TopologyView` to allow users to draw custom routing between nodes.
- **Multi-Node Telemetry**: Add a "Node Health" dashboard to monitor CPU load, latency, and memory pressure for every attached remote sidecar.

## Constraints & Architectural Invariants
- Maintain strict "Triple-Plane Isolation."
- Ensure all network operations are off-loaded from the primary audio thread.
- All SIMD pathways must adhere to 64-byte alignment.
