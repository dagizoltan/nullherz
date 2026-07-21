# Nullherz Technical Optimization & Hardening Log

---

## Profile-Driven DSP Optimization — 2026-07-21 (session 4)

The earlier audit optimized by inspection; a sampler inner-loop hoist that
"looked" hot measured as pure noise (reverted). Lesson applied: **profile
first.** New tool `profile_console_nodes` reads the engine's own per-node cycle
telemetry from the bootstrapped 4-deck console and ranks nodes by cost.

**The profile (before), total per-block node time ~537 µs:**

| type | share | note |
| :--- | ---: | :--- |
| KeySync | **48.7%** | 4 decks × ~65 µs — full STFT every block *even at unison* |
| Limiter | **22.3%** | one node (master), ~119 µs — a quadratic look-ahead scan |
| DjIsolator | 12.9% | 4 decks |
| Sampler | 6.8% | (the thing inspection had targeted) |

Two fixes, both **bit-verified** (golden master render unchanged; per-change
equivalence tests added):

- **Limiter look-ahead: O(window) → O(1) (`limiter.rs`).** The brick-wall
  limiter rescanned the entire ~88-sample look-ahead window for its max on
  every one of 256 samples (~22.5k ops/block). Replaced with a monotonic deque
  (pre-reserved to capacity, no RT allocation) giving the sliding-window max in
  O(1) amortized — the *identical* max value, so output is bit-for-bit the same
  (`test_deque_lookahead_matches_bruteforce_bitexact` pins it against the
  original algorithm). Node cost **119 µs → 12 µs (~10×)**.

- **KeySync unison identity path (`spectral.rs` + `keysync.rs`).** The phase
  vocoder ran a full FFT→IFFT round-trip per frame even at unison (no pitch
  shift — the common case), where the vocoder op is a no-op. But an identity op
  makes `IFFT(FFT(window·frame)) == window·frame` (the 1/n norm cancels the
  transform scaling), so the reconstruction reduces to
  `overlap_add(synth_window · window · frame)`. New `SpectralPipeline::process_identity`
  computes exactly that, skipping BOTH transforms; KeySync calls it when
  `|ratio − 1| < 0.001`. Same framing/latency/buffers as the FFT path, so
  engaging/releasing pitch shift stays continuous; output matches the FFT path
  to within round-trip float error (~−120 dBFS, far below the −60 dBFS golden
  floor — `test_process_identity_matches_fft_roundtrip` pins < −80 dB). Node
  cost **~65 µs → ~4.9 µs (~13×)** per deck at unison; full pitch-shift path
  unchanged.

**Measured (bench_console_block, interleaved A/B, 4 reps, default config):**

| metric | before (merged main) | + limiter + KeySync |
| :--- | ---: | ---: |
| mean | ~600 µs (10.3% budget) | **~191 µs (3.3%)** |
| p99 | ~1.25–1.54 ms | **~0.40 ms** |
| max | up to ~6.5 ms | ~1.8–2.1 ms |

Total per-block node time **537 µs → 202 µs (2.65×)**. The console now sits at
~3.3% of the 256-sample budget; a 128-sample period (2.9 ms) is comfortable on
mean/p99. New hot nodes for a future pass: DjIsolator (36.6% of the smaller
total) and Sampler (22.4%).

---

## Hot-Path Performance Audit — 2026-07-21

**Scope:** full read of the RT execution path (`AudioEngine::process` → `StandardKernel` → `ProcessorGraph::process_parallel` → `GraphExecutor` / `TaskPool` workers → processors), the telemetry finalizer, ipc-layer primitives, and the ALSA backend loop. The architecture is fundamentally sound — static-dispatch kernel, O(1) topology swap, lock-free rings with cache-line padding, FTZ/DAZ, pre-allocated buffers, panic isolation. The findings below are optimization-level, ranked by estimated per-block impact at 44.1 kHz / 256-sample periods (~172 blocks/s, 5.8 ms budget).

### P1 — measurable per-block waste on the RT thread

| # | Finding | Location | Cost & Fix |
| :--- | :--- | :--- | :--- |
| 1 | **16–32 KB of stack zero-init per node dispatch.** `pdc_storage` is `[[0.0; 256]; 16]` (16 KB) in every worker job and `[[0.0; 256]; 32]` (32 KB) in the serial path — memset'd for every node, every sub-block, even though PDC only engages when a `Spectral` node gives a path nonzero latency (all other processors report `latency_samples() == 0`). | `pool.rs:170`, `executor.rs:230` | ~40–60 µs/block wasted on a 40-node console. Gate the array behind the `delay_f > 0` check (`MaybeUninit` or a small per-worker scratch buffer initialized lazily). |
| 2 | **One `eventfd` write syscall per completed job, plus a wake of every worker per stage.** Each worker `notify()`s the completion fd after every job (`pool.rs:264`); `notify_workers()` wakes all N workers per stage even those with zero jobs (they spin 1000 iterations, then sleep); the RT thread blocks immediately in `completion_fd.wait()` with no spin phase. For ~40 nodes across ~8 stages that is >50 syscalls per block on RT-priority threads. | `pool.rs`, `executor.rs:222` | Notify completion only when the worker's queue is empty (batch), skip waking workers with no jobs this stage, and give `wait_for_completion` a short spin phase before the blocking read. |
| 3 | **Telemetry finalizer does fixed heavy work every block regardless of consumers.** Per block: two 4 KB buffer fills + 1024-pt FFT + 512 `sqrt`s for the spectrum, goniometer decimation, and — worst — the `..Telemetry::default()` spread constructs a *second* full multi-KB `Telemetry` (with its own `Default` field-by-field init) just to fill remaining fields; the struct is then copied into the SPSC ring and again into the flight recorder. The UI consumes at most 30 Hz; the spectrum is produced at 172 Hz. | `telemetry_finalizer.rs` | Decimate FFT/spectrum/goniometer to every Nth block (or behind a "UI connected" atomic flag), and initialize the remaining `Telemetry` fields explicitly instead of the `..Default::default()` spread. |
| 4 | **Full-graph peak metering every sub-block.** `update_peak_levels` scans every output buffer of every node (~64 nodes × 2 ch × 256 samples ≈ 128 KB of reads per sub-block, ~22 MB/s of cache traffic that re-touches all pool buffers after the DSP pass). | `telemetry.rs:23` | Meter only nodes the UI can display (named nodes), or decimate to every 4th block; peak-hold semantics survive decimation if the scan covers the skipped span. |

### P2 — structural inefficiencies, engage under specific features

| # | Finding | Location | Cost & Fix |
| :--- | :--- | :--- | :--- |
| 5 | **SamplerVoice planar loop is channel-inner with per-sample slice re-derivation.** For every sample × channel it recomputes `plane * frames` and re-bounds-checks `buffer.get(start..start+frames)`. This is the main real-audio workload (4 decks). | `oscillators.rs:514` | Hoist the per-channel plane slice out of the sample loop (or invert to channel-outer, saving/restoring the playhead); keeps the SIMD interpolator on one contiguous plane per pass. |
| 6 | **PDC delay lines are written/read per-sample with `%` and re-derived 3-D indexing.** `set_sample`/`get_sample_interpolated` each recompute `node*16*4096 + ch*4096` and take up to 3 modulos per sample. Only active with nonzero `input_delays` (spectral paths), but then it's per-sample scalar work. | `buffer_pool.rs`, both executors | Split the ring copy into ≤2 contiguous `copy_from_slice` spans; hoist the base offset; interpolate on the contiguous span with SIMD. |
| 7 | **Repeated `downcast_mut::<TaskPool>` inside the per-node scheduling loop** (assignment cache read, cache write, telemetry pointer — up to 3 dynamic casts per node per stage per sub-block). | `executor.rs:133-194` | Downcast once per `execute_stage` call (or add the cache/telemetry accessors to the `ParallelExecutor` trait). |
| 8 | **`eprintln!` + peak scan on the ALSA audio thread every 500 blocks.** A blocking `write(2)` to stderr on the RT thread; if stderr is a stalled pipe this blocks audio. Also the extra 2×period peak scan. | `alsa.rs:249-257` | Route through `RtLogger` (already exists) or drop; the engine telemetry already carries peaks. |
| 9 | **Crossfade-era buffer capture copies the whole pool.** `capture_old_buffers` copies all 128 AudioBlocks (~139 KB) per block while any crossfade is active, though a crossfade references exactly 2 buffers. | `buffer_pool.rs:74`, `mod.rs:360` | Copy only the `old_buffer_idx`/`new_buffer_idx` pairs named in active `CrossfadeState`s. |

### P3 — micro / hygiene

| # | Finding | Location | Fix |
| :--- | :--- | :--- | :--- |
| 10 | `Job` is ~640 B (three `[usize; 16]` index arrays + `[f32; 16]` delays + copied `Transport`), memcpy'd into a ring per node per sub-block. | `pool.rs:9` | `u16` indices / counts; reference plan-owned delay rows. |
| 11 | In-proc `RingBuffer` push/pop use `% capacity` (runtime div) per op on the job/command rings. | `ipc-layer/lib.rs:572-595` | Require power-of-two capacity, mask like `MpscRingBuffer` already does. |
| 12 | Crossfade SIMD loop rebuilds the 8-lane progress vector with scalar inserts every iteration. | `executor.rs:36-46` | Precompute the lane ramp once; add a splatted `8 * inv_total` step per iteration. |
| 13 | `ZdfSvf` recomputes `a1 = 1/(1+g(g+k))` (a divide) per sample. | `filters.rs:152-219` | Cache `a1/a2/a3` on coefficient change. |
| 14 | Deck-strip `BiquadProcessor` (via `MultiChannelDspProcessor`) runs L then R as two sequential scalar recursions. | `dsp_kernel_processor.rs:67` | Process the stereo pair in 2 SIMD lanes (independent recursions vectorize cleanly), or reuse `SimdBiquad`'s multi-channel path for strips. |
| 15 | `EngineHost::push_command` (RT-side) clones the `Arc`-backed producer per push. | `ipc-layer/lib.rs:226-236` | Give `Producer::push` a `&self` path (interior indices are atomics already). |

### Measurement follow-up
~~`nullherz-bench` (criterion) has no benchmark for a full `process_block` of the bootstrapped 4-deck console.~~ **DONE (2026-07-21):** `cargo run --release -p nullherz-conductor --example bench_console_block` drives the bootstrapped console (4 decks playing multitone stereo, worker pool active, no backend thread) for 20 000 timed blocks and reports mean/percentiles against the 5 805 µs budget.

### Implemented 2026-07-21 — items 1, 2, 7

- **#1 PDC scratch (DONE):** the worker pool now owns a per-thread scratch `Box` (allocated at spawn) and the serial path uses a scratch owned by `PdcLines` (allocated at graph construction) — the 16–32 KB stack zero-init per node dispatch is gone. In passing this unified an inconsistency: the pooled path truncated fractional delays with `as usize` in its input-swap check, so a purely sub-sample PDC delay (0 < d < 1) was applied by the serial executor but silently dropped by the workers.
- **#2 pool syscall batching (DONE):** workers drain their whole queue and signal the completion eventfd once per batch instead of once per job; the stage scheduler wakes only the workers that actually received jobs (`notify_workers_masked`, built from job placement); `wait_for_completion` spins ~2000 iterations before parking on the eventfd.
- **#7 downcast hoist (DONE, co-located):** the `TaskPool` downcast is resolved once per stage instead of up to three times per node; job assembly is shared between the TaskPool fast path and the generic `ParallelExecutor` fallback. The stale Kani harness call in `verification.rs` (predating the PDC/faulted-state parameters) was repaired.

**Measured** (bench_console_block, 20 000 blocks, same machine, same run conditions; output peak identical at 0.5350, full suite 211/211 green, golden master render unchanged):

| Metric | Before | After | Δ |
| :--- | ---: | ---: | ---: |
| mean | 1 232 µs (21.2 % budget) | 995 µs (17.1 %) | **−19 %** |
| p50 | 1 136 µs | 931 µs | −18 % |
| p90 | 1 519 µs | 1 247 µs | −18 % |
| p99 | 2 840 µs | 2 326 µs | −18 % |
| p99.9 | 4 859 µs | 3 621 µs | **−25 %** |
| max | 10 287 µs | 5 092 µs | **−50 %** |

The tail improvement is the one that matters for xruns: worst-case block time halved.

### Implemented 2026-07-21 (session 2) — items 3, 4, and the worker-count experiment

- **#3 telemetry finalizer (DONE):** the 1024-pt FFT + spectrum fold-down + goniometer + DNA-latent computation now runs decimated (every 4th block, ~43 Hz, still 1.4× the 30 Hz UI cadence) via a pre-allocated `SpectralTelemetryCache`; off-phase blocks republish the cached analysis. The `..Telemetry::default()` spread — which built and dropped a second multi-KB `Telemetry` every block to fill seven trailing fields — is replaced with explicit field init.
- **#4 peak metering (DONE):** `update_peak_levels` (a full-graph output-buffer scan, ~128 KB of reads) now runs every 4th physical block, latched at block start (`meter_this_block`) so all sub-blocks of a metered block agree and the offset==0 reset + accumulate keeps peak-hold intact. Meter cadence ~43 Hz; the UI samples ≤ 30 Hz and takes a running max, so no displayed peak is lost.
- **Measured:** on this 2C/4T laptop the serial-path mean was ~490 µs both before and after #3/#4 — the change is real (removes provably-wasted work, 211/211 green, golden render unchanged) but sits below run-to-run noise here, because the 4-deck console DSP dominates the serial path and telemetry was a small slice of it. Retain the fixes (they scale with block rate and matter more at smaller periods / on faster DSP), but they are not the lever on this hardware.

### The worker-count experiment (audit item #2 follow-up) — DECISIVE

Added `NULLHERZ_WORKERS` (0 = no pool, pure serial execution on the RT thread; else worker count) so the pool can be A/B'd. `bench_console_block`, 6 000 blocks/run, interleaved with 15–20 s cooldowns to control laptop thermal drift, `powersave` governor, no RT privilege:

| workers | mean | p99 | p99.9 | max |
| ---: | ---: | ---: | ---: | ---: |
| **0 (serial)** | **~490 µs (8.4 %)** | ~1.10 ms | ~1.20 ms | **~1.2 ms** |
| 4 (default pool) | ~1 060 µs (18 %) | ~2.4 ms | ~3.8 ms | ~5–6 ms |

**Serial is ~2× faster on mean and ~4× tighter on tail than the 4-worker pool on this machine.** Cause: per-node DSP costs are in the microseconds, so `push_job → eventfd → worker wake → completion eventfd` dispatch (onto only 2 physical cores, hyperthread-oversubscribed, no SCHED_FIFO) costs more than the parallelism returns. The pool is a net loss whenever (dispatch overhead) > (serial cost of the stage's nodes) — true for this console on ≤ 2 physical cores, and true for ANY small stage even on big machines.

**Consequence:** the hardcoded `DEFAULT_WORKER_COUNT = 4` was wrong in both directions — it lost to serial on small machines and did not scale up on large ones.

### Implemented 2026-07-21 (session 3) — adaptive worker policy (cost gate + core-aware default)

Chosen direction: **adaptive cost-gate**, both halves.

- **Per-stage cost gate (`executor.rs`):** a stage dispatches to the pool only if it has ≥ 2 nodes AND its telemetry-measured node-time sum (`telemetry_node_times_cycles`) meets `TaskPool::parallel_threshold_cycles` (default `DEFAULT_PARALLEL_THRESHOLD_CYCLES = 150_000`, ≈ the reference box's measured per-stage dispatch overhead; override via `NULLHERZ_PARALLEL_THRESHOLD_CYCLES`). Otherwise the stage runs inline on the RT thread. Cold start reads zero telemetry → serial (safe default); a stage escalates to the pool only once its measured cost proves it worthwhile, so the policy is self-calibrating and can never regress a cheap stage. Single-node stages are always serial (nothing to parallelize).
- **Core-aware default (`engine/mod.rs`):** `default_worker_count()` = `available_parallelism() - 1` (leaves the RT thread a core), replacing the hardcoded 4. This is now an *upper bound* on parallelism — spare workers just park on their eventfd — because the gate decides actual dispatch. Resolution order unchanged: resource config → `NULLHERZ_WORKERS` → this default.
- Pool-path coverage preserved with a new test (`test_pool_multinode_stage_dispatch`): two independent nodes in one stage, gate forced open (threshold 0), both dispatched and asserted correct. Suite 212/212.

**Measured — the DEFAULT config (no env vars), bench_console_block, 6 000 blocks:**

| Config | mean | p99 | p99.9 | max |
| :--- | ---: | ---: | ---: | ---: |
| Old default (hardcoded 4, always pool) | ~1 060 µs (18 %) | ~2.4 ms | ~3.8 ms | ~5–6 ms |
| **New default (auto workers + cost gate)** | **~515 µs (8.9 %)** | ~1.24 ms | ~1.39 ms | **~1.6 ms** |
| (reference: hand-forced serial) | ~490 µs (8.4 %) | ~1.10 ms | ~1.20 ms | ~1.2 ms |

The adaptive default reaches serial-level performance **automatically** on this hardware (~2× mean, ~3× tail vs the old default), and on many-core / RT-privileged hardware the same binary will dispatch genuinely heavy stages to the pool without configuration. Headroom is now large enough that a 128-sample period (2.9 ms budget) is realistic on mean/p99 here — the next acceptance test to run on target hardware.

---

## Previous log — June 22, 2026

**Focus:** Performance Maximization & RT-Hardenings

---

## 1. Real-Time (RT) Hardenings
*Ensuring the audio thread is 100% deterministic and jitter-free.*

| Task | Priority | Description |
| :--- | :---: | :--- |
| **Kernel Devirtualization** | VERIFIED | Replaced `Box<dyn ProcessingKernel>` with static dispatch (`AudioEngine<K: ProcessingKernel>`). Verified via `audio-core`. |
| **Pre-allocated Commands** | VERIFIED | Refactored `MixerBridge` to use a pool of pre-allocated `Vec<Command>` via `ResourceRecycler`. Verified via `nullherz-conductor`. |
| **Stack-Pinning** | High | Investigate stack-pinning for critical DSP nodes to improve cache locality and prevent accidental movement during block cycles. |
| **Denormal Safeguard v2**| Low | Add explicit `is_subnormal` checks in the `Gain` kernel as a secondary defense to FTZ/DAZ hardware flags. |

---

## 2. DSP & SIMD Optimizations
*Maximizing throughput and reducing per-sample CPU cycles.*

| Task | Priority | Description |
| :--- | :---: | :--- |
| **AVX-512 Pathway** | VERIFIED | Implemented 16-wide `FloatX16` path in `simd_vec.rs` and core kernels. Moved state to stack locals for register optimization. |
| **Zero-Copy Sampler** | VERIFIED | Refactored `SamplerVoice` Lagrange interpolation for direct SIMD pointer loads via `f32x4`. |
| **Biquad Unrolling** | Medium | Implement 4x unrolled biquad kernels for the `DjIsolator` to reduce the overhead of crossover filtering. |
| **FFT Twiddle Caching** | Low | Implement a global twiddle-factor cache in `audio-dsp` to avoid re-calculation during dynamic FFT size changes. |

---

## 3. Infrastructure & Throughput
*Reducing IPC overhead and orchestration latency.*

| Task | Priority | Description |
| :--- | :---: | :--- |
| **Ring Buffer Affinity** | VERIFIED | Implemented thread pinning via `sched_setaffinity` in `ipc-layer`. RT threads are locked to dedicated cores to maximize L3 cache locality. |
| **Telemetry Batching** | Medium | Batch `NodeMetrics` telemetry to reduce the frequency of atomic writes to the shared-memory status segment. |
| **Zero-Copy Serialization**| Medium | Move `ProjectState` toward a binary format (e.g., `rkyv`) to achieve near-instant project save/load. |

---

## 4. UI Rendering & Metrics
*Ensuring the Inspector remains fluid even under heavy DSP load.*

| Task | Priority | Description |
| :--- | :--- | :--- |
| **Widget Caching** | VERIFIED | Implemented `egui::Shape` caching for industrial widgets in `nullherz-ui-hal`. Static geometry is persisted in temporary memory. |
| **Waveform Downsampling**| Medium | Implement a multi-level waveform cache (MIP-maps) to reduce the number of points rendered in the rolling monitor. |
| **Telemetry Throttle** | Low | Decouple UI refresh rate (60Hz) from telemetry ingestion rate (RT-frequency) using a damping filter. |

---

## 5. Summary of Immediate Action Items
1.  **SIMD Pointer Access:** Refactor sampler kernels for direct aligned memory access.
2.  **UI Geometry Caching:** Optimize industrial-steel widget rendering.

---

**Audit Conducted by:** *Nullherz Systems Architecture Team*
