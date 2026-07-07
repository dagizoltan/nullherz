# Nullherz Verification & QA Strategy

**Focus:** Mathematical Certainty and Real-Time Robustness.

---

## 1. Formal Verification (The Protocol Shield)
Because Nullherz relies on lock-free shared memory (`ipc-layer`), standard unit tests are insufficient to catch rare race conditions.
*   **Kani Proofs:** We will implement **Kani formal verification** for all SPSC and MPSC ring buffers. This mathematically proves the absence of memory safety violations and logic errors in our lock-free primitives.
*   **Target:** 100% proof coverage of the `ipc-layer` core logic.

---

## 2. Real-Time Fuzzing (The Kernel Stress-Test)
DSP kernels often fail at edge cases (e.g., extremely high resonance, non-finite inputs, or buffer size changes).
*   **Cargo-Fuzz:** Every processor RFC must include a fuzzing target. We will use `cargo-fuzz` to feed random, high-entropy signal data and parameter updates into our kernels.
*   **Hardening Goal:** Ensure that no sequence of inputs can cause a kernel to produce `NaN` or `Inf` values (Layer 5: Error Rehabilitation).

---

## 3. Performance Budgeting (The Jitter Guard)
The "Triple-Plane Model" only works if the Execution Plane remains within its cycle budget.
*   **CI-Benchmarking:** We will integrate `nullherz-bench` into our CI pipeline.
*   **Regression Blocking:** Any PR that increases the median execution time of the `StandardKernel` by more than 2% or increases jitter (max latency) will be automatically blocked.
*   **Flamegraph Audit:** Automated generation of CPU flamegraphs for every release build to identify cache-locality bottlenecks.

---

## 4. RT-Safety Lints (Enforcing the Three Laws)
Manual audits for heap allocations in `process()` are error-prone.
*   **Zero-Alloc Lint:** Investigate and implement `no-panic` and custom `clippy` rules to statically ensure the audio thread never calls `malloc` or `free`.
*   **Subnormal Protection:** Ensure the `FTZ/DAZ` flags are verified at the start of every block cycle via an automated conformance check.

---

## 5. UI-Logic Decoupling QA
*   **Playwright Integration:** For the `nullherz-inspector`, we will use Playwright to verify that UI interactions (e.g., moving a fader) result in the correct bit-pattern being sent to the command bus.

---

## 6. Current Coverage Status (Audit: 2026-07-07)

| Verification Method | Status | Details |
|---------------------|--------|---------|
| **Kani Proofs** | [PARTIAL] | 3 proofs implemented in `ipc-layer` (SPSC, MPSC, ShmSignal). |
| **Cargo-Fuzz** | [PARTIAL] | Baseline template created for `GainProcessor`. Processor-wide coverage pending. |
| **CI Workflow** | [PARTIAL] | Minimal workspace testing (`cargo test`) implemented. Benchmarking not yet integrated. |
| **Zero-Alloc Lint** | [PLANNED] | Statically enforced via manual audit only. |
| **Playwright** | [PLANNED] | Not yet implemented. |

**QA Invariant:** *It is better to have one mathematically verified kernel than ten un-hardened features.*
