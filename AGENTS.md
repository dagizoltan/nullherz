# Nullherz Agent Guidelines: Architectural Invariants & RT-Safety

As an AI agent working on the Nullherz codebase, you must adhere to the following architectural invariants and engineering principles. These rules are designed to maintain the integrity of our high-performance, real-time audio system.

---

## 1. The Triple-Plane Isolation Model

Nullherz is strictly divided into three planes. Never allow logic or resources to leak across these boundaries improperly.

1.  **The Orchestration Plane (`nullherz-conductor`)**:
    *   Handles high-level logic, lifecycle, and declarative state.
    *   Performs expensive operations (e.g., Kahn's algorithm for topology compilation) off the audio thread.
    *   The conductor **tick/command path is latency-critical too** (it feeds the RT command ring): no blocking work — file decode, disk I/O — inline in a command handler. Decode on background threads and re-drive on completion (see async track hydration; an inline decode once froze every queued command, including Play, for seconds).
    *   Communicates with the execution plane via the **Protocol Plane**.

2.  **The Protocol Plane (`ipc-layer`, `nullherz-traits`)**:
    *   Provides lock-free, zero-allocation communication primitives (Ring Buffers, SPSC/MPSC).
    *   Defines the shared command and telemetry schemas.
    *   Must remain ABI-stable where possible.

3.  **The Execution Plane (`audio-core`, `audio-dsp`)**:
    *   The "Hot Path" where audio is processed.
    *   Must remain 100% deterministic and jitter-free.

---

## 2. The Law of Zero Allocation (RT-Safety)

Any code that runs within the `process()` or `process_block()` path of a `SignalProcessor` or the `AudioEngine` **MUST NOT**:
*   **Allocate on the Heap**: No `Vec::new()`, `Box::new()`, `Arc::new()`, or any operation that triggers `malloc`/`free`.
*   **Take Locks**: No `std::sync::Mutex` or `RwLock`. Use lock-free primitives from `ipc-layer` or atomic variables.
*   **Execute Blocking Syscalls**: No File I/O, Network I/O, or thread sleeping.
*   **Throw Panics**: Ensure all math is checked and indices are validated. Use `wide` SIMD types to handle bulk operations safely.

---

## 3. Engineering Hardening Principles

*   **Static Dispatch**: Prefer generics over trait objects (`Box<dyn ...>`) in the execution plane to eliminate vtable overhead.
*   **Denormal Protection**: Ensure FTZ (Flush-to-Zero) and DAZ (Denormals-Are-Zero) flags are respected.
*   **SIMD Alignment**: All audio buffers must be 64-byte aligned (use `AudioBlock` or `AlignedBuffer`).
*   **Sample Accuracy**: Commands must be timestamped relative to `Transport.absolute_samples`.
*   **Separate Address Spaces**: Node indices (`< MAX_NODES = 128`) and audio-buffer/edge indices (`< MAX_BUFFERS = 240`) are distinct address spaces. Both live in `nullherz-traits/src/execution.rs`; quote them from there, never from memory. For scale: the bootstrapped 4-deck console occupies nodes 0–57 and reaches buffer 86. Buffer ids travel as the `BufferId` newtype in graph structures — keep them typed; converting to `usize` early reintroduces the confusion the type exists to kill. Interpret or produce the crossfade sentinel encoding only via `BufferSlot` (`from_raw`/`encode_crossfade`), never with inline `MAX_*` arithmetic. Out-of-range buffer indices are rejected at the conductor and compiler; do not reintroduce clamping.
*   **Targeted Commands**: `PerformanceCommand`s are broadcast to every node. A processor arm MUST match its own address (`node_idx == self.id` / `target_id == self.id`); an untargeted `{ node_idx: _, .. }` match makes one deck's control fire on all of them. `SetParam` needs an explicit bus-delivery arm per processor — without one, parameters are silently dropped.
*   **UI Node Binding**: UI controls resolve node ids by NAME from the telemetry node map, and SKIP the command when unresolved. Never hardcode a node index in a view and never default a failed lookup to 0 — node 0 is deck A's sampler, and that fallback silently corrupted live decks from five different controls.
*   **Logical vs. Graph Node Ids**: `NodeConventions` constants (PREVIEW, the four deck sequencers) are LOGICAL sentinels living at `LOGICAL_BASE = 0xFFFF_FF00` and above — a range no graph will ever reach. The conductor translates them to allocated indices (`node_names`); passing one to the engine as a graph index is silently dropped by the `idx < MAX_NODES` guards. **Test with `NodeConventions::is_logical(idx)`, never against individual constants**, and never hardcode the numbers: they used to be 111 and 70–73, which were out of range only because `MAX_NODES` was 64. At today's `MAX_NODES = 128` all five of those old values are *legal graph indices*, so a stale sentinel is now a command aimed at a real node. `_SENTINELS_ABOVE_MAX_NODES` makes any future `MAX_NODES` increase that reopens that hole a compile error.
*   **Per-Channel DSP State**: Stereo strips run both channels through one processor node. Any processor with per-signal state (filter history, FFT overlap rings, vocoder phase) must keep one state instance per channel (`MultiChannelDspProcessor`, vocoder lanes) — sharing state across channels corrupts both.
*   **Planar Sample Layout**: Decoded sample buffers are planar (channel `c` occupies `buffer[c*frames .. (c+1)*frames]`); `SampleMetadata.total_samples` counts frames *per channel*. Any code slicing sample buffers must map per plane.
*   **Two Block Sizes**: `MAX_BLOCK_SIZE = 1024` sizes engine-internal render buffers and is cheap to grow. `IPC_BLOCK_SIZE = 256` is the frame count inside `AudioBlock`, a **protocol ABI type** pinned at 1088 bytes by a const assert and replicated across every SHM ring. They are deliberately different — do not unify them. A render block larger than `IPC_BLOCK_SIZE` must be split before crossing the IPC boundary; the bridge currently asserts rather than chunking.
*   **Insert Slots, Not Inline Processors**: A deck chain is `Sampler → [pitch slot] → [dna slot] → Gain → Biquad → StereoUtility → [fx slot] → DjIsolator`, where each slot holds `ProcessorTypeId::BYPASS` until engaged and is filled by `TopologyCommand::SwapProcessor` (routing preserved). Adding a processor to the *default* chain costs its latency on every deck on every block forever — an unconditional `KeySync` cost 21.3 ms to serve a latch that defaults OFF. Put it in a slot.
*   **`cfg`-Gated Code Is Untested Code**: Any `#[cfg(target_feature = ...)]` / `#[cfg(target_arch = ...)]` arm that no CI job compiles is not merely unexercised — it is not type-checked. Two arms of `audio-dsp`'s `FloatX16` currently fail to compile for exactly this reason. If you add a platform arm, add the CI target that builds it, or do not add the arm.

---

## 4. Verification Requirements

Before submitting any DSP or Core changes:
1.  **Run the Conformance Suite**: Ensure all processors pass the `Gauntlet` stress-tests (NaN ingestion, buffer oscillation).
2.  **Verify Reset Determinism**: Ensure `reset()` returns the processor to a silent, clean state.
3.  **Check RT-Safety Lints**: If custom lints are available, ensure they pass.

---

*Signed, The Nullherz Architectural Council*
