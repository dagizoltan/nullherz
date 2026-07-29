# Implementation Roadmap — July 2026

**Companion to:** [`docs/system/PRE_IMPLEMENTATION_DESIGN_GATE_2026_07.md`](../system/PRE_IMPLEMENTATION_DESIGN_GATE_2026_07.md) (design decisions, measurements, defect inventory).
**Scope:** every item discussed at the design gate, sequenced to completion.

---

## How this is ordered

Three rules govern the sequence:

1. **Silently-corrupting defects first.** Anything that produces wrong data without an error ships damage into users' libraries. These are one-field changes; they must not wait.
2. **Irreversible decisions before dependent work.** A schema or thread-boundary decision made late is a migration or hot-path surgery. §0 and the start of each phase carry these.
3. **Every phase ends in a measurable gate.** Not "done" — a number or a green test. Phases without gates become open-ended.

**Effort sizing** is relative: **S** ≈ hours–2 days · **M** ≈ 3 days–2 weeks · **L** ≈ 3–8 weeks. Calibrate against your own velocity; the ratios matter more than the absolutes.

**Marking something done.** A row is struck through only with the file it lives in
named, so the claim can be checked instead of believed. This matters because the
failure has already happened here twice, in both directions: the archived status
documents claimed maturity that did not exist, and then *this* document — audited
2026-07-29 — turned out to be **under**-reporting, with Phases 0 and 1 reading as
open while the code said 8 of 10 and 7 of 11 were finished. Stale-in-your-favour is
still stale: it hides that the real blocker had moved to Gate 1, which is a
60-minute unattended run nobody had started.

Where an item is partly done, say which part and where the rest went — a row that
reads DONE with a quiet remainder is how work gets lost between phases.

**Track U** (usability) runs in parallel from the start — it touches UI files that the architecture phases don't, and it keeps the product usable while the deep work lands.

---

## Phase 0 — Schema locks and silent-corruption fixes

*Nothing else starts until this is merged. Every item is small; several are unrecoverable if deferred.*

> **STATUS: complete, verified against the code 2026-07-29.** Eight of the ten
> items were already done and this document had not been updated to say so —
> two whole phases read as open while the code said otherwise, which is the same
> failure the archived status documents were retired for. Each row below now
> records where the implementation lives, so the claim can be checked rather than
> believed.
>
> Two rows carry a **remainder that is not Phase 0 work** (0.2, 0.5): their
> subject is the Phase 5 control bus, which does not exist yet. They are marked
> and cross-referenced rather than left looking unfinished. **Gate 0 is met.**

| # | Item | Size | Why now |
| :-- | :-- | :-- | :-- |
| 0.1 | ~~**`SampleMetadata.sample_rate: u32`** (serde default 44100)~~ **DONE** — `nullherz-traits/src/dna_schema.rs`, `#[serde(default = "default_sample_rate")]`, with the 8.2%-drift rationale recorded on the field itself | S | Every `hot_cues`, `loop_points`, `beat_grid_offset`, `transients`, `total_samples` is frames with no rate. At 44.1→48 every cue is **8.2% early** and beat grids drift. Silent. Retrofit = guessing what old records meant |
| 0.2 | **Patch format version DONE; `bus_layout_version` DEFERRED TO PHASE 5.** `CURRENT_PROJECT_VERSION` exists (`conductor/src/persistence.rs`) and `ProjectState` carries it. `bus_layout_version` has no subject yet — the 16 bus descriptors it would version arrive with 5.1/5.2. **Make it an entry condition on 5.1**, not a Phase 0 debt: versioning a layout before the layout exists pins nothing | S | Saved patches bind to slot semantics. Change the 16 descriptors later and every patch, light-rig map and trained `W` silently means something else |
| 0.3 | ~~**`play_head`, `background_playhead`, `trigger_offset` → f64**~~ **DONE** — all three are `f64` in `audio-dsp/src/oscillators.rs`; positions stay f64 through the sync path | S | f32 freezes playback at **25.4 min @ 44.1 kHz**, **5.8 min @ 192 kHz**. Live bug; blocks 192 k entirely |
| 0.4 | ~~**Move `NodeConventions` out of numeric range**~~ **DONE, and better than asked.** Sentinels moved to a `LOGICAL_BASE = 0xFFFF_FF00` range, plus `is_logical()` (preferred over comparing individual constants) and a `_SENTINELS_ABOVE_MAX_NODES` const assert that turns any future `MAX_NODES` increase reopening the hole into a **compile error**. Guarded by `nullherz-traits/tests/node_sentinel_test.rs` | S | Prerequisite for 0.5. At `MAX_NODES=128`, sentinels 70–73 and 111 become *legal* indices and the "drops ≥ MAX_NODES" net dies |
| 0.5 | **Mostly DONE.** `MAX_NODES` 128 ✓ · `MAX_BUFFERS` 240 ✓ · `MAX_MUTATIONS` 256 ✓ (`nullherz-traits/src/execution.rs`, guarded by `tests/capacity_constants_test.rs`). Two deltas: **`MAX_BLOCK_SIZE` is 1024, not 2048** — deliberate, it sizes real buffers and 1024 covers every quantum PipeWire negotiates here; raise it only with a measured need. **`MOD_SLOTS` absent** — it sizes the Phase 5 control bus, so it lands with 5.1 | S | 6 compile errors for `MAX_NODES`, all hardcoded `64` in `telemetry_finalizer.rs`. Total memory cost ~2.2 MB |
| 0.6 | ~~**`pipewire.rs:239` chunking loop**~~ **DONE** — extracted to `nullherz-backends/src/chunking.rs` and shared with ALSA rather than duplicated | S | Truncates any quantum > 256; PipeWire commonly negotiates 1024 |
| 0.7 | ~~**Sampler sync: PLL clamp saturation + end-of-track wrap**~~ **DONE, both halves, by dissolving the mechanism rather than patching it.** Track sync now locks **beat PHASE**, not absolute position: `transport.beat_position.fract()` against the playhead's position within the beat. That is bounded to half a beat by construction, so the `% source_frames` wrap is gone entirely — and with it the end-of-track restart, which was caused by wrapping the TARGET while leaving the PLAYHEAD unwrapped. Clamp saturation addressed by correcting toward the *nearest* beat (not always forward), a one-frame deadband, and `k_p = 0.001` clamped to ±2%; rate is EMA-smoothed (α = 0.1) to kill the pitch-modulation buzz. Slicer mode keeps absolute comparison, which is correct — `trigger_offset` anchors it | M | Rate pinned at +2% for whole tracks with 0.43 s re-seeks; `expected_pos % source_frames` while `play_head` isn't wrapped makes a long track **restart instead of ending** |
| 0.8 | ~~**`feature_vector[0]` semantics**~~ **DONE** — band layout fixed in `analysis_kernel.rs`; the bogus reader was removed from `mixer_orchestrator.rs` rather than re-pointed, because DNA auto-gain needed a real loudness measure (LUFS, Phase 4.8) and none exists. Guarded by `conductor/tests/feature_vector_test.rs` | S | `mixer_orchestrator.rs:55` reads "average RMS"; the kernel writes octave-band ratios |
| 0.9 | ~~**Fix the two failing tests + the clock-dependent flake**~~ **DONE (2026-07-28).** The `spawn_blocking` panic was already fixed (`spawn_background` guards on `Handle::try_current`). Two further causes found and fixed: (a) `long_track_control_path_test` asserted a 5.8 ms wall-clock budget in a **debug** build, so `cargo test --workspace` — the documented gate — failed on build profile, not regression; budgets are now release-only with the profile-independent ratio invariant still asserted in both, and a `test-release` CI job enforces them. (b) **Six tests opened the developer's real 233 MB `library.redb`** (five via `Conductor::new()`, one via `OfflineRenderer::new()` transitively). This was silent: redb's exclusive lock makes `with_library_path` substitute a private fallback instead of erroring, so under parallel test binaries the first opener got the real library and the rest got empty ones — the same test, two different worlds, plus 25 accumulated `fallback_*.redb` files. Tests now use `":memory:"`, the fallback announces itself, `OfflineRenderer` takes an injected path, and `test_isolation_gate_test.rs` guards both. Suite verified **hermetic** by hashing the working tree across a full run. Caveat: the original 1-in-5 abort was never reproduced under capture (17 clean runs since), so it is not *confirmed* to be this defect | M |
| 0.10 | ~~**Reachability gate in CI**~~ **DONE (2026-07-28).** `conductor/tests/reachability_gate_test.rs` covers all three clauses: every registered factory is instantiated by the bootstrapped console or declared in `known_unreachable()` **with a reason**, every literal `get_node_id("…")` in the views resolves against the real graph, and no view hardcodes a graph index. It was *written* earlier but was **not in CI** — `.github/workflows/ci.yml` had been deleted in commit `968cd12` ("patch"), so for roughly a day nothing gated `main` at all. CI restored, so the gate now actually gates. **14 of 24 processors remain on the allowlist; shrinking that number is the honest measure of the product getting more capable rather than merely larger** | M | Would have caught all four unwired subsystems, the node-150 bug, and the dead `waveform_peaks` producer. Makes `[V]` mean "a user can do this" |

**Gate 0:** full suite green, reachability gate passing, no silent-truncation or silent-drop path remaining.
**→ MET (2026-07-29).** Suite green in both profiles (368 debug / 369 release) and hermetic; reachability gate passing and in CI; the silent-drop paths named in 0.4 and 0.7 are closed by construction rather than by guard (sentinels are unrepresentable as graph indices; phase-lock removed the wrap that caused the end-of-track restart).

---

## Phase 1 — RT floor: make the good numbers *delivered*

*The DSP already fits (2.0–2.8% of budget). This phase is about blocks arriving on time.*

> **STATUS: 7 of 11 done, verified against the code 2026-07-29. The gate itself is
> the blocker, not the work.** `mlockall`, thread pinning, worker count, governor
> detection, backend honesty, the 44100 purge and the 48 kHz move are all in.
> What remains (1.5, 1.9, 1.10, 1.11) is tuning and diagnosis, none of it
> prerequisite to *running* Gate 1.
>
> **Latency note (2026-07-29):** this phase is about blocks arriving on time, and they do — 0 xruns over 5 min on real ALSA hardware. But *action-to-sound* latency is **28.7 ms**, dominated by an FFT window in every deck (new row 1.12), and no amount of Phase 1 tuning touches it. Do not read a green Gate 1 as "the console is low-latency".
>
> **Gate 1 has never been run to its own terms.** It needs 60 minutes on a real
> backend; the only survival evidence on record is 1–2 minutes on **Threaded**,
> which is the software-clocked fallback and — per this document's own debt log —
> structurally unable to observe hardware xruns. That is unattended wall time
> rather than effort, and everything in Phases 2–7 assumes the floor it proves.
>
> **Before running it, check the box.** The 2026-07-29 reference machine had
> **3.5 GB of its 4 GB swap in use**. `mlockall` protects the audio pages, but a
> machine under that much memory pressure will produce a red result that tells
> you about the host, not the code.

| # | Item | Size |
| :-- | :-- | :-- |
| 1.1 | **`mlockall` DONE; pre-touch NOT.** `mlockall(MCL_CURRENT\|MCL_FUTURE)` runs before the first audio thread exists (`conductor/src/orchestrator.rs`, logs `[RT] memory locked`), and `ipc-layer/tests/realtime_environment_test.rs` verifies it by reading back `VmLck` rather than trusting the return code — a good test, since `mlockall` returning 0 proves nothing. **No pre-touch/prefault of the audio buffers exists**, so the first write to each page still faults even though the page cannot later be evicted. That is a startup-transient cost, not a steady-state one, which is why it has not shown up — expect it to matter for 1.5 (warm calibration measuring a cold graph) more than for Gate 1 | S |
| 1.2 | ~~**Pin the RT thread to a core**~~ **DONE, as opt-in — default-on was not justified.** The primitive already existed (`ipc_layer::pin_thread_to_core`) but was never called for the audio thread. Two reasons not to default it: this row's original justification was the 462–732 µs tail, which 1.10 showed to be a non-RT benchmark artifact; and the reference machine has no `isolcpus`/`nohz_full`, so affinity restricts only our own thread and reserves nothing. Added `NULLHERZ_AUDIO_CPU` opt-in plus `cpu_siblings()`/`has_isolated_cpus()`, and the startup note states both caveats. **Also fixed: every sidecar was hardcoded to `pin_thread_to_core(1)`** — all of them onto one CPU, and on this machine CPU1 is where `snd_hda_intel`'s IRQ is serviced. Now opt-in via `NULLHERZ_SIDECAR_CPU`. Real benefit still unmeasured; needs Gate 1 conditions | S |
| 1.3 | ~~**Worker count from `available_parallelism()`**, not `DEFAULT_WORKER_COUNT = 4`~~ **DONE** (`default_worker_count()` in `audio-core/src/engine/mod.rs`, `available_parallelism() - 1`). **Do not tune the pool further on 256-sample measurements.** Re-measured 2026-07-29: at 256 the cost gate never fires at all (one stage must clear ~46 µs alone; the whole 34-node graph is ~126 µs across many stages), so pool-on and pool-off are the same code path, and *forcing* dispatch costs 20% mean. At 1024 it fires and wins 7%. Worker count is not the lever — see 1.10 and `ARCHITECTURE.md` "Cost-gated parallelism" | S |
| 1.4 | ~~**Governor detection + startup warning**~~ **DONE** — `ipc_layer::realtime_environment_warnings()`, called from `orchestrator.rs:414` on the boot path. Observed live 2026-07-29: it correctly reports `powersave` on `intel_pstate` *and explains that this is the normal mode which does reach full clock*, naming ramp-after-idle as the actual risk, plus a separate warning that swap is enabled. That nuance is the difference between a useful warning and one people learn to ignore. *(An earlier revision of this row claimed the warning was unwired — that was a bad grep on my part: the callers use the `realtime_environment_warnings()` wrapper, not `cpu_governor()` directly. Corrected after seeing it fire.)* | S |
| 1.5 | **Warm calibration.** Never calibrate in the first 30 s — a 15 W part measures turbo and sets budgets it cannot sustain. FLOOR targets ~50% of nominal budget | M |
| 1.6 | ~~**Backend honesty**~~ **DONE.** Report rate/period + buffer in ms; NOTE on any `*_near` substitution; device configurable via `NULLHERZ_ALSA_DEVICE`/`set_device`; `enumerate_devices()` now queries `snd_device_name_hint` (25 real devices here) instead of returning a fabricated `["default", "hw:0,0"]`. **Correction to this row's premise:** the negotiated rate was *already* fed back via `set_config` (alsa.rs:215) — the gap was elsewhere, see 1.8 | M |
| 1.7 | ~~**Remove the hardcoded `44100`s**~~ **DONE.** Only ~14 of the 73 were runtime paths; the rest are DSP unit tests (valid) or *source*-rate fallbacks where 44100 is the correct legacy guess about a file. Split into `DEFAULT_SAMPLE_RATE` (what we ask the device for) and `LEGACY_SOURCE_SAMPLE_RATE` (deliberately pinned at 44100 so moving the session rate cannot transpose an existing library). Delay lines now sized in seconds; `Telemetry` gained `block_size` so DSP load is exact. **Found and fixed en route: an RT-thread crash** — the graph buffer pool was built from `AudioBlock` (`IPC_BLOCK_SIZE` = 256) while both backends chunked to `MAX_BLOCK_SIZE` = 1024, so any device period > 256 indexed out of bounds on the SCHED_FIFO thread. Reproduced at period 512, fixed with a distinct `RenderBlock` type | M |
| 1.8 | ~~**Canonical rate → 48 kHz**~~ **DONE.** Verified on hardware: `[ALSA] Negotiated: rate=48000`, so the resampling stage is gone (we previously asked for 44100, PipeWire accepted it and resampled to its own 48 k graph). **Was not an S.** Flipping the constant exposed that `set_config` only re-runs `setup()` on nodes already in the *active* graph, while `topology_manager.current_sample_rate` — what the factory passes to every node it CONSTRUCTS — was never updated from the device at all. Added `Conductor::sync_session_rate()`. Also: `SignalProcessor::setup` defaults to a no-op, so `DjIsolator`, `Compressor` and `EnvelopeFollower` silently kept construction-rate coefficients (the isolator's 300 Hz/3 kHz crossover landed at ~276 Hz/2.76 kHz). Guarded by a sweep test asserting build-at-A-then-setup(B) ≡ build-at-B | M |
| 1.9 | **Two backend modes:** *Studio/desktop* (PipeWire native or JACK API, quantum configurable) and *Performance* (`hw:N,M` exclusive with `org.freedesktop.ReserveDevice1` handshake — PipeWire holds the card, so `hw:` returns `EBUSY` without it). **The worker pool splits along this line too** (measured 2026-07-29): at Studio quantums (512–1024) the cost gate fires and beats serial by 5–7% with its jitter at 11–18% of budget; at Performance quantums (256) it never fires and forcing it eats 50% of budget in tail. Whatever this row lands, the pool should follow the mode rather than be tuned globally | M |
| 1.10 | **Diagnose the tail.** *Premise corrected 2026-07-29: the serial tail DOES scale with block size* — p99.9 measured at **270 / 646 / 867 µs for 256 / 512 / 1024** samples, roughly linear. That points at proportional work (the DSP itself, or a per-sample path with a bad constant), **not** the fixed-cost page-fault/scheduler jitter the original row assumed. Useful discriminator: the *worker pool's* tail is genuinely fixed-cost by contrast (2662 / 1900 / 2410 µs at the same sizes), so the two have different causes and want different fixes. Note also that 1.1's `mlockall` and 1.2's pinning have landed since the original measurement, so the 462–732 µs figure is stale — re-baseline before diagnosing. Harness: `cargo run --release -p nullherz-conductor --example bench_console_block` with `NULLHERZ_BENCH_BLOCK_SIZE` | M |
| 1.11 | **Tier probe + budget table** (FLOOR/STANDARD/STUDIO). Tiers set budgets, never code paths. Overload response is subtractive: shed taps, coarsen telemetry, never allocate | M |
| **1.13** | **NEW — Feature-gate the WASM sidecar host.** `wasmtime = "22.0"` is an unconditional dependency of `fx-runtime`, so every build — including the headless daemon — carries a full WASM JIT: **128 of the daemon's 244 unique crates**, for a host the console never references. Gate it behind a default-off feature. Justified on desktop merit alone (build time, binary size, supply-chain surface); it is also the first step toward a stripped build, which [`HARDWARE_DIRECTION.md`](./HARDWARE_DIRECTION.md) §3 records as currently impossible. Same defect class as 1.12 — an opt-in capability charged to everybody unconditionally | S |
| **1.12** | ~~**Get KeySync out of the default deck chain.**~~ **DONE 2026-07-29 — measured 28.7 ms → 7.33 ms at block 256, 3.33 ms at block 64.** The deck chain is now `Sampler → [pitch slot] → [dna slot] → Gain → EQ → StereoUtil → [fx slots] → DjIsolator`, where every slot holds `ProcessorTypeId::BYPASS` (an identity pass-through, zero latency) until something is installed. `SetDeckKeySync` emits `SwapProcessor` + `CommitTopology` instead of a `SetParam`, so engaging KEY *installs* the vocoder and releasing it *removes* it. Making engagement a TYPE change is what keeps PDC correct — `latency_samples()` is read at compile time, so the latency has to arrive as a topology change. Guarded by `pdc_latency_test.rs` (5 tests; 2 fail if the commit-path sync call is deleted). `known_unreachable()` now declares KeySync/DnaMorph as *installed on demand* rather than absent. **Golden master regenerated:** L peak −9.6 dBFS / rms −18.9 dBFS vs the outgoing reference — the vocoder is no longer in the default path, so its artefacts are gone from the render; console re-measured at peak 0.5322, 2.3% of budget, 0 xruns. *A human listen is still owed.* ORIGINAL ROW: **Get KeySync out of the default deck chain.** *The single largest latency win available, and it is not a performance problem.* Measured 2026-07-29: end-to-end deck→master latency is **28.7 ms**, of which **21.3 ms is KeySync's 1024-point FFT window**, present in every deck on every block regardless of pitch ratio. Detaching it measures **7.3 ms** — a 21.4 ms recovery. It serves the KEY latch, which since RAW mode **defaults to off**, so today every user pays 21 ms for a feature nobody enabled. The topology system already supports the fix: insert/remove the node when the KEY latch flips (off-thread Kahn compile, O(1) `SetTopology` swap, and PDC re-polls `latency_samples()` at compile time, so the latency change is compensated correctly *because* it goes through a recompile). **Prerequisites, both DONE 2026-07-29:** (a) `KeySync` and `DnaMorpher` now declare their latency instead of inheriting the trait default of 0 — the graph previously believed the entire console had zero latency. (b) **The off-thread compile now receives those latencies at all.** `GraphCompiler` copies `plan.node_latencies` (its comment says "populated by GraphManager", which is the RT-side path), but the plan the *conductor* compiles is what ships as `SetTopology`, and the RT thread deliberately never recompiles it — so the conductor's plan is authoritative and **nothing conductor-side had ever written that array**. Every path latency was derived from zeros. Invisible while all four decks carry identical chains (nothing to compensate, so compensating wrongly looks correct); silently wrong the moment an insert makes them differ. Fixed by `ProcessorRegistry::latency_for_type` + `TopologyManager::sync_node_latencies`, called from `CommitTopology`. Guarded by `conductor/tests/pdc_latency_test.rs` — note the wiring test specifically, because the first version of that file passed with the call deleted (it proved the function worked, not that anything called it).

**Mechanism confirmed available for the insert work:** `TopologyCommand::SwapProcessor { node_idx, processor_type_id }` swaps a processor in place *preserving routing*, and the pattern is already proven in production by `SidecarSupervisor`'s fallback swap. So inserts should be **pre-allocated bypass slots swapped on demand**, not re-linked graphs. A useful side effect: making "engaged" a TYPE change rather than a parameter change resolves `DnaMorpher`'s conditional-latency hazard, because the type change is what triggers the recompile. Verify with `cargo run --release -p nullherz-conductor --example probe_deck_latency` | M |

**Gate 1 — the floor contract:** `bin/survival` green on the reference machine — **4 decks, warm, 60 minutes, zero xruns**, `mlockall` on, governor `performance`, at 256 quantum. (Today: 453–642 xruns.)

> **The harness could not fail this gate until 2026-07-27.** It graded on
> `telemetry.xrun_count`, plumbed from an `AtomicU32` in audio-core that
> **nothing ever incremented** — so `xruns == 0` was unconditionally true. A
> 2-minute run where the threaded backend printed `Total Xruns: 13` to stderr
> still reported `Xruns: 0` and PASS. It also had no output-level check, so a
> completely silent graph would have passed too (zero underruns is trivially
> true with no audio).
>
> Fixed: `AudioBackend::xruns() -> Option<u64>` (the `Option` is load-bearing —
> `None` means "this backend does not measure", which must not read as clean),
> implemented for ALSA and Threaded; the verdict now grades on the backend's
> counter and additionally requires signal in >90% of frames. Failure path
> verified by fault injection (7 injected underruns → FAIL). **Any Gate 1 PASS
> recorded before this date is void.**
>
> Interim evidence (not Gate 1): 2 min on Threaded at 48 kHz, 0 backend xruns,
> peak output 1.01, 100% frames with signal, peak block 615 µs against a
> 5333 µs budget. Held 0 xruns with 6 spinner processes on 4 logical CPUs
> (peak block rose to 1438 µs, still 27% of budget).

---

## Phase 2 — Memory model and streaming

| # | Item | Size |
| :-- | :-- | :-- |
| 2.1 | **Sample buffer abstraction** — replace `Arc<Vec<f32>>` in `RegisteredSample`, `SamplerVoice` and `TopologyMutation::AddSource` with something that can be a `Vec` *or* an mmap | M |
| 2.2a | **Registry eviction: mechanism DONE, policy OPT-IN.** `SampleRegistry::remove()` added (no default impl — a defaulted `None` would let residency silently stay unbounded), plus `Conductor::reap_registry()`, gated behind `NULLHERZ_REGISTRY_REAP=1`. Two predicates, both load-bearing: **not in use** (deck-held or mid-hydration stays) and **recoverable** (only evict a library track whose file is still on disk) — the second matters because the registry also holds transfusion children, captures and chops that exist NOWHERE else. **Default-off because the 1 Hz sweep races ingest:** the scanner registers a decoded track as the hand-off to the analysis worker, and the reap was evicting it before analysis read it ("Hydrated registry for X" then "released 1 sample, 121 MB"). Needs an analysis-complete signal (the conductor cannot see `AnalysisWorker::processed_ids`) and a pressure/LRU trigger rather than a fixed sweep. Note also that eviction alone does not free — RCU retires the map, so `drain_garbage()` must follow | M |
| 2.2c | ~~**Scanner dedup decoupled from residency**~~ **DONE — regression found by enabling 2.2a.** The scan deduped on registry residency, so a reclaimed track caused the next 10-second sweep to re-decode the whole file and the following reap to release it again: a decode/evict treadmill on the largest files (observed: the same 121 MB track released twice in one minute). `FolderMonitor` now carries a session-scoped `scanned` set, preserving prior behaviour exactly (a content change at a known path is still only picked up on next start). The comment above that guard records an earlier incident where re-decoding "exhausted RAM and froze the app" — this reopened it | S |
| 2.2b | **LRU eviction of deck-loaded samples — BLOCKED on an RT-safety prerequisite the original row missed.** `SamplerProcessor` holds `Arc<Vec<f32>>` and drops it on `AddSource` (sampler.rs:382); the comment at :376 says outright that this is safe *because* "the registry retains the buffers". The unbounded registry is therefore currently load-bearing for RT safety — evict a loaded sample and that drop becomes a multi-hundred-MB `free()` on the SCHED_FIFO thread. Needs a buffer garbage-return channel first: `apply_topology_mutation` takes no `ProcessContext`, and `GarbageProducer` only accepts `Box<dyn AudioProcessor>` | M |
| 2.3 | **Streaming against plain files** — disk thread → SPSC ring → RT consumer. Rewrite `StreamingManager` (it downmixes to mono, pushes one sample at a time with 2 ms sleeps, and is bound to a node type never instantiated) and actually wire it | L |
| 2.4 | **mmap session cache**, keyed by content hash, f32, kernel-evicted | M |
| 2.5 | **Reference-based edit list.** A clip is `(sample_id, start, end, gain, fades)` over an immutable source; render only on bounce | L |
| 2.6 | **Fix `checkpoint()`** — it `Arc`-clones the *entire registry* per undo step, 50 deep. With materialising edits that's ~6.3 GB for 50 edits of one long track. 2.5 makes undo store descriptors instead of buffers | S *(after 2.5)* |

**Gate 2:** first sound **< 10 ms** for a 6-minute track (from 681 ms) · 4 decks under **100 MB** of audio residency (from 507 MB) · library scan of 500 tracks with bounded RAM · Gate 1 still green.

---

## Phase 3 — Identity, import pipeline, library storage

| # | Item | Size |
| :-- | :-- | :-- |
| 3.1 | **Content-addressed ids: BLAKE3 over canonical PCM, hashed *before* resampling** — so the same master as WAV and FLAC dedups. Migration path from `DefaultHasher(path)` for existing libraries and projects | M |
| 3.2 | **Binary library rows.** `rkyv` derives already exist on `SampleMetadata`; only `library.rs` chose `serde_json`. Enables zero-copy archived reads | S |
| 3.3 | **Import pipeline:** decode → hash → **measure** (true bandwidth cliff, LUFS, true-peak, DC, clip count) → analyse → archive → cache on demand | M |
| 3.4 | **Ceiling-not-target resampling:** never upsample; 176.4→88.2 and 192→96 only, always 2:1 | S |
| 3.5 | **Archive bit-exact at native rate/depth.** WavPack for float (FLAC cannot store 32-bit float), FLAC for integer, or a per-chunk codec flag | M |
| 3.6 | **Widen the scanner extension filter** to everything Symphonia handles (it's already built with `features = ["all"]`; the filter admits only wav/flac/mp3/ogg) | S |

**Gate 3:** rename a file → library row survives · same master in two containers → one id · a transcoded "320 kbps" file is correctly flagged by measured bandwidth · `get_track` flat in track length.

---

## Phase 4 — DNA schema

| # | Item | Size |
| :-- | :-- | :-- |
| 4.1 | **Descriptor validation experiment** *(do this first — it gates 4.2)*. Run candidate descriptors over real material: Reese bass, neuro bass, 909 kick, acoustic kick, pad, vocal, atmosphere. Check they move **independently** and discriminate material classes. **Now known to gate Phase 6 as well** (spike, 2026-07-29): the current `latent_space` encoding has a measured representation error of **0.026 on flat material but 0.452 on tonal material**, because `(share * 10).min(1.0)` saturates any band above 10% of the energy. That error is a hard floor on transfer quality — the 6.1 spike hit it and could not cross it by fixing DSP. **Add a concrete acceptance test to this row: a candidate encoding must round-trip a tonal spectrum (kick, bass, single-note pad) to a materially lower error than 0.452**, or Phase 6 inherits the same ceiling | M |
| 4.2 | **L1 SM (64 B):** 24 int8 decorrelated timbre (DCT of mel), dynamics, rhythm, tonal, spatial, measured-quality, header | M |
| 4.3 | **L2 MD (64 B):** the 16 named descriptors, normalized to [-1,1] at source, identical layout static and live | M |
| 4.4 | **L3 LG (~4 KB):** 16-entry timbre codebook (16 spectra × 128 mel bands) + weights/dwell, formants, envelope stats, micro-timing, onset probabilities, per-band spatial and artifact | L |
| 4.5 | **Mel spacing, quantization, top band capped at 16–20 kHz regardless of source rate** | S |
| 4.6 | **Aggregation operators + album/artist/genre records.** Mean *and variance*; histogram for BPM; circular for key; probability for onsets. Genre = centroid + covariance | M |
| 4.7 | **Retire or populate the dead 768 bits** — `harmonicity`, all of `artifacts`, most of `spatial`. Note `stereo_width` is *unknowable* today: the analyser is fed channel 0 only, so a two-channel analysis path is required | M |
| 4.8 | **Psychoacoustics — currently absent entirely.** LUFS/K-weighting, equal-loudness weighting of DNA bands. Fixes `calculate_similarity`, which weights all dimensions equally | M |
| 4.9 | **Source-filter decomposition** (LPC or cepstral). *The* theoretical spine for what transfers: the filter transfers, the source is preserved. Nothing exists today | L |
| 4.10 | **Search/index:** brute-force scan is viable to ~1 M tracks (64 MB, **~6.4 ms** exhaustive) — no ANN subsystem needed yet | S |

**Gate 4:** descriptors demonstrably discriminate material classes (4.1) · index at 1 M tracks ≤ 160 MB · full-library search ≤ 10 ms · aggregates round-trip correctly for all field types.

---

## Phase 5 — Tap bus and neurons

*5.1 is the least reversible item in the whole roadmap.*

| # | Item | Size |
| :-- | :-- | :-- |
| 5.1 | **Control bus in `audio-core`, on the audio thread.** Double-buffered `[[f32; 512]; 2]`, read N-1 / write N. `ModulationMatrix` becomes the *editor* of the routing table and never carries a value (its current path is conductor-side, ~60 Hz, measured stalling 130 ms) | L |
| 5.2 | **Two-graph separation.** Audio graph stays a compiled DAG; mod graph is a separate structure with cycles legal and no compilation | M |
| 5.3 | **Taps as `BufferId` subscriptions** — `(BufferId, descriptor, bin_range)`. No insertion, no recompile, no latency, no node consumed. **Subscription-driven: an unsubscribed tap costs zero; one FFT per tap shared by all spectral descriptors.** Note this is the first planned workload that could push a single stage past the pool's ~46 µs cost gate at LIVE block sizes — today nothing does, so the worker pool is inert at 256 samples (`ARCHITECTURE.md`, "Cost-gated parallelism"). Worth measuring the gate's behaviour as part of Gate 5 rather than assuming either outcome | M |
| 5.4 | **`fn internal_taps(&self, _out: &mut [f32]) -> usize { 0 }`** — additive defaulted trait method for probe points inside a processor | S |
| 5.5 | **Neurons as a sparse weight matrix**, `bus_next = tanh(W·bus_prev + b)`. Not an object graph — this is what lets a trained model drop in as the same structure | M |
| 5.6 | **Safety: NaN/Inf sanitiser on every bus write · per-node slew (default a few ms) · `tanh` bounding** | S |
| 5.7 | **Static DNA as constant bus sources** — "a mesh of multiple DNA" becomes a neuron whose inputs don't change | S |
| 5.8 | **Bus recorder** — 352 KB/s. Build it now even with no consumer; sessions before it exists are training data you cannot recover | S |
| 5.9 | **External control output** — OSC / Art-Net / DMX at ~60 Hz (16 × int16 = 1.9 KB/s per channel), timestamped from the existing PTP clock. No second timebase | M |
| 5.10 | **Patch persistence + patch UI.** A cyclic multi-input graph can't be drawn as the current topology tree — scope separately, it determines whether anyone can use this | L |

**Gate 5:** channel A's live descriptors audibly modulate channel B · a cyclic patch is stable and does not diverge · 16 spectral taps + sparse `W` ≤ 25% of budget at 128 frames · Gate 1 still green with the bus live.

---

## Phase 6 — Transfer, breeding, and the unwired subsystems

> **Spiked out of order 2026-07-29, and the result argues for keeping the order.**
>
> The proposal was to pull 6.1/6.4 forward as a de-risking spike, on the grounds
> that Phases 2–5 build a lot of infrastructure to carry a transfer nobody has
> heard. Reasonable question; the answer came back the other way.
>
> Transfer **is** directional — with real donor DNA the output does move toward
> the donor (16-band profile distance 0.839 → 0.637, centroid 0.64 → 2.51 against
> a donor at 7.51). It is not inert. But it covers only ~27% of the distance at
> full bias, and fixing every DSP defect found (relative instead of absolute
> comparison, conjugate mirroring, energy preservation, log-domain interpolation)
> **did not raise that ceiling** — it reached 0.612, essentially the same place,
> while making level stable and the control usable.
>
> The ceiling is not the DSP. It is the **encoding**. `latent_space[i]` is
> `(band_share * 10).min(1.0)` (`analysis_kernel.rs:247`), so **any band holding
> more than 10% of the energy saturates**. Measured distance between a source's
> true 16-band profile and the profile its `latent_space` implies:
>
> | source | encoding error | bands clamped |
> | :-- | ---: | ---: |
> | broadband noise (flat) | **0.026** | 1 / 16 |
> | harmonic stack (tonal) | **0.452** | 1 / 16 |
>
> The encoding is faithful for flat, broadband material and **catastrophically
> wrong for concentrated material** — which is to say, for bass, kicks, and
> anything tonal: most of what this instrument is for. A transfer can only ever
> aim at the implied profile, so that error is a floor no amount of DSP work
> crosses.
>
> **Therefore Phase 4 is a genuine prerequisite for Phase 6, not merely scheduled
> before it.** 4.2–4.4 (L1/L2/L3, the 16-entry timbre codebook over 128 mel bands)
> replace exactly the representation that is failing here, and 4.1 — validate the
> descriptors on real material before building on them — is the right first move.
> Do not wire transfer into the deck chain expecting it to be musical until the
> descriptors can represent a tonal spectrum.
>
> What IS worth doing now, cheaply and out of order: **6.1(a), the unconditional
> 1-second delay.** That is a plain bug, not a design question, and it will
> mislead anyone who touches this processor before then.
>
> Reproduce: `cargo run --release -p nullherz-conductor --example spike_dna_transfer`

| # | Item | Size |
| :-- | :-- | :-- |
| 6.1 | **Wire `PersonalityInheritanceProcessor`** into the deck chain, remove its `vec![]` in `process()`, and connect `set_source_personality` (currently called only from a unit test, so donor DNA is permanently zero). **Spiked 2026-07-29** (`conductor/examples/spike_dna_transfer.rs`) — ran it with real analysed DNA for the first time. Findings, in the order they will bite: **(a) a 1-second delay on ALL audio, unconditionally.** The rhythmic delay line reads `delay_buffer[write_ptr]` *before* writing the current sample, so the delay is one whole buffer, not zero — even at `rhythmic_bias = 0`. Measured first non-silent output at sample **48267** (= 48000 + pipeline latency) while `latency_samples()` claims 1024. Wire this into a deck as-is and the deck goes silent for a second and reports the wrong PDC. Fix before anything else here. **(b)** the `vec![0.0; input.len()]` at `personality_inheritance.rs:79` is a heap allocation in `process()`. **(c)** the spectral transfer has a **unit error**: it computes `latent_space[i] / current_bin_magnitude`, dividing a dimensionless energy RATIO by an absolute FFT magnitude, so the result tracks the recipient's level rather than the donor. Costs 6 dB at full bias. **(d)** it scales only bins `0..n/2` and never mirrors the conjugate half, so half the intended effect is cancelled by the unscaled mirror (there is a comment in the source wondering whether this is needed — it is) | M |
| 6.2 | **Fix the Breeder's node addressing** — `target_node_idx: 150` is a `ProcessorTypeId` used as a graph index | S |
| 6.3 | **Delete the deck DNA panel.** Four knobs writing to a metadata field no processor reads; overwrites measured analysis with fabricated values; contradicts "run it raw" | S |
| 6.4 | **Engage `DnaMorpher` with real DNA** (ships `engaged: false`, `dna_a`/`dna_b` never set) | M |
| 6.5 | **Relative spectral transfer** — donor envelope normalised by its own mean, not absolute magnitudes, so a quiet donor doesn't gate the recipient | M |
| 6.6 | **Mark transferable vs identity-preserving fields** in the schema. Recipient keeps pitch/key/tempo/level; donor lends envelope, formants, artifacts, spatial | M |
| 6.7 | **Breeding against aggregates** — breed a track toward an artist or genre centroid. Keep `transfuse_dna` / `chaotic_transfuse_dna` | M |
| 6.8 | **Iteration stability.** Iterated transfer either contracts to a fixed point (character stops developing) or expands (artifacts accumulate). Define the controlled injection and the stability condition that keeps generations productive | M |
| 6.9 | **Convolution as a node.** The partitioned-convolution primitive exists in `spectral.rs` but isn't exposed — and convolving one sound with another is a core transfusion technique | M |

**Gate 6:** donor→recipient transfer is audible and continuously controllable · iterated transfer is bounded and measurably productive across ≥8 generations · every processor in the registry is reachable (Gate 0.10 still green).

---

## Phase 7 — Container and P2P

| # | Item | Size |
| :-- | :-- | :-- |
| 7.1 | **Chunked container:** fixed 65,536-frame independently-decodable chunks, chunk table, peak mipmaps and LG DNA in-container, per-chunk codec flag | L |
| 7.2 | **Merkle tree over chunks** — per-chunk verification without trusting the peer | M |
| 7.3 | **Lossless export escape hatch** (WAV/FLAC), first-class and tested. Also the migration path when the format revises | M |
| 7.4 | **DNA gossip:** replicate SM + MD only. **Never audio without rights** — 160 B of measured descriptors is legally trivial; audio chunks are a different conversation and define how the project is perceived | L |
| 7.5 | **Peer fetch:** parallel ranges from N peers, resume from chunk boundary, verify streaming | L |

**Gate 7:** play a remote track in **< 200 ms** (chunk 0 only, vs ~5.6 s for a full fetch) · corrupt a chunk in transit and have it detected before it reaches the audio thread · every network feature still has a working single-player mode.

---

## Track U — Usability (runs in parallel from Phase 0)

*Independent of the architecture phases; touches UI files they don't.*

| # | Item | Size |
| :-- | :-- | :-- |
| U.1 | **Knob label overlap fix** — `knobs.rs:76` paints the label 4–12 px *outside* its allocated rect (`rect.center_bottom() + 4` with `Align2::CENTER_TOP`), so the next widget lands on it | S |
| U.2 | **Deck control redesign:** track-meta row (title / artist / BPM / time), then two columns — VU left, and gain/EQ/balance rotaries + **linear** volume fader right | M |
| U.3 | **Single play/pause button.** Matches the engine: `StopNode` is a *pause* that holds `play_head`; CUE is what returns to the start. Two buttons imply a stop-and-rewind that doesn't exist | S |
| U.4 | **Sidebar: unified compact padding across all tabs** | M |
| U.5 | **Library list as an accordion** — details expand on click via a details toggle; **double-click still loads to the active deck** | M |
| **U.7** | **NEW — DONE 2026-07-29: KEY LOCK (master tempo).** The gap §9 of [`MARKET_COMPARISON.md`](../business/MARKET_COMPARISON.md) records: every competitor holds pitch constant while tempo moves; Nullherz resampled, so SYNC shifted pitch turntable-style. **It needed no new processor.** Tempo sync runs the deck at rate `r = transport_bpm / track_bpm`, which transposes by `12·log2(r)`, so key lock is the existing KeySync vocoder driven to `-12·log2(r)`. `PerformanceCommand::SetDeckKeyLock` + `MixerManager::key_lock_decks`, off by default like every other latch. **KEY and KEY LOCK compose**: both drive the same vocoder in the pitch slot and their corrections ADD, so one function (`pitch_slot_state`) decides whether the vocoder belongs there and what it shifts by — handled separately, releasing one latch would remove a node the other still needs (there is a test for exactly that). A LOCK button sits beside KEY on each deck. Confirms the earlier analysis that key lock is **inherently realtime and cannot be pre-rendered**: the correction changes whenever the tempo does | M |
| U.6 | ~~**RAW mode, default on.**~~ **DONE (2026-07-28).** A load is now the source plus its tempo mode and nothing else. `Core::SetBpm`, harmonic key-sync and groove transfusion are gated on two per-deck latches (`MixerManager::sync_decks` / `key_sync_decks`, driven by `PerformanceCommand::SetDeckSync` / `SetDeckKeySync`), both off by default; `SamplerProcessor::quantize_enabled` now defaults false and the conductor states the intended mode explicitly on every load rather than trusting the constructor. SYNC and KEY are latching buttons on each deck, lit when engaged — **the SYNC button that existed before sent `SyncDecks`, which the orchestrator matched and ignored**, so the visible control did nothing while the invisible automation did everything. Releasing KEY re-centres the KeySync node at 0 semitones (merely ceasing to update it would leave the deck transposed with the light off). In RAW the library facet lookup is skipped entirely, so the load no longer takes the library mutex on the command path. Pinned by `conductor/tests/raw_mode_test.rs` (8 tests, verified to fail against the old behaviour). **Sonic change:** golden master render regenerated, L peak **−5.3 dBFS** / rms −14.8 dBFS vs the outgoing reference — decks are no longer phase-locked onto the global beat grid, so playhead positions differ; console re-measured at peak 0.5322, 0 xruns, 2.3% of block budget. *A human listen is still owed — the fixture was regenerated on reasoning and measurement, not on ears.* **Master key resolved (same day):** KEY now shifts toward the track on `active_master_deck`, not a hardcoded C. An unknowable reference — no master track, no analysed key on either side, or the deck IS the master — yields **0 semitones**, never an assumed C; that substitution was the defect, since a track nobody had key-analysed still got transposed. Because the reference is no longer constant, loading a new master track or moving master re-aligns every KEY-latched deck (`realign_key_latched_decks`), otherwise the light stays lit while the deck matches a track that is no longer the master. `active_master_deck` moved from `Conductor` to `MixerManager` so translation can read it without a second copy of console state, and `SetMasterDeck` is applied in the same pre-translation pass as the latches. 13 tests, 7 of which fail against a re-introduced hardcoded C | M |
| U.7 | **Tool decoupling: each tool owns its selection.** Sampler follows `now_playing[focused_deck]` while its VU is hardcoded to deck A. Editor is already library-driven (correct). Composer reads `now_playing[track_idx % 4]`, so a sequencer track can only be a deck's track | M |
| U.8 | **Composer: load samples into the sequencer** — clip slots referencing the registry by id, independent of decks | L |
| U.9 | **Editor: non-destructive save / save-as** over the reference-based edit list (2.5) | M |

**Gate U:** a stranger loads a track, plays it, EQs it, cues it and loops it in 15 minutes without hitting a wall.

---

## Track D — Documentation and positioning

| # | Item | When |
| :-- | :-- | :-- |
| D.1 | **License boundary decided** — permissive engine (`nullherz-traits`, `ipc-layer`, `audio-dsp`, `audio-core`), copyleft application (`conductor`, `inspector`). **Must precede the first external contributor**: relicensing later needs every contributor's consent | **Before the repo opens** |
| D.2 | **`MARKET_COMPARISON.md` corrections** — add Bitwig (per-plugin isolation is *their* headline feature; The Grid is the nearest existing cross-adaptive system), add stem separation to trends, add the missing VST3/AU/CLAP hosting row, downgrade the tags ground truth contradicts, and redefine `[V]` once the reachability gate exists | After Gate 0 |
| D.3 | **Theory formalisation** — short and rigorous, not long and vague. Source-filter spine, stability condition for iteration, perceptual units, explicit under-determination (analysis is many-to-one, so transfer is under-determined), and a time-scale hierarchy (grain / note / phrase / track) mapping onto LG / MD / SM. Authored by us and labelled as such | Alongside Phase 4 |
| D.4 | **Material-specific documentation** — basses, drums, atmospheres, vocals, neurofunk workflow. 5 of the source's 30 topics; we have none, and 4.1 depends on this knowledge | Alongside Phase 4 |
| D.5 | **Publish latency on real hardware** with a real interface, and relabel any figure measured through `snd_pcm_open("default")` — that path measures PipeWire's quantum, not your engine | After Gate 1 |
| D.6 | **Scaling measurement at 100+ nodes**, not 46 | After Phase 5 |

---

## Critical path

```
Phase 0 ──► Phase 1 ──► Phase 2 ──► Phase 3 ──► Phase 4 ──► Phase 5 ──► Phase 6 ──► Phase 7
(schema)    (RT floor)  (memory)    (identity)  (DNA)       (tap bus)   (transfer)  (P2P)
   │                                                │
   │                                          4.1 gates 4.2
   │                                       (validate the 16 first)
   └──► Track U (parallel throughout) ──────────────────────────────────────────►
   └──► Track D ────────────────────────────────────────────────────────────────►
```

**Hard dependencies:**

- 0.4 → 0.5 (sentinels before raising `MAX_NODES`)
- 2.1 → 2.4 (buffer abstraction before mmap cache)
- 2.5 → 2.6 (edit list before the undo fix)
- 3.1 → 2.4, 7.1 (content ids key the cache and the container)
- **4.1 → 4.2/4.3** (validate descriptors before locking the layout — §8.1 is the most expensive thing here to change later)
- 5.1 → everything in Phase 5 and 6
- 5.2 → 5.5 (two graphs before cyclic neurons)

**Deliberately deferred:** audio-rate cross-channel feedback · ML inference runtime (build the matrix and recorder; add inference only when a trained model beats a hand patch) · user-placeable taps at arbitrary sample offsets · custom filesystem · custom lossless codec.

---

## Progress metrics

Track these across phases; they are the honest measure of the roadmap.

| Metric | Today | Target | Phase |
| :-- | :-- | :-- | :-- |
| Time to first sound, 6-min track | 681 ms | **< 10 ms** | 2 |
| Audio residency, 4 decks | 507 MB | **< 100 MB** | 2 |
| Library residency, 500 tracks | 51.7 GB | **bounded** | 2 |
| `get_track` cost, 6-min track | 61 ms | **flat in length** | 3 |
| Survival on the reference machine, 60 min | 453–642 xruns | **0** | 1 |
| Replicated DNA per track | 302 B (32% dead) | **160 B, all live** | 4 |
| Full-library search, 1 M tracks | n/a | **< 10 ms** | 4 |
| Unreachable registered processors | 4 | **0** | 0 (gate), 6 (fixed) |
| Failing tests | 2 + 1 flake | **0** | 0 |
| Remote track first sound | n/a | **< 200 ms** | 7 |

---

## What "done" looks like

A DAW that, on a 2017 dual-core laptop: plays any format at any length with a 4 ms load, holds a thousand-track library in bounded memory, runs four decks at 48 kHz with zero xruns for an hour, lets you patch any channel's measured character into any other channel's parameters (or a light rig) with cycles that don't blow up, transfers a donor sound's timbre onto a recipient without touching its pitch, and finds any track in a million by how it sounds — with every one of those features working with zero peers, and each verifiable by a skeptical developer in an afternoon.
