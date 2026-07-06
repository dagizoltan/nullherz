# Nullherz Engineering Hardening Manifesto

**Purpose:** To lock down architectural invariants before scaling the "AnaWaves" ecosystem.

---

## 1. The Three Laws of Nullherz
Every line of code in the execution plane must obey these laws. No exceptions.

### 1.1 The Law of Zero Allocation
The audio thread (Real-Time path) shall never perform a heap allocation. This includes `Vec::push`, `Box::new`, `Arc::new`, or any implicit allocation within 3rd-party crates.

#### 1.1.1 Zero-Allocation Guest-Host Communication
WASM host functions and Sidecar IPC bridges must utilize stack-allocated buffers and non-allocating serialization (e.g., `bincode::serialize_into` with a pre-allocated `[u8]` slice).
*   **Hardening Goal:** Eliminate `bincode::serialize` (which returns a `Vec<u8>`) from all `fx-runtime` host functions.
*   **Hardening Goal:** Implement a build-time or runtime lint to detect unexpected allocations in the `process()` path.

### 1.2 The Law of Bit-Exact Reset
Calling `processor.reset()` must return the processor to a state identical to its initialization. No DC offsets, no lingering filter history, no cached samples.
*   **Hardening Goal:** Every processor in the `ProcessorRegistry` must pass the `ConformanceSuite::verify_silence_after_reset` check.

### 1.3 The Law of SIMD-First Design
Scalar DSP is a fallback, not the standard. All new kernels must be designed with 64-byte alignment and SIMD throughput (AVX-512/NEON) as the primary execution path.
*   **Hardening Goal:** 100% coverage of `verify_simd_alignment` across the `audio-dsp` library.

---

## 2. AnaWaves Compliance
To be a valid part of the Nullherz ecosystem, a DSP node must support the **5 Layers of Transfusion**.

1.  **Granular Ready:** Can the node be decomposed into grains without losing its character?
2.  **Spectral Export:** Does the node provide a frequency-domain "Personality" that can be inherited?
3.  **Cyclic Capture:** Does the node support `pull_snapshot()` for evolutionary re-injection?
4.  **Eco-Modulation:** Does the node publish all internal parameters to the Modulation Matrix?
5.  **Artifact Tolerance:** Does the node handle non-finite floats gracefully via the "Safe Mode" fallback?

---

## 3. The "Lock-In" Workflow
Before a feature moves from "PLANNED" to "IN PROGRESS":

1.  **RFC Submission:** A technical spec detailing alignment, SIMD path, and AnaWaves integration.
2.  **Implementation:** Build out the kernel in `audio-dsp` or `nullherz-processors`.
3.  **Conformance Test:** Run the `ConformanceSuite`.
4.  **Audit:** Final architect review for RT-safety violations.

---

**Signed:** *Lead Nullherz Architect*
