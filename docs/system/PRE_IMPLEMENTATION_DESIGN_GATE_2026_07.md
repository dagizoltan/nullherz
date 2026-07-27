# Pre-Implementation Design Gate — July 2026

**Status:** Decision record. Produced at the design gate before the DNA / tap-bus implementation.
**Scope:** Storage & library, Sound DNA schema, tap-based modulation ("neuron") system, RT/platform constraints, and the defects that must be closed first.

---

## 0. How to read this

Every claim carries provenance. Challenge any tag that isn't backed.

| Tag | Meaning |
| :-- | :-- |
| **[M]** | Measured on the reference machine during this review (release build) |
| **[V]** | Verified by reading the code in this repository |
| **[D]** | Design decision taken in this review |
| **[O]** | Open — requires a human decision before implementation |

**Reference machine** (the FLOOR tier, see §7): Lenovo ThinkPad X270 — Intel i5-7300U, **2 cores / 4 threads**, 15 W, AVX2+FMA, no AVX-512, L2 512 KiB, L3 3 MiB; 14 GiB RAM; **SATA** SSD (Toshiba THNSFJ256GDNU, 238 GB — *not* NVMe); 4 GB swap enabled; CPU governor `powersave`. [M]

---

## 1. Landed during this review

Fixes already applied and verified. Recorded here so the numbers below are reproducible.

| Change | Before | After |
| :-- | :-- | :-- |
| `TelemetryService::update_timeline` — removed a per-audio-block full library row read (`get_track` per deck) that produced a telemetry field with **no readers anywhere in the repo** | **268 ms/frame** (2 MP3 decks) = 4617% of block budget | **164 µs** (2.8%) [M] |
| `sync_sampler_metadata` — snapshot node/sample pairs, release the `engine_handle` lock before resolving metadata; registry-first, library fallback; push only on `Arc` change, record only when the ring push lands | worst `tick()` **130 ms** (2249% of budget) | **136 µs** (2.3%) [M] |
| `GeneticLibrary::get_track_facets` — waveform-free read via the existing facet index; used by deck-load translation and matchmaking | `Load+Play` **318 ms** | **57–119 ms** [M] |
| Net effect on the reported symptom | first audio **11.6 s** | **1.1 s** [M] |
| Added `tests/long_track_control_path_test.rs` | — | fails on the old code with the correct diagnostics, passes on the new [M] |

**Root cause, for the record:** `LibraryTrack` embeds the full waveform (peaks + 8-level MIP pyramid + 5-series band waveform) and is persisted as **JSON**. `get_track` is therefore linear in track length — **61 ms** for a 6-minute track vs **2.5 ms** for a 17-second one; 1.6 M floats parsed from text. `list_tracks` over 12 rows costs **154 ms**. [M]

---

## 2. Measured baselines

All on the reference machine, release build.

### 2.1 Throughput

| | Rate |
| :-- | :-- |
| Decode (MP3 → planar f32) | **25.6 M frames/s** ≈ 580× realtime [M] |
| Full analysis (transients, peaks, band waveform, key, DNA) | **17.0 M frames/s** ≈ 385× realtime [M] |
| 1024-point FFT (derived from the analysis pass) | **~15–25 µs** [M] |

### 2.2 4-deck console block cost

Engine driven directly — **no backend, no driver, no worker-pool contention.**

| Block | Budget | mean | mean % | p99 | p99.9 | **max** | max % |
| :-- | :-- | :-- | :-- | :-- | :-- | :-- | :-- |
| 256 | 5805 µs | 117.4 µs | **2.0%** | 258 µs | 321 µs | 462 µs | 8.0% |
| 128 | 2902 µs | 67.8 µs | **2.3%** | 166 µs | 263 µs | 418 µs | 14.4% |
| 64 | 1451 µs | 40.2 µs | **2.8%** | 101 µs | 171 µs | **732 µs** | **50.4%** |

Cost model fitted to those points: **~18 µs fixed per block + ~0.39 µs per frame** — i.e. **1.7% of one core per second of audio** for the full 4-deck console. [M]

**The tail does not scale with block size** (462 → 418 → 732 µs) — but see the correction below before drawing any RT conclusion from it.

> **CORRECTION (measured during Phase 1).** The max column above says nothing
> about realtime behaviour. `bench_console_block` drives the engine on a **plain
> thread with default scheduling** — it never calls `setup_rt_thread`, so it has
> no RT priority and is freely preempted by whatever else is on the desktop. The
> real audio path *does* take RT priority (`threaded.rs:74`
> `setup_rt_thread(90, Some(0))`; ALSA likewise).
>
> Measured with `/usr/bin/time -v` across repeated runs: **major page faults were
> 0 every time**, and the max tracks **involuntary context switches** —
> 944 → 1415 µs, 629 → 898 µs, 126 → 440 µs. It is desktop preemption of a
> non-RT thread, not swapping and not DSP.
>
> The **mean and percentiles remain valid** (that is what this harness measures
> well, and they are stable at ~120–128 µs). The tail must be re-measured on the
> actual RT path via `bin/survival` before any RT claim rests on it.

### 2.3 Memory

| | Size |
| :-- | :-- |
| 6-min stereo track as planar f32 | **126.8 MB** [M] |
| Library row *metadata* as JSON, same track | **6.4 MB** (larger than the 5.5 MB source file) [M] |
| `Telemetry` struct | 6,456 B [M] |
| `GraphTopology` | 42,768 B [M] |
| `CompiledGraphPlan` | 21,064 B [M] — scales as **4N²** in `MAX_NODES` |
| `SoundDNA` | **2,416 bits = 302 B**, of which **768 bits (31.8%) are never written** by the analyser [V] |
| Registry residency, 500 tracks × 5 min | **51.7 GB** — `SampleRegistry` has no removal method [V] |

---

## 3. Locked design decisions

### 3.1 Storage and library

| # | Decision |
| :-- | :-- |
| 3.1.1 | **No custom codec.** FLAC/WavPack are past sufficient; beating them is a research project. [D] |
| 3.1.2 | **Custom container: yes — but justified by P2P, not by speed.** Verified partial fetch from an untrusted peer is impossible with a plain file at any effort level. Local performance wins are available far sooner and more cheaply. [D] |
| 3.1.3 | **Archive bit-exact at native rate and depth.** Never resample the master. Normalisation happens only into the session cache. [D] |
| 3.1.4 | **Canonical session rate: 48 kHz, f32.** Evidence-backed — see §6.1. [D] |
| 3.1.5 | **Ceiling, not target: never upsample.** 44.1 / 48 / 88.2 / 96 keep native; 176.4 → 88.2; 192 → 96. Every conversion stays a clean 2:1; the 44.1-family catalogue is never touched. (Crossing the 44.1/48 families is a 320/147 ratio — expensive and quality-sensitive.) [D] |
| 3.1.6 | **Chunk codec: WavPack for float, FLAC for integer** (or a per-chunk codec flag). **FLAC cannot store 32-bit float at all.** [D] |
| 3.1.7 | **Fixed-length chunks, 65,536 frames.** Independently decodable and independently verifiable — the same granularity serves O(1) seek, range-parallel decode, and per-chunk Merkle verification. |
| 3.1.8 | **Hash before resampling**, over canonical PCM at native rate — so the same master as WAV and as FLAC dedups correctly. [D] |
| 3.1.9 | **Session cache is mmap-able f32**, keyed by content hash. The kernel becomes the evictor; no LRU of our own. [D] |
| 3.1.10 | **Measure true bandwidth on import.** Detect the lossy spectral cliff (128 kbps MP3 dies ~16 kHz, 320 kbps ~20 kHz). A quality tier derived from *measurement* rather than *declaration* catches transcodes claiming to be lossless. Store it in SM DNA. [D] |
| 3.1.11 | **"Enhance quality" is declared restoration, never automatic upgrade.** Upsampling recovers no information. Restoration (declip, denoise, band extension) stays opt-in, non-destructive, and flagged. [D] |

### 3.2 Sound DNA — four layers

```
L0  IDENTITY     32 B   BLAKE3(canonical PCM)          index key
L1  SM           64 B   search / grouping / gossip      replicated globally
L2  MD           64 B   16 named descriptors            replicated + LIVE per block
L3  LG          ~4 KB   transfer / resynthesis          ships with the audio
```

Replicated per track: **160 B**, against today's 302 B — *less* size, *more* usable information (96 dead bytes removed, everything quantized, timbre decorrelated). At 1 M tracks: **160 MB**, laptop-resident.

**Load-bearing property:** L2's static and live forms share one layout, so a patch driven by stored DNA and the same patch driven by live audio are the same patch. No special cases. [D]

**L1 — SM, 64 B**

| Field | Encoding | B |
| :-- | :-- | :-- |
| Timbre: DCT of mel spectrum (MFCC-style, decorrelated) | 24 × int8 | 24 |
| Dynamics: crest, transient density, attack, LRA | 4 × int8 | 4 |
| Rhythm: log-BPM, tempo confidence, syncopation, onset density | 4 × int16 | 8 |
| Tonal: key, key confidence, harmonicity, chroma spread | 4 × int16 | 8 |
| Spatial: width, L/R correlation | 2 × int8 | 2 |
| Quality (measured): bandwidth cliff, LUFS, true-peak, clip count | 4 × int16 | 8 |
| `schema_version`, flags, duration, sample rate | — | 10 |

24 timbre dims, not 128: mel bands are ~0.9 correlated with neighbours, so a DCT collapses them with near-zero loss. **Past ~200 dimensions, distance concentration makes cosine similarity *worse*, not better.** Uniqueness needs ~47 bits for 10 M tracks — already satisfied many times over; it is not a reason to grow the record. [D]

**L2 — MD, 64 B — the 16 named descriptors** [O — see §8]

| # | | # | | # | | # | |
| :-- | :-- | :-- | :-- | :-- | :-- | :-- | :-- |
| 1 | level | 5 | sub <60 | 9 | air >8k | 13 | noisiness |
| 2 | peak | 6 | low 60–250 | 10 | brightness | 14 | pitch |
| 3 | crest | 7 | mid 250–2k | 11 | spread | 15 | width |
| 4 | transient | 8 | high 2k–8k | 12 | flux | 16 | density |

f32 in memory (**exactly one cache line**), int16 on the wire. **All normalized to [-1, 1] at the source** with documented mappings — this is what makes a future ML model portable. 1–9 and 15 need no FFT; 10–14 share **one FFT per tap**.

**L3 — LG, ~4 KB, on demand**

The important choice is a **timbre codebook: 16 characteristic spectra × 128 mel bands (int8 dB)** plus weights and dwell times — ~2 KB of the 4. A single averaged spectrum flattens a sound's whole life into one curve and cannot represent "this bass has a filter sweep." Sixteen timbral states is what makes transfer musical. Remainder: formants, per-band-group envelope statistics, micro-timing, onset probabilities, per-band spatial, per-band artifact profile.

Grow this layer freely — it is not replicated.

**Aggregates (album / artist / genre)**

Aggregation is a **schema constraint**, and several fields do not aggregate by averaging:

| Field | Operator |
| :-- | :-- |
| Timbre dims, LUFS, bandwidth, width | mean **and variance** — variance is the payload (consistent vs eclectic artist) |
| BPM | **histogram.** A 174 BPM DnB artist with 87 BPM halftime tracks averages to 130, describing nothing they have made |
| Key | **circular** mean or 12-bin histogram. The linear mean of B and C is F♯ |
| Onset mask | per-step **probability** — the aggregate is a *different type* than the member |
| Formants | unordered set; peaks must be matched before averaging |

Aggregate record ≈ mean(64) + variance(64) + BPM histogram(16) + key histogram(12) = **160 B**. Genre is a *label*, not a measurement: model it as **centroid + covariance**, which yields genre-fit scoring (Mahalanobis distance) and emergent-genre clustering for free. [D]

**Mel bands cap at 16–20 kHz regardless of source rate**, or a 192 kHz file's DNA is not comparable to a 44.1 kHz file's and similarity search breaks across the library. [D]

**Breeding is retained, repositioned.** `transfuse_dna` / `chaotic_transfuse_dna` stay. Breeding produces a *target* genome; live cross-adaptive modulation is how you reach it in the mix. Breeding against an **artist or genre centroid** ("push this toward that sound") is a request a producer actually makes; breeding two arbitrary tracks is not. [D]

### 3.3 Tap / bus / neuron system

| # | Decision |
| :-- | :-- |
| 3.3.1 | **A tap is a subscription on an existing `BufferId`, not an insertion.** Every edge in the graph is already a numbered buffer (~87 exist in the 4-deck console [V]). No graph mutation, no recompile, no added latency, no node consumed. "Between volume and EQ", "between FX items" are already addressable. [D] |
| 3.3.2 | **Tap input spec is `(BufferId, descriptor, bin_range)`.** Once a tap's FFT exists, computing a descriptor over an arbitrary bin range is nearly free — **the FFT is expensive, slicing it is free.** "Channel 1's kick transients duck only channel 2's low-mids" becomes a two-field patch. This is the most genuinely novel primitive in the design. [D] |
| 3.3.3 | **Internal taps via an additive defaulted trait method** — `fn internal_taps(&self, _out: &mut [f32]) -> usize { 0 }`. Every existing processor compiles unchanged; only processors that want internal probes override it. Not invasive. [D] |
| 3.3.4 | **Subscription-driven, mandatory.** An unsubscribed tap costs zero. A tap whose consumers want only `level` runs no FFT. One FFT per tap, shared by every spectral descriptor. [D] |
| 3.3.5 | **Double-buffered control bus** — `[[f32; MOD_SLOTS]; 2]`, every node reads N-1 and writes N. This one decision makes cycles legal by construction, evaluation order irrelevant (no topological sort, no recompile on patch), evaluation trivially parallel, and results deterministic regardless of thread scheduling. Cost: one block of control latency (5.8 ms at 256/44.1k) — inaudible for modulation. [D] |
| 3.3.6 | **`tanh` as the default activation.** Bounded to [-1,1], so a feedback loop *cannot diverge* — worst case is saturation, which is musically a plateau. This is what makes circular modulation safe by construction. [D] |
| 3.3.7 | **NaN/Inf sanitiser on every bus write.** One NaN in a cycle propagates forever and never clears — it would poison a patch permanently, including after the causing audio has stopped. [D] |
| 3.3.8 | **Per-node slew, default a few ms.** Block-rate feedback can oscillate at ~86 Hz and will be audible on whatever it drives. [D] |
| 3.3.9 | **Store the patch as a sparse weight matrix over bus slots, not an object graph.** `bus_next = activation(W · bus_prev + b)` with a double-buffered bus **is an RNN timestep**. A hand patch is a sparse `W`; a trained model is a denser `W`; a hybrid is `W_patch + W_learned`. Same data structure, same evaluation code, same RT cost profile — training later requires no architectural change. **This is the ML-readiness decision.** [D] |
| 3.3.10 | **Static DNA values are constant sources on the same bus.** "A mesh of multiple DNA" is a neuron whose inputs happen not to change. Uniform. [D] |
| 3.3.11 | **Two graphs, two rule sets.** Audio graph = strict DAG, compiled (Kahn). Mod graph = separate structure, cycles legal, no compilation. They overlap on nodes and share no rules — the same class of distinction `BufferId` already exists to enforce. [D] |
| 3.3.12 | **Audio-rate cross-channel routing is a separate, explicitly-delayed feature.** Control-rate cycles are free; audio-rate cycles need an explicit one-block feedback buffer, like a hardware aux send. [D] |
| 3.3.13 | **Build the bus recorder now**, even with no consumer. 512 slots × 4 B × 172 Hz = **352 KB/s**. Without it, every session before it exists is training data that cannot be recovered. [D] |
| 3.3.14 | **`MOD_SLOTS = 512`, statically assigned** — a fixed `W` shape is what the ML story requires. |

**Modulation lives in `audio-core`, on the audio thread.** Today `ModulationMatrix` is macro→parameter only, expands to `SetParam` commands, and runs on the conductor thread — a path measured stalling **130 ms** [M] and inherently ~60 Hz. The conductor's role shrinks to *editing the routing table*; it never carries a value. **This is the least reversible decision in the document** — a control bus that ever travels as commands will feel rubbery forever.

### 3.4 Budget check

| | 64 frames (1451 µs) | 128 frames (2902 µs) |
| :-- | :-- | :-- |
| Console mean [M] | 40 µs | 68 µs |
| 16 spectral taps | ~320 µs | ~320 µs |
| Mod graph, 128 active slots | ~8 µs | ~8 µs |
| Mod graph, 512 dense (memory-bound: 1 MB `W` vs 512 KiB L2) | ~110 µs | ~110 µs |
| **Total, 16 taps + sparse** | **368 µs — 25%** | **396 µs — 14%** |
| Console **max** [M] + taps + mod | 1100 µs — **76%** | 790 µs — **27%** |

**Taps cost ~12× more than the neural network does.** Design around tap cost, not inference cost. The full system is comfortable at 128 frames and tight at 64 — and tight because of the existing tail (§5.2), not the DSP.

---

## 4. Irreversible decisions — settle before writing implementation code

Each shares one property: **cheap now, migration-or-surgery later.** Four of the six are the same failure mode — *a value whose meaning depends on context that is not stored alongside it.*

### 4.1 `SampleMetadata` has no `sample_rate` field [V]

Every position field is denominated in frames with no record of the rate: `hot_cues`, `loop_points`, `beat_grid_offset`, `transients`, `total_samples`.

With multi-rate support (§3.1.4–3.1.5), a track analysed at 44.1 kHz loaded into a 48 kHz session has **every cue point 8.2% early** — a cue at 30.00 s lands at 27.56 s — and the beat grid drifts identically, breaking sync. **Silently.** No error, just everything slightly wrong in a way that reads as a DSP bug.

Adding `sample_rate: u32` with a serde default costs nothing today. Retrofitting means guessing what rate every existing record meant.

### 4.2 `Arc<Vec<f32>>` is baked into the protocol plane and the RT path [V]

```rust
pub struct RegisteredSample { pub buffer: Arc<Vec<f32>>, ... }        // public trait type
pub struct SamplerVoice   { pub buffer: Option<Arc<Vec<f32>>>, ... }  // RT hot path
```

Also flows through `TopologyMutation::AddSource`. The session cache must be **mmap-able** (§3.1.9), and an mmap is not a `Vec`. Introduce an abstraction now (enum over Vec/mmap, or `Arc<dyn Deref<Target=[f32]>>`) — one indirection you will never measure. Retrofitting means changing a concrete type in a public trait and in the voice struct with audio running.

### 4.3 Content-addressed ids [V]

`id = DefaultHasher(path)` is persisted in every library row, every crate membership, every project's `sample_id`, and will be in every gossiped DNA record. Two independent reasons it must change:

- Renames orphan rows and force full re-analysis.
- **`std::collections::hash_map::DefaultHasher` is explicitly documented as not stable across Rust releases.** A `rustup update` can invalidate the entire library.

Switching later is a full library **and** project-file migration. It is also the prerequisite for dedup, P2P addressing and stable cross-peer references.

### 4.4 Two graphs, not one

See §3.3.11. If the mod graph uses the audio graph's topology structures, Kahn rejects cycles — and cycles are the point. You would either forbid the interesting patches or special-case the compiler.

### 4.5 Version the bus layout and the patch format

Saved patches bind to **slot semantics** — "input 7 is brightness." Change the 16 MD descriptors later and every saved patch, every light-rig mapping, and every trained `W` silently means something different. Not an error; a wrong sound.

A `bus_layout_version` on every persisted patch, and models declaring the version they were trained against, costs two bytes. Without it the 16 descriptors are locked the day the first patch is saved rather than the day you are confident in them.

### 4.6 License boundary [O]

`MARKET_COMPARISON.md` §4 flags this: *"License / cost: Undecided — decisive for this identity."* It is irreversible for a mechanical reason: **once external contributors land patches you cannot relicense without every contributor's consent.** In practice, never. So it must be settled **before the first outside PR**, not before 1.0.

It is also strategic. **Permissive (MIT/Apache-2.0) on the engine** is close to mandatory for the "Rust audio infrastructure" identity — GPL excludes every commercial embedder, i.e. most of the target users. **Copyleft on the application** protects against a proprietary fork of the DAW. These are not in conflict: **dual-license by layer** — engine/traits/ipc-layer/DSP permissive, conductor/inspector GPL. That only works if the boundary is drawn while you still own all the code.

---

## 5. Defects to close before or with implementation

### 5.1 Correctness

| # | Defect | Consequence |
| :-- | :-- | :-- |
| 5.1.1 | **`SamplerVoice::play_head` is `f32`** [V]. f32 integers are exact only to 2²⁴. Past 2²⁶ = 67.1 M frames the ULP is 8, and `play_head += 1.0` **rounds to no change** | **Playback freezes at 25.4 min @ 44.1 kHz; at 5.8 min @ 192 kHz.** Must become f64 or u64 fixed-point. Blocks 192 kHz entirely |
| 5.1.2 | **`pipewire.rs:239` truncates**: `num_samples = (size_bytes/4).min(MAX_BLOCK_SIZE)` with no chunking loop [V]. `alsa.rs` handles this correctly via an `offset` loop | Latent. Fires the moment `audio_backend: "Pipewire"` is selected with the graph at 1024 — half the output buffer left unwritten |
| 5.1.2b | **The sidecar bridge clamps rather than chunks** — `sidecar.rs` pushes `min(len, IPC_BLOCK_SIZE)` [V]. *Found during implementation of the constants change.* Same defect class as 5.1.2 | Harmless while the realtime block is 128–256, and now guarded by a `debug_assert!`. Blocks offline render and 512+ safe-mode from using sidecars at all until it splits across multiple `AudioBlock` pushes |
| 5.1.3 | **`MAX_MUTATIONS = 64` silently drops**: `if self.pending_mutation_count < MAX_MUTATIONS` [V] | A 4-deck bootstrap builds ~46 nodes plus edges. Already close; at 128 nodes with decomposed EQs it overflows, and the overflow is a silent drop presenting as an incomplete graph |
| 5.1.4 | **Sampler sync PLL saturates.** Measured: playback rate pinned at the +2% clamp for an entire track, with periodic ~19,000-frame (0.43 s) playhead jumps [M] | Track plays ~2% sharp with recurring re-seeks |
| 5.1.5 | **`expected_pos_samples` is wrapped `% source_frames` while `play_head` is not** [V]. Measured at end of track: `advance = -15,815,558` [M] | A long track **restarts instead of ending** |
| 5.1.6 | **`feature_vector[0]` semantic mismatch.** `mixer_orchestrator.rs:55` reads it as "average RMS energy"; `analysis_kernel.rs:270` writes **octave-band energy ratios** [V] | DNA-aware auto-gain compensates on sub-bass content, not loudness |
| 5.1.7 | **Two pre-existing test failures** — `test_golden_stereo_master_render` (render diverges from fixture) and `test_preview_command_is_audible`. Both confirmed failing on a clean tree [M] | — |
| 5.1.8 | **Clock-dependent flake.** `tick()` calls `tokio::task::spawn_blocking` when `epoch_secs % 60 == 0`; `golden_master_render_test.rs` has no Tokio runtime [V] | `test_capture_records_master_as_planar_stereo` fails ~1 second in every 60. Also: should `tick()` panic at all when no runtime is present? |

### 5.2 RT and systems hygiene

| # | Issue | Consequence |
| :-- | :-- | :-- |
| 5.2.1 | ~~**No `mlock` anywhere**~~ **FIXED in Phase 1.** `mlockall(MCL_CURRENT\|MCL_FUTURE)` now runs once in `start_backend`, verified by reading back `VmLck` from `/proc/self/status`. Swap remains enabled (4 GB), which is fine now that locking succeeds | Failure chain it closes: unbounded registry → 51.7 GB on a 500-track library → memory pressure on 14 GiB → kernel swaps → the audio thread's pages go to a SATA SSD → a block deadline becomes a multi-millisecond dropout. **Note this was NOT the cause of the observed tail** — measurement showed 0 major page faults; see the correction in §2.2. It is correct insurance against the residency problem, not a fix for the tail |
| 5.2.2 | **Pinning implemented but unused.** `pin_thread_to_core` exists; `setup_rt_thread(80, None)` is called without a core id [V] | On 2 cores, pinning the RT thread and keeping workers off its sibling hyperthread matters a great deal |
| 5.2.3 | **`DEFAULT_WORKER_COUNT = 4`** on a 2-core machine [V][M] | RT thread + 4 workers = 5 threads on 4 hardware threads. Actively harmful; must derive from `available_parallelism()` |
| 5.2.4 | **CPU governor is `powersave`** [M] | Aggressive downclocking precisely when the RT thread needs clocks. Detect and warn at startup (~20 lines) |
| 5.2.5 | **`SampleRegistry` has no removal method** [V], and `scan_folder_sync` decodes and registers *every* file it finds | 51.7 GB for 500 tracks. Every other RT guarantee is conditional on the user not scanning a large folder |
| 5.2.6 | **`checkpoint()` snapshots the entire registry per undo step**, 50 deep [V] | Each step `Arc`-clones every loaded sample. Combined with edits that *materialise* new buffers (`Crop`, `TimeStretch`, `Normalize` via `map_planes`), 50 edits of a 127 MB track can pin **~6.3 GB**. Also O(library) work per user action. Fix ties to §9.3 (reference-based edits) |
| 5.2.7 | **Never calibrate from a cold start.** i5-7300U: 2.6 GHz base, 3.5 GHz turbo, sustained all-core on 15 W settles ~2.2–2.6 GHz | Calibration in the first 30 s measures turbo and sets budgets the machine cannot sustain — passes, then xruns ten minutes into a set. `calibration_samples` / RTL calibration must run **warm**. FLOOR tier should target ~**50% of nominal budget**, not 90% |

### 5.3 Built and never wired

Four instances of one failure mode. Each has passing unit tests and is unreachable in the running product.

| Subsystem | State [V] |
| :-- | :-- |
| `StreamingManager` | Complete — disk decoder thread, SPSC channel, feeder, SHM ring. Downmixes everything to **mono**, pushes one sample at a time with 2 ms sleeps when full, and is attached to `STREAMING_SAMPLER`, a node type **the console never instantiates** |
| `PersonalityInheritanceProcessor` | The one implementation of the donor→recipient transfer model. **Not instantiated** in the graph (`dj.rs` builds `DNA_MORPH` + `KEY_SYNC`); `set_source_personality` is called **only from a unit test**, so source DNA is permanently all-zero. Also allocates in `process()` (`vec![0.0; input.len()]`), violating the Law of Zero Allocation |
| Deck DNA panel | Four knobs (METALLIC/ORGANIC/WARM/AGGRESSIVE) sending `ApplyFeatureMutation` → `FeatureMutator::mutate`, which edits `latent_space`/`tilt`/`glitch_density` in a metadata struct. **No processor reads those fields at playback.** Completely silent; overwrites measured analysis with fabricated values; lost on restart. **Recommend deletion** — it contradicts the "run it raw" goal |
| Breeder view | `target_node_idx: 150` — that is `ProcessorTypeId::PERSONALITY_INHERITANCE` used as a *graph index*. `MAX_NODES = 64`, so every command it sends is silently dropped. Exactly the failure `AGENTS.md` warns about under "Logical vs. Graph Node Ids" |
| `DnaMorpher` | *Is* in every deck chain, but ships `engaged: false` and nothing sets `dna_a`/`dna_b` — a dry passthrough |

**Diagnosis:** the verification discipline is real but measures the wrong property. **Tests verify that things exist; nothing verifies that things are reachable.** Every one of these is `[V]`-tagged in `MARKET_COMPARISON.md` — not dishonestly, but because "tested" and "reachable in the product" are different properties and only the first is checked.

**Fix — a reachability gate in CI:**
1. Enumerate every factory in `ProcessorRegistry`; assert each is instantiated by some bootstrap topology or explicitly annotated experimental.
2. Assert every UI-issued command target resolves to a live node index in the running graph.
3. Assert every telemetry field written has at least one reader, and vice versa.

That single check would have caught all four subsystems, the node-150 bug, and the dead `waveform_peaks` producer that wedged the console.

### 5.4 Verified good — do not re-litigate

| | Status [V] |
| :-- | :-- |
| **FTZ/DAZ denormal handling** | Correct. Set permanently per RT thread in `setup_rt_thread`; RAII `FpControlGuard` that *restores* for JACK's shared process; ALSA sets it too. Professional-grade |
| **Panic / lock discipline on the RT path** | Clean. The `unwrap`/`expect`/`Mutex` hits in `audio-core` and `nullherz-processors` are in `#[test]` blocks and test doubles. `metrics.rs` uses `try_lock` with an explicit comment about never blocking on the audio thread |
| **Planar sample layout** | Decided, documented, load-bearing, and correctly enforced |
| **Sample-accurate sub-blocks** | `sub_block_offset` / `is_last_sub_block` already in `ProcessContext` |
| **Lock-free architecture** | Rings, no-alloc lint enforcement, SIMD, double-buffered graph swap with garbage return, PTP clock discipline. These are the right choices and they produce the §2.2 numbers |

---

## 6. Platform and backend

### 6.1 The audio chain is not what the config says [M]

```
engine            thinks: 44100 Hz, 256-frame blocks
   ↓ snd_pcm_open("default")            ← alsa.rs:103, hardcoded
pipewire-alsa plugin                     ← /usr/share/alsa/alsa.conf.d/99-pipewire-default.conf
   ↓ RESAMPLE 44100 → 48000
PipeWire graph                           ← clock.rate=48000, clock.quantum=1024,
   ↓                                        clock.allowed-rates=[48000]
PipeWire's ALSA sink → kernel ALSA → hardware: 48000 Hz · S32_LE · period 1024 · buffer 32768
```

`"Alsa"` in `system_config.json` **does not mean direct hardware.** Three consequences:

1. **An unrequested resampler on every sample.** `allowed-rates` is `[48000]`; PipeWire will not switch to 44.1k.
2. **Latency you cannot see or control.** Quantum 1024 @ 48 kHz = 21.3 ms; realistic output latency **40–70 ms** — not the 5.8 ms the block budget implies.
3. **Misleading benchmark labels.** The survival harness prints `Backend: Alsa`, which reads as "direct hardware, pro latency." Any latency figure measured this way is measuring PipeWire's quantum.

**This settles the canonical rate.** The hardware is at 48 kHz and PipeWire will not move. Running the engine at 44.1 kHz costs a resampling stage permanently for no benefit — and the **73 hardcoded `44100`s** [M] mean the engine currently *cannot* run at 48 k to avoid it. Matching 48 kHz **deletes a stage from the signal path**.

### 6.2 Backend strategy [D]

Stop opening `"default"` for the pro-audio path — it is the worst option: PipeWire's latency *plus* a resampler *plus* no graph integration *plus* no visibility into real device parameters. (Related: `enumerate_devices()` returns a hardcoded `["default", "hw:0,0"]` with a comment admitting it is a stub.)

Expose **two named modes** rather than a latency ladder:

| Mode | Path | Latency | Coexistence |
| :-- | :-- | :-- | :-- |
| **Studio / desktop** *(default)* | PipeWire native or JACK API (`pipewire-jack`) | quantum-dependent; `min-quantum` is 32, so 128/256 achievable | ✅ browser, video call, lighting software |
| **Performance** | `hw:N,M` exclusive, requesting release via the D-Bus device-reservation protocol (`org.freedesktop.ReserveDevice1`) | best | ❌ — *and that is a feature*: nothing can interrupt a set |

Note that PipeWire is already holding the card, so `hw:0,0` will most likely return `EBUSY` without the reservation handshake.

**PipeWire at 128–256 quantum is within a few ms of direct hardware.** The old JACK-vs-Pulse tradeoff is gone; PipeWire implements both APIs on one graph. Direct hardware still wins for **live tracking**, where monitoring round-trip is perceptible; for DJing and playback-oriented production the difference is inaudible.

**Realistic target for the reference machine: 256 quantum**, after the governor, `mlockall` and pinning are fixed. 512 if the tail proves stubborn. Do not chase 128 while a 732 µs spike exists — that is 27% of a 2.67 ms quantum.

**Two cheap fixes regardless:** make the device string configurable, and **query and report the rate and period actually obtained, warning on mismatch with config.** Today the config says 44100/256 and reality is 48000/1024, silently.

### 6.3 Constants and address spaces

| Constant | Now | Proposed | Rationale |
| :-- | :-- | :-- | :-- |
| **`MAX_BLOCK_SIZE`** | 256 | **1024** *(revised — see note)* | Enables efficient offline render and a large-buffer safe mode. **Raise as *capacity*; keep the realtime default at 128–256** — at large blocks a stereo buffer pair no longer fits comfortably in the 512 KiB L2 once several decks are live, so big blocks are *worse* per-sample for realtime |
| **`IPC_BLOCK_SIZE`** *(new)* | — | **256** | **Split out of `MAX_BLOCK_SIZE` during implementation.** `AudioBlock` embeds `[f32; MAX_BLOCK_SIZE]` and is a shared-memory **protocol ABI** type, allocated in fixed 16-deep rings for every input, output and sidechain channel of every sidecar. Tying it to render capacity would have inflated every ring 8× (17 KB → 132 KB) while `len` still reported the 256-frame realtime block — paying bandwidth and residency for frames never used. Decoupling keeps `AudioBlock` at **1088 B, unchanged**: zero protocol churn. Capacity raised to 1024 rather than 2048 to keep the render/IPC gap small until the bridge chunks (§5.1.2b) |
| **`MAX_BUFFERS`** | 128 | **240** | ~87 used; decomposing deck EQs for taps adds ~8/deck → ~119. Hard ceiling **247** — `block_x_map` packs `MAX_BUFFERS + k` in a `u8`, compile-asserted. **Since taps observe buffers, this is the tap budget** |
| **`MAX_NODES`** | 64 | **128** | 6 compile errors, all hardcoded `64` literals in `telemetry_finalizer.rs` [M]. Cost: `Telemetry` 6,456 → 7,736 B; `CompiledGraphPlan` 21 → ~74 KB (4N²); ring traffic 1.11 → 1.33 MB/s. Negligible. The N² term is *allocation*, not traversal |
| **`MAX_MUTATIONS`** | 64 | **256** | Silently drops today (§5.1.3) |
| **`MOD_SLOTS`** | — | **512** | 4 KB double-buffered |
| `MAX_CHANNELS` | 16 | 32 *(optional)* | Cheap; only needed if surround/Atmos enters scope (7.1.4 = 12) |

**Prerequisite before raising `MAX_NODES`:** `NodeConventions` sentinels (`DECK_*_SEQUENCER` = 70–73, `PREVIEW` = 111) are **deliberately** ≥ `MAX_NODES`, with the safety net documented in the code as *"the graph drops indices >= MAX_NODES."* At 128 **all five become legal graph indices** and the net dies for exactly those values. Move them out of numeric range — or better, make them a distinct type, the same reasoning that produced `BufferId`.

Governing principle: **compile-time constants define *capacity*; runtime configuration defines *usage*.** Identical binaries and data layouts on every machine → portable projects and meaningful golden-master tests across tiers.

---

## 7. Tiers and the floor contract

**Principle:** do not constrain the software to the reference hardware, but the basic setup must run on it. More powerful machines get smoother, not different.

**The tension:** "automatic" means adaptive; "stable" means deterministic. Adaptive behaviour under pressure introduces variance at the worst moment. Resolution:

> **Probe once at startup. Preallocate generously. Then degrade by shedding optional work — never by allocating.**

1. All sizing decisions happen at init or session-open, never per block.
2. "Automatic allocation" means automatically-*sized* preallocation, not runtime growth.
3. **Overload response is subtractive** — drop taps, reduce telemetry detail, coarsen waveform LOD. Never allocate to cope; never change block size mid-session. This makes degradation a finite, enumerable, testable list.

| | **FLOOR** (reference machine) | STANDARD | STUDIO |
| :-- | :-- | :-- | :-- |
| Probe | ≤2 cores, ≤16 GB, SATA | 4–8 cores, 16–32 GB, NVMe | 8+ cores, 32 GB+, NVMe |
| Session rate | **48 kHz** | 48 / 96 kHz | up to 192 kHz *(archive only on FLOOR)* |
| Block / quantum | 256–512 | 256 | 128–256 |
| Workers | **1** | 3 | 7+ |
| Spectral taps | **~8** | ~24 | 64+ |
| Mod graph | sparse | sparse or dense | dense 512×512 |
| Session cache | **streaming mandatory** | streaming | streaming or resident |

**Tiers change budgets, not code paths.** No tier-specific branches in the DSP path, or there are three products to test.

**Hardware envelope**

| | Minimum | Comfortable | Driver |
| :-- | :-- | :-- | :-- |
| CPU cores | 2 | 8+ | RT thread wants a near-exclusive core |
| RAM (streaming) | 8 GB | 16 GB | ~17 MB/deck + 160 MB index per 1 M tracks |
| RAM (no streaming) | 16 GB | 32 GB | 4 decks × 276 MB @ 96 k = **1.1 GB** of deck buffers alone |
| Disk | 500 GB | 2 TB NVMe | **553 MB per 6-min 192/32 track**; the reference machine's 238 GB holds ~430 uncompressed / ~860 compressed |
| Network (P2P) | 10 Mbps | 50 Mbps | ~96 KB/s per stream; control frames 15 KB/s |

**Streaming is what makes the hardware requirement civilised.** Without it, 4 decks at 96 kHz is 1.1 GB of audio buffers before a library is loaded.

### The stability contract

> **The FLOOR tier passes `bin/survival` on the reference machine — 4 decks, warm, 60 minutes, zero xruns — with `mlockall` on and the governor at `performance`.**

The harness already exists (xrun timeline, budget-overrun counting, markdown report, non-zero exit on any xrun). It needs to become the gate rather than an occasional diagnostic. **It would not pass today: 453–642 xruns observed** [M], with 4 workers on 2 cores, `powersave`, and no streaming.

---

## 8. Open questions requiring a human decision

| # | Question | Notes |
| :-- | :-- | :-- |
| 8.1 | **Are those the right 16 MD descriptors?** | Every patch, light rig, peer and trained `W` binds to this list. Chosen for musical patchability and mutual independence; the producer is the better judge of what a performer reaches for. **The most expensive item here to change later** |
| 8.2 | **Does MD-static belong in the replicated index?** | Included above (160 B/track) so cloud search can filter on "punchy, bright, wide" as well as similarity. Dropping it halves the index to 96 B and loses semantic filtering |
| 8.3 | **Neuron form: weighted-sum + `tanh` (general), or a curated function set** (`sum`, `difference`, `product`, `min`, `max`, `crossfade`)? | The general form is more powerful and much harder to make comprehensible in a UI. Product question |
| 8.4 | **License boundary** (§4.6) | Must precede the first external contributor |
| 8.5 | **Does the P2P network move audio, or only DNA?** | Replicating 64–160 B of measured descriptors is legally trivial — facts about audio, not audio. Replicating audio chunks defines how the project is perceived regardless of intent. The layered design makes the safe version natural: **global index is descriptors; audio is content-addressed but permissioned.** That is plausibly also the business model — discover by sonic DNA, acquire from the rights holder |
| 8.6 | **Canonical rate confirmation** | §6.1 makes 48 kHz evidence-backed, but confirm against the actual catalogue: if it is overwhelmingly 44.1 k releases, the resampling burden shifts |
| 8.7 | **Sidecar density budget** | `ipc_audio_bridge` has a jitter buffer with drift compensation, implying each sidecar crossing adds *latency*, not just CPU. The §2.2 numbers are in-process only. Process isolation is a real reliability win but trades against density — quantify before leaning on it as a headline feature |

---

## 9. Sequencing

### 9.1 Prerequisites — before any tap/DNA implementation

| # | | Effort |
| :-- | :-- | :-- |
| 1 | `play_head` → f64 (§5.1.1) — a live bug at the current rate | small |
| 2 | `SampleMetadata.sample_rate` (§4.1) — corrupts silently | small |
| 3 | Bus layout + patch versioning (§4.5) — corrupts silently | small |
| 4 | Move `NodeConventions` out of numeric range (§6.3) | small |
| 5 | `MAX_NODES` 128, `MAX_BUFFERS` 240, `MAX_BLOCK_SIZE` 2048, `MAX_MUTATIONS` 256 | small |
| 6 | Worker count from `available_parallelism()`; governor detection + warning | hours |
| 7 | `mlockall` + pre-touch (§5.2.1) | small |
| 8 | Use the existing thread pinning (§5.2.2) | small |
| 9 | Reachability gate in CI (§5.3) | small–medium |

### 9.2 Foundation

10. Registry eviction / mmap session cache — makes the FLOOR memory budget possible at all
11. Streaming against **plain files** — the single number the storage argument rests on: **681 ms → ~4 ms** first sound. Prove it before committing to a format
12. Content-addressed ids (§4.3)
13. **Modulation moves into `audio-core`** (§3.3) — the least reversible item
14. Diagnose the 462–732 µs tail; confirm `survival` green at 256 quantum

### 9.3 System

15. Tap bus + subscription + sparse `W` + bus recorder
16. Reference-based edit list — a clip is `(sample_id, start, end, gain, fades)` over an immutable source; render only on bounce. Fixes §5.2.6 and is what actually makes chop/loop/merge fast. **No storage format makes a crossfade faster**
17. Binary library rows (rkyv derives already exist on `SampleMetadata` [V])
18. DNA schema rewrite: SM/MD/LG, quantized, mel-spaced, aggregation operators
19. Chunked container (Merkle-ready), then P2P

### 9.4 What is deliberately out of scope

- **Audio-rate cross-channel feedback** — needs explicit delay buffers; separate feature.
- **ML inference runtime** — build the matrix and the recorder; add inference only when a trained model beats a hand patch. Otherwise: a runtime with nothing to run.
- **Removing DNA breeding** — retained and repositioned (§3.2).
- **User-placeable taps at arbitrary sample offsets** — fixed buffer-edge taps cover the need at zero cost.
- **Custom filesystem** — wrong layer. Chop/loop/chain/merge are in-memory operations governed by the edit model, not by disk layout. An FS layer would reimplement the page cache and crash consistency to gain nothing while breaking every backup and sync tool.
- **Custom lossless codec** — see §3.1.1.

---

## 10. Honest position

**What is proven:** the DSP is not the constraint. **2.0–2.8% of block budget** for the 4-deck console on a 2017 15 W dual-core; **1.7% of one core per second of audio**; ~18 µs fixed overhead so it scales down to low latency gracefully. FTZ/DAZ, panic and lock discipline are correct. The architecture — lock-free, no-alloc, SIMD, double-buffered graph swap — is the right set of choices and it is what produces those numbers.

**What is not:** delivered RT reliability. No `mlock`, swap enabled, pinning unused, the tail undiagnosed, 453–642 xruns in survival today, and two failing tests. Large-session scaling is entirely unmeasured — 46 nodes is an easy problem; nobody has run this at 500.

**Where leadership is plausible:** not speed. **Crash-isolation granularity** (per-node, finer than any shipping product), **machine-checked invariants** (Kani — essentially nobody in audio does this), **remote DSP offload**, and **DNA-driven cross-adaptive modulation**. Those are defensible firsts.

**Strategic caveat:** performance is a credibility floor, not a differentiator. Reaper is the most efficient DAW ever shipped and holds low single-digit share. What the §2.2 headroom actually buys is **room for the features that differentiate** — 16 spectral taps plus a mod graph fits in a quarter of the period.

**And on open source:** for *end-user creative tools* the track record is poor (Ardour, Mixxx, LMMS, Zrythm — none lost on technical merit). For *infrastructure* it is the winning model, because the users are developers who evaluate on exactly these axes. `MARKET_COMPARISON.md` §4 already identifies this correctly. Every network feature therefore needs a **single-player mode that is independently worth having** — content addressing, DNA search, cross-adaptive modulation and the chunked container all pass that test; P2P fetch does not, and must never become a precondition for a core workflow.

---

## Appendix A — Corrections owed to `MARKET_COMPARISON.md`

| | |
| :-- | :-- |
| **Bitwig Studio is absent from §2**, and it is the closest competitor — per-plugin process isolation is *their* headline feature and predates Ableton's, and The Grid is the nearest existing thing to the cross-adaptive modulation described here. Its omission makes the moat look wider than it is |
| **Stem separation is absent entirely.** Serato, rekordbox, djay and VirtualDJ shipped real-time stem separation around 2023; it moved from novelty to expectation quickly. §7's "AI Integration: VST-bolted" is out of date for the DJ segment |
| **No VST3/AU/CLAP hosting.** The sidecar SDK is a bespoke ABI, so there is zero existing plugin ecosystem. For a producer tool that is an adoption wall, not a feature gap. It deserves a row |
| **§3.1 claims parity or advantage on BPM/key/transient sync** while the console could not play a full-length track. Table stakes outrank differentiators |
| **"Modulation Matrix (test-pinned) [V]"** is real but its only source is a UI macro; there is no audio-derived modulation source anywhere. Technically true, strategically misleading |
| **"redb + JSON/rkyv round-trip [V]"** — the JSON half is what wedged the console. Parity with `.als`, not advantage |
| **The `[V]` tag needs a stronger definition.** A unit test can pass on a processor that is never instantiated (§5.3). Until the reachability gate exists, `[V]` means "a test passes", not "a user can do this" |
