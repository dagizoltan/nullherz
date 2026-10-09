# Nullherz Technical Debt & Stubs Log

**Author:** Senior Lead Audio & Rust Systems Architect
**Status:** OPEN DEBT REGISTER
**Last verified against the tree:** 2026-10-08 (`main` @ `9d4be33`)

Open technical debt, stubs and prototype logic **verified in the codebase on the
date above**. Every entry carries a file path, and where possible the command
that shows the problem.

Two rules for this file, both learned the hard way:

1. **An entry must be re-verified to stay here.** The 2026-10-08 pass found two
   entries describing code that had since been fixed (see §0). A debt register
   nobody re-reads is worse than none, because it spends engineering attention
   on problems that no longer exist.
2. **Resolved items move to §4 or leave.** They do not stay in the open list
   with a `— RESOLVED` suffix; that is how §0's two stale entries survived.

Numbers, measurements and the full issue inventory live in
[`REVERSE_ENGINEERING_SYSTEM_REPORT_2026.md`](./REVERSE_ENGINEERING_SYSTEM_REPORT_2026.md).
This file is the register; that one is the audit.

---

## 0. Corrections to the previous edition

Both of these described real defects when written. Neither is true today.

* **`StreamingManager` is wired.** The previous edition said it was *"never
  constructed or held as a field anywhere; `start_stream`/`stop_stream` have
  zero callers."* It is now constructed at
  [`orchestrator.rs:322`](../../crates/nullherz-conductor/src/orchestrator.rs:322),
  held as a field at `:73`, and `start_stream` is called from
  [`command_handler.rs:181`](../../crates/nullherz-conductor/src/command_handler.rs:181).
  The *latent teardown bug* it also described is still open and has been
  re-filed as §1.3 below — now that the manager is reachable, that bug is too.
* **`SpectralProcessor::set_ir` does not allocate on the RT thread.** The
  previous edition filed it under "Execution Plane & Real-Time Safety Gaps". Its
  only caller is
  [`command_handler.rs:371`](../../crates/nullherz-conductor/src/command_handler.rs:371),
  on the **control** thread, building a processor before a swap. It is not
  reachable from `apply_topology_mutation`. Allocating there is correct, not
  debt. The entry is withdrawn; the invariant worth keeping is *"`set_ir` must
  never acquire a caller on the RT path"*, and that belongs in a test, not here.

---

## 1. Verified Core Technical Debt & Stubs

### 1.1 Clock Synchronization & PTP Engine
- **SO_TIMESTAMPING Engine Integration**:
  - *Location*: `crates/nullherz-conductor/src/ptp_engine.rs` and `crates/nullherz-traits/src/clock.rs`.
  - *Detail*: While `PtpClockProvider` implements high-precision raw packet timestamp extraction via `recv_with_timestamp` utilizing `SO_TIMESTAMPING` and `SCM_TIMESTAMPING` (`crates/nullherz-traits/src/clock.rs`), the main synchronization loop in `ptp_engine.rs` timestamps packet arrival via the standard software clock `get_system_time_ns()`. Integrating true hardware RX timestamps directly into the engine's receipt path remains an open goal.
- **System Clock Synchronize Placeholder**:
  - *Location*: `crates/nullherz-traits/src/clock.rs` — `SystemClockProvider::synchronize_with_master`.
  - *Detail*: This function is a no-op placeholder. Standard desktop/VM runs fallback entirely to software monotonic time discipline.
- **Best-Master-Clock (BMC) Election**:
  - *Location*: `crates/nullherz-conductor/src/ptp_engine.rs` — `PtpEngine::new`.
  - *Detail*: Node roles (master vs. slave) are hardcoded as configuration/constructor flags. There is no dynamic Best-Master-Clock algorithm (IEEE 1588 BMC) to automatically elect the highest-quality clock on the subnet.

### 1.2 WASM Sidecar Zero-Copy SHM Mapping — RESOLVED
- **Zero-Copy SHM Guest Mapping**:
  - *Location*: `crates/fx-runtime/src/wasm_runtime.rs`.
  - *Detail*: Fully implemented. Host functions in `wasm_runtime.rs` perform direct pointer mapping and slice operations into guest linear memory (`mem.data_mut(&mut caller)`), eliminating intermediate heap/stack allocations during SHM command and audio block serialization/deserialization.

### 1.3 Execution Plane & Real-Time Safety Gaps
- **Spectral Domain Arbitrary Block Sizes**:
  - *Location*: `crates/nullherz-processors/src/spectral.rs`.
  - *Detail*: The spectral processing kernels are verified to support block sizes of power-of-two ≤ 1024. Arbitrary, non-power-of-two hardware buffer blocks require further buffer padding and overlap-add buffering wrappers to prevent filter leakage or slice overflows.
- **Spectral `set_ir` Allocation on RT Thread**:
  - *Location*: `crates/audio-dsp/src/spectral.rs` (approx. line 231).
  - *Detail*: The partition buffer allocations and FFT calculations are performed inside `apply_topology_mutation`. Although tolerable for short impulse responses, this should be pre-partitioned and packaged as a ready-made mutation payload on the Conductor side to completely shield the RT thread.
- **Retired Sample Buffer Drops**:
  - *Location*: `crates/audio-core/src/engine/resource_recycler.rs`.
  - *Detail*: When a sample buffer is replaced on a deck, the original `Arc<Vec<f32>>` is dropped on the RT thread if the sample registry does not retain a copy. While standard practice retains samples in the registry (reducing drop to a simple atomic decrement), a secondary lock-free garbage collection ring should be introduced to defer all buffer deallocations off-thread.
- **Threaded Audio Backend Xrun Blindness**:
  - *Location*: `crates/nullherz-backends/src/threaded.rs`.
  - *Detail*: The software fallback Threaded backend clocks callbacks using an interval sleep loop. It cannot programmatically detect or log hardware-level underruns (xruns) under adversarial scheduler loads, unlike the ALSA or PipeWire backends.
- **Synchronous Device Enumeration on the Conductor Tick — RESOLVED (2026-10-08)**:
  - *Location*: `crates/nullherz-conductor/src/orchestrator.rs` (`refresh_audio_devices`, `scan_audio_devices`); `crates/nullherz-conductor/src/backend.rs` (`BackendManager::active_type`); `crates/nullherz-backends/src/alsa.rs` (`ALSA_LIB`).
  - *Detail*: `Conductor::tick()` enumerated audio devices inline. Caching the RESULT on a 5-second timer (the earlier fix, for the per-telemetry-frame version of the same bug — its history is in the header of `tests/telemetry_hot_path_test.rs`) left the scan itself synchronous on the tick thread — the thread that feeds the RT command ring. `snd_device_name_hint` walks ALSA's entire config tree from disk on **every** call: measured at **15.0 ms** here (31 hints) and 74.6 ms on a machine with more cards. One tick in roughly every 860 therefore cost 10.8 ms against a 5.8 ms audio-block budget, delaying every command queued behind it — `Play` included — and starving that tick's telemetry. Caught by `tests/long_track_control_path_test.rs::test_long_track_does_not_stall_the_control_path`.
  - *Fix*: Enumeration runs on a named `device-scan` background thread and publishes into `cached_audio_devices` through an `mpsc` channel that `tick()` drains with `try_iter()` — the same shape as async track hydration, per AGENTS.md §1. An `AtomicBool` slot (cleared through a `Drop` guard, so a panic inside libasound cannot wedge it) keeps one scan in flight at a time; the list is seeded with `default` so the UI picker is never empty while the first scan runs. Because `Box<dyn AudioBackend>` cannot be shared with a thread, the scan asks a fresh backend of `BackendManager::active_type` — sound because every backend's `enumerate_devices` is a stateless driver query.
  - *Measured correction to the original diagnosis*: the `dlopen` + ~40 `dlsym` in `AlsaLib::load()` was **not** the per-call cost. Measured standalone: 326 µs cold, ~1 µs warm for `dlopen` (the loader keeps the library mapped and refcounted), ~15 µs for the symbol resolution — against 15.0 ms for the hint walk on every round. `AlsaLib` is now cached in a `OnceLock` (`ALSA_LIB`) regardless, which removes the redundant work for every caller and the 326 µs cold cost, but it does **not** move the tick budget on its own; verified by measurement (11.2 ms with that change alone). The async hand-off is what fixes it, and it is the robust fix in any case: it removes an unbounded foreign-library call from a latency-critical path rather than making it merely faster.
  - *Non-vacuity*: `tests/telemetry_hot_path_test.rs` now counts `Conductor::device_scans_started()` rather than calls through the injected live backend — which the scan no longer touches, so the old counting assertions would have passed at zero forever. `test_the_background_scan_publishes_what_it_found` covers the new failure mode (a scan whose answer never arrives looks exactly like a fast tick) and was confirmed to fail when the channel drain is removed.

### 1.4 Unwired Processor: Delay — **RESOLVED**
- **`DelayFactory` registered** at `crates/nullherz-processors/src/registry.rs:51` (verified 2026-07-28). It is reachable through `create_by_id`/`create_by_name`, and is declared in `known_unreachable()` as "available for FX chains; not in the default master chain" — a deliberate state, tracked by the reachability gate, rather than an accident.

### 1.5 Unwired Subsystem: Disk Streaming — **RESOLVED (STEREO UPGRADE)**
- **`StreamingManager` & `StreamingSamplerProcessor` upgraded to stereo**: Interleaved stereo sample pairs ($L_i, R_i$) are decoded and pushed to the shared-memory ring buffer, and `StreamingSamplerProcessor` extracts and routes stereo Left/Right outputs.
- *Original finding, retained for the liveness bug:*
  - *Location*: `crates/nullherz-conductor/src/streaming_manager.rs` (`StreamingManager`, `start_stream`/`stop_stream`); `crates/nullherz-processors/src/streaming_sampler.rs` (`StreamingSamplerProcessor`); `crates/nullherz-processors/src/registry.rs` (`StreamingSamplerFactory` registered).
  - *Detail*: The RT consumer `StreamingSamplerProcessor` is registered (reachable via `StreamingSamplerFactory`) and correctly outputs silence on ring-buffer underrun (no block/panic). But `StreamingManager` — the disk decoder + feeder that fills that ring — is **never constructed or held as a field anywhere**; `start_stream`/`stop_stream` have zero callers. So a `StreamingSampler` node has a ring nothing ever fills → it produces silence. The subsystem is half-wired dead code (cf. the Delay processor above).
  - *Latent bug (only if wired)*: both feeder/decoder threads stop via `Arc::strong_count(&ring) <= 1`, but `StreamingManager::start_stream` also inserts an `Arc` clone into `self.streams` (line 31). While that entry lives, the count can never reach 1, so the per-stream threads would **not terminate when the consumer releases its ring** — they'd run (feeder sleep-spinning on a full ring) until `stop_stream()` clears the entire map. Fix when wiring it: track streams so the liveness check excludes the registry's own `Arc` (e.g. compare against a known baseline count, or add explicit per-stream teardown), and set the feeder thread's priority to match its "high-priority" comment (today it is a plain `thread::spawn` at default priority).

### 1.6 User Interface (UI) Micro-Frictions & Placeholders
- **Session Restoration Integration — RESOLVED**:
  - *Location*: `crates/nullherz-inspector/src/views/settings/preferences.rs` and `main.rs`.
  - *Detail*: Fully integrated. When enabled (`restore_last_session = true`), startup state restoration automatically reloads `autosave.json` via `Conductor::load_project` and restores active preferences, views, shortcuts, and custom theme colors.
- **Velocity Drag Sensitivity & Tooltips — RESOLVED**:
  - *Location*: `crates/nullherz-inspector/src/views/composer.rs`.
  - *Detail*: Smoothed step velocity dragging sensitivity (`0.005`) for high-DPI mouse precision and added step hover tooltips (`STEP N: VELOCITY XX%`).
- **Detached Visual Window 60 Hz Smoothing — RESOLVED**:
  - *Location*: `crates/nullherz-inspector/src/main.rs`.
  - *Detail*: Locked detached viewports and main window rendering cadence to 16ms (60 Hz) when `has_detached` is true.
- **TAU Constant Approximation Warning & Inspector Lints — RESOLVED**:
  - *Location*: `crates/nullherz-inspector/src/state.rs`.
  - *Detail*: Cleaned up float approximation of TAU constant in `ImageTextureEngine` with `std::f32::consts::TAU`. System workspace now compiles 100% warning-free under `RUSTFLAGS="-D warnings" cargo check --workspace --all-targets`.
- **System Mixer Input Source Signal Badges**:
  - *Location*: `crates/nullherz-inspector/src/views/mixer.rs`.
  - *Detail*: Channel input selector dropdowns in System Mixer lack live green signal presence indicators.
- **Organism Editor Macro Sliders**:
  - *Location*: `crates/nullherz-inspector/src/views/organism_editor.rs`.
  - *Detail*: 64-D genome weights require high-level macro sliders (Morphology, Chaos, Reactivity, Symmetry) for live performance.
- **Breeder Pipeline Telemetry**:
  - *Location*: `crates/nullherz-inspector/src/views/breeder.rs`.
  - *Detail*: The transfusion progress bar displays linear progress but lacks real-time sub-block DSP pipeline feedback metrics from the execution plane.
## 1. Open — Orchestration & Control Path

### 1.1 🔴 ALSA device enumeration runs synchronously on `tick()` — **gate-red**

* *Location:* [`orchestrator.rs:1338`](../../crates/nullherz-conductor/src/orchestrator.rs:1338)
  (`refresh_audio_devices`), [`backends/src/alsa.rs:642`](../../crates/nullherz-backends/src/alsa.rs:642)
  (`enumerate_devices`).
* *Detail:* When the active backend reports no devices — true before a backend is
  attached, and whenever the selected backend enumerates empty —
  `refresh_audio_devices()` falls through to
  `AlsaBackend::new().enumerate_devices()`. Measured at **9.1–10.0 ms per call**,
  on *every* call: `AlsaLib::load()` re-`dlopen`s `libasound.so.2` and
  re-resolves ~40 symbols each time, then `snd_device_name_hint(-1, "pcm", …)`
  walks the whole ALSA configuration tree from disk. Nothing is cached between
  calls. The 5-second result cache above it limits how *often* this happens, not
  how long it blocks.
* *Why it is P0:* it breaks the gate.
  `cargo test --release -p nullherz-conductor --test long_track_control_path_test`
  fails 5/5 at 10.5–11.0 ms against a 5.805 ms budget. It breaches `AGENTS.md`
  §1 — no blocking work inline on the conductor command path — and delays every
  queued command, `Play` included, by up to 10 ms.
* *Not* an audio dropout: `refresh_audio_devices` takes no engine lock. The
  failing test's message blames the engine lock; that mechanism is not what is
  happening. Correct the message when fixing.
* *Fix:* (a) `OnceLock` the `AlsaLib` handle — three lines, helps every caller;
  (b) move enumeration to a background thread publishing into
  `cached_audio_devices`, the pattern async track hydration already uses in this
  same file. (b) is what makes the budget robust rather than merely faster.

### 1.2 PTP: hardware RX timestamps not in the engine arrival path

* *Location:* [`ptp_engine.rs`](../../crates/nullherz-conductor/src/ptp_engine.rs),
  [`traits/src/clock.rs`](../../crates/nullherz-traits/src/clock.rs).
* *Detail:* `PtpClockProvider` implements raw-packet timestamp extraction via
  `recv_with_timestamp` / `SO_TIMESTAMPING` / `SCM_TIMESTAMPING`, but
  `PtpEngine`'s synchronisation loop timestamps packet arrival with the software
  clock (`get_system_time_ns()`). The hardware path exists and is unused.
* *Also open in the same subsystem:*
  `SystemClockProvider::synchronize_with_master` is a no-op placeholder, so
  desktop and VM runs fall back entirely to software monotonic discipline; and
  master/slave roles are constructor flags with **no IEEE-1588 Best-Master-Clock
  election**.

### 1.3 `StreamingManager` per-stream threads cannot terminate

* *Location:* [`streaming_manager.rs`](../../crates/nullherz-conductor/src/streaming_manager.rs)
  (`start_stream`, line ~31).
* *Detail:* Both feeder and decoder threads stop on
  `Arc::strong_count(&ring) <= 1`, but `start_stream` also inserts an `Arc` clone
  into `self.streams`. While that entry lives the count can never reach 1, so the
  threads do **not** terminate when the consumer releases its ring — the feeder
  sleep-spins on a full ring until `stop_stream()` clears the map.
* *Status change:* previously filed as "latent (only if wired)". **The manager is
  now wired** (§0), so this is live.
* *Fix:* exclude the registry's own `Arc` from the liveness check (known baseline
  count, or explicit per-stream teardown). While there: the feeder thread's
  comment says "high-priority" and it is a plain `thread::spawn` at default
  priority.

---

## 2. Open — Execution Plane & DSP

### 2.1 🟠 Reverb has no sample rate and no stereo width

* *Location:* [`algorithmic_reverb.rs`](../../crates/nullherz-processors/src/algorithmic_reverb.rs),
  [`factory.rs:443`](../../crates/nullherz-processors/src/factory.rs:443).
* *Detail — three distinct defects:*
  1. **Hardcoded to 44.1 kHz.** `comb_lengths = [1116, 1188, 1277, 1356]` and
     `allpass_lengths = [556, 441]` are literals inside `process()`. The struct
     has no `sample_rate` field, `process()` takes `_ctx` and never reads
     `transport.sample_rate`, `setup()` is not implemented, and
     `ReverbFactory::create_processor` takes `_sample_rate: f32` and discards it.
     At 48 kHz every delay is 9% short; at 96 kHz the decay is under half its
     intended length. Compare `MultiBandCompressor`, `ModulationFx`,
     `HyperNetworkEq` and `Analysis`, which all correctly read
     `ctx.transport.sample_rate` per block — this processor has no path to the
     session rate at all.
  2. **Both channels are identical.** Same delay lengths, same initial phase, no
     stereo spread (Freeverb offsets the right channel by 23 samples). A mono
     source yields perfectly correlated outputs: a stereo reverb with **zero
     stereo width**. This is the defect a listener notices first.
  3. **384 KB inline in the struct** (`2×4×8192 + 2×2×8192` f32 as fixed arrays)
     for 1356 samples of useful delay — 23× waste, and the reason the tail
     cannot be lengthened without re-architecting. RT-safe, which is the point of
     the fixed arrays, but it is L2/L3 pressure on the audio thread.
* *Also missing vs. any shipping reverb:* pre-delay, tail modulation, HF damping
  in the allpass stage, parameter smoothing on `room_size`. It is a **4+2**
  Freeverb against the canonical **8+4**, which is audibly more metallic.

### 2.2 🟠 No oversampling anywhere in the tree

* *Location:* absent from `audio-dsp` and `nullherz-processors` entirely.
* *Consequences:*
  * `LimiterProcessor` is **sample-peak only**. Its `ceiling` is an honest
    sample-peak ceiling and nothing more. It is **not** an ITU-R BS.1770
    true-peak limiter; inter-sample peaks that matter after lossy encoding are
    invisible to it. Documentation must never imply otherwise.
  * Every saturator aliases. `TapeSaturator` is `(x·drive·0.8).tanh()`,
    `TubePreamp` a Padé rational, `NeuralFilter`/`NeuralSaturator` the same Padé
    — memoryless nonlinearities at base rate, so harmonics above Nyquist fold
    back in-band. The tape saturator's "head gap filter" low-pass runs *after*
    the nonlinearity and removes nothing already folded.
* *Fix:* one 2× oversampling wrapper, usable by both families. This is a single
  primitive that unlocks true-peak limiting and non-aliasing saturation together.

### 2.3 `NeuralFilterProcessor` hardcodes 48 kHz inside `process()`

* *Location:* [`neural_filter.rs:40`](../../crates/nullherz-processors/src/neural_filter.rs:40)
  — `let sample_rate = 48000.0f32;` in the per-block path, used to derive `w0`.
* *Detail:* the cutoff lands at the wrong frequency at every rate but 48 kHz.
  Unlike §2.1 this is a one-line fix: `ctx.transport.sample_rate`, exactly as its
  sibling processors already do. (`ctx` is currently bound as `_ctx`.)

### 2.4 Limiter: single-stage, 1.2% THD on bass under limiting

* *Location:* [`limiter.rs`](../../crates/nullherz-processors/src/limiter.rs).
* *Measured:* `cargo run --release -p nullherz-processors --example probe_limiter_lookahead`
  — at the shipped 2.00 ms look-ahead, THD on a +6 dB-over 60 Hz tone is
  **−38.7 dB (1.2%)**; 997 Hz is −68.3 dB. It holds the ceiling exactly
  (overshoot 1.0000) at every setting and is transparent below threshold
  (−150 dB).
* *Detail:* one gain stage with a single exponential release. One cycle of 60 Hz
  is 16.7 ms, so a gain change inside it reshapes the waveform rather than riding
  its level. Acceptable for a performance limiter; not a mastering limiter.
  Needs program-dependent / dual-stage release, and §2.2 for true peak.

### 2.5 KeySync's bin-rounding remap is a timbral error

* *Location:* [`keysync.rs`](../../crates/nullherz-processors/src/keysync.rs).
* *Measured:* `cargo run --release -p nullherz-processors --example probe_keysync_quality`
  — worst partial suppression **−17.8 dB** at the shipped N=1024 / hop 128, with
  **7.65 dB spread between partials** on an A2/C3/E3 chord.
* *Detail:* the spread is the finding. Because partials do not sag together, **no
  makeup gain can correct it** — a scalar tuned on a sine would mis-level every
  chord. Integer bin rounding redistributes energy between partials.
* *Fix, already specified by the probe:* replace the remap with time-stretch plus
  resampling, which routes the pitch change through the −129 dB sinc resampler
  instead of bin arithmetic. Latency is 21.33 ms, which is why the processor is
  installed on demand rather than carried in the default deck chain.
* *Credit where due:* this is the best-documented defect in the tree — the probe
  proves it is wrong, explains why the cheap fix fails, and names the expensive
  one.

### 2.6 Spectral kernels assume power-of-two blocks ≤ 1024

* *Location:* [`spectral.rs`](../../crates/nullherz-processors/src/spectral.rs).
* *Detail:* arbitrary non-power-of-two hardware buffer sizes need overlap-add
  wrapping to avoid filter leakage or slice overflow. Not hit by current device
  periods; `graph/verification.rs` sweeps the *executor* over randomized
  geometry, not these kernels.

### 2.7 Retired sample buffers can drop on the RT thread

* *Location:* [`resource_recycler.rs`](../../crates/audio-core/src/engine/resource_recycler.rs).
* *Detail:* replacing a deck's sample drops the original `Arc<Vec<f32>>` on the
  RT thread if the registry does not retain a copy. Normal operation keeps it in
  the registry, reducing the drop to an atomic decrement, so this is a
  conditional violation rather than a steady-state one — which is also why the
  zero-allocation guard does not catch it. A lock-free garbage ring should defer
  all buffer deallocation off-thread.

### 2.8 Threaded fallback backend is xrun-blind

* *Location:* [`backends/src/threaded.rs`](../../crates/nullherz-backends/src/threaded.rs).
* *Detail:* clocks callbacks with an interval sleep loop and cannot detect or log
  hardware underruns under adversarial scheduler load, unlike ALSA or PipeWire.

---

## 3. Open — User Interface & Reachability

### 3.1 Pad subchannel strips are presentational — **the deck rack is FIXED**

The deck FX rack described in the previous edition is **closed** — four slots per
deck, positionally bound to `deck_<x>_fx1..fx4`, with remove, reorder and the
parameter knobs all reaching the graph. See §4 for what keeps it closed. What
the old entry called *"same pattern:
`render_sampler_subchannel_fx_item`"* is **not** the same defect and remains
open, because it cannot be fixed the same way.

* *Locations:* [`mixer.rs`](../../crates/nullherz-inspector/src/views/mixer.rs)
  — `render_sampler_subchannel_fx_item`, `render_pad_subchannel_strips`,
  `render_custom_subchannel_fx_item`.
* *Detail:* there is **nothing to point these at**. The graph holds one
  `drum_machine_node` and no per-pad strip at all, so *every* control on a pad
  subchannel — the FX rack, the fader, GAIN, PITCH, and all three EQ bands —
  writes `app.sampler.*` / `app.mixer.custom_subchannel_*` and is read by
  nothing. Verified: no `command_sender.send` anywhere in the pad strip, and no
  `sampler_pad_*` name in `MixerManager::node_names`. The deck rack was a
  **wiring** defect; this is an **absent subsystem** wearing a finished UI.
* *Why it is not a wiring fix:* 16 pads at the deck strip's shape is 16 nodes for
  gain alone and 64 for four insert slots each, against **57 nodes of headroom**
  after the deck racks (71 of `MAX_NODES` = 128, per `graph_budget`). It needs a
  design decision — a shared pad bus, a smaller slot count, or a sub-mixer the
  pads render into — before any of it can be wired.
* *Interim:* the remove and reorder buttons are **disabled** rather than left
  looking live, because a `✕` that drops a label and leaves a processor running
  is worse than no `✕`. The deck rack's own controls carry no such caveat.

### 3.2 Hardcoded node-index fallback in a view — **RESOLVED**

* *Was:* `.unwrap_or(i as u32 * 4 + 2)` at `store.rs:468` and `:730`.
* *Resolved:* both fallbacks deleted; the hot-load path skips when the slot's
  name does not resolve. Worth recording what the constant actually did: for
  deck A it computed node **2**, which is deck A's `dna_slot` — so a failed FX
  lookup would have swapped a **source** insert, not an FX insert. The previous
  edition called it *"latent, not harmless"*; that was right, and the `insert`
  alias that masked it is now removed too.
* *Also found and fixed, same class:* three sites in
  [`composer.rs`](../../crates/nullherz-inspector/src/views/composer.rs) used
  `.unwrap_or(70)`. `AGENTS.md` describes sequencer ids 70–73 as logical
  sentinels that are safe for sitting **above** `MAX_NODES` — but `MAX_NODES` is
  128 and the console allocates 71 nodes, so 70 is a real graph node and those
  commands were **misdirected rather than dropped**. All three now skip. The
  `AGENTS.md` wording is stale on this point and is worth correcting there.
* *What keeps it closed:* see §4 — the gate now rejects the *form*, not just the
  literal.

### 3.3 The reachability gate cannot see `format!` node names — **RESOLVED**

* *Was:* the gate scraped views for **literal** `get_node_id("…")` arguments and
  covered dynamic lookups with the suffix list
  `["sampler","gain","filter","isolator","sequencer"]`. `fx1..fxN` was absent,
  which is exactly how §3.1(c) survived a gate built to catch the class.
* *Resolved:* the list now carries `fx1..fx4` (derived from
  `nullherz_mixer::DECK_FX_SLOT_COUNT`, so it tracks the constant rather than a
  transcribed number) plus `pitch_slot`, `dna_slot` and `stem_matrix`.
* *The deeper hole, also closed:* `test_ui_views_do_not_hardcode_node_indices`
  matched only `node_idx: <digit>` in a struct literal. It therefore could not
  see a hardcoded index that arrived through a **variable binding and
  arithmetic** — which is precisely the form §3.2 shipped. The gate now also
  rejects any `get_node_id(..)` chain ending in `unwrap_or*`, which is what found
  the three `composer.rs` sites above. The lesson generalises: a gate that
  matches a *syntax* catches one spelling of a defect; one that matches the
  *rule* catches the class.
* *Still not scraped:* `fx_rack.rs` lives at `src/`, not `src/views/`, so the
  scraper does not read it. That is by design — its names come from one
  `slot_node_name()` helper that the explicit required-names list covers — but it
  means the list is now load-bearing for the rack. Anything that invents a deck
  node name outside that helper is unguarded.

### 3.4 26 visual/organism controls with no runtime path

* *Locations:* [`organism_editor.rs`](../../crates/nullherz-inspector/src/views/organism_editor.rs)
  — 18 sliders, **0** command sends;
  [`visuals.rs`](../../crates/nullherz-inspector/src/views/visuals.rs) — 8 knobs,
  **0** command sends.
* *Detail:* these mutate in-memory state whose only outbound path is
  `std::fs::write` of JSON (`organism_editor.rs:207`,
  `visuals.rs:1072`, `visuals.rs:1350`). There is **no runtime path from an
  organism gene to a running generator or to the audio engine.** Either route
  them to the visual sidecars or remove the controls; a surface that pretends is
  worse than a smaller one.
* *Related:* `visual_preset_N.json` is written to the **process CWD**, not
  `storage/` (`visuals.rs:1350`).

### 3.5 Sidecar out-of-process path is unexercised by the shipped catalogue

* *Location:* [`store.rs:470`](../../crates/nullherz-inspector/src/views/store.rs:470)–`490`.
* *Detail:* the Store maps every descriptor id it ships
  (`"neural-saturation"`, `"algorithmic-delay"`, `"algorithmic-reverb"`, …) to an
  **in-process** `ProcessorTypeId` and issues `SwapProcessor`. Only an *unmatched*
  id falls through to `CoreCommand::HotLoadSidecar`. So the out-of-process host —
  cgroup RSS limits, `wasmtime` fuel limits, the whole crash-isolation story in
  `fx-runtime` — is reached by nothing in the catalogue, and **no test asserts
  that a sidecar crash is survived end to end.** The performance decision is
  defensible; the untested guarantee is not.

### 3.6 Smaller UI gaps

* **Input source signal badges** — channel input dropdowns in the System Mixer
  have no live signal-presence indicator
  ([`mixer.rs`](../../crates/nullherz-inspector/src/views/mixer.rs)).
* **Organism macro sliders** — 64 individual float genes are not performable
  live; needs Morphology / Chaos / Reactivity / Symmetry macros
  ([`organism_editor.rs`](../../crates/nullherz-inspector/src/views/organism_editor.rs)).
  Blocked behind §3.4: macros over a surface that reaches nothing are still
  nothing.
* **Breeder pipeline telemetry** — the transfusion progress bar is linear and
  carries no sub-block DSP feedback from the execution plane
  ([`breeder.rs`](../../crates/nullherz-inspector/src/views/breeder.rs)).

---

## 4. Debt that stays fixed (kept for context)

Items here are closed **and** have a test or a measurement that would fail if
they regressed. That is the bar for leaving the open list.

| Closed item | What keeps it closed |
| :--- | :--- |
| Deck FX rack was presentational: one engine slot under an unbounded label list, three seeded non-node inserts, remove/reorder/knobs emitting nothing (old §3.1) | 4 positional slots per deck (`DECK_FX_SLOT_COUNT`); `fx_rack.rs` owns every mutation; 10 unit tests incl. `four_loads_occupy_four_distinct_nodes`, `remove_returns_the_node_to_bypass`, `reorder_moves_the_processors`, `unresolved_slot_sends_no_command_and_stores_nothing`; `test_every_fx_slot_is_named_and_starts_empty` asserts all four boot as `BYPASS` and that the `deck_<x>_insert` alias stays retired |
| `get_node_id(..).unwrap_or(<index>)` in views — a hardcoded graph index behind a variable binding (old §3.2) | `scan_defaulted_lookups` in `reachability_gate_test.rs` rejects the form, not just the literal; it is what found the three `composer.rs` `.unwrap_or(70)` sites |
| The gate could not see `format!`-built node names, so `fx<n>` went unguarded (old §3.3) | required-names list carries `deck_<x>_fx1..fx4` derived from `DECK_FX_SLOT_COUNT`, plus `pitch_slot`/`dna_slot`/`stem_matrix`; `test_ui_node_names_resolve_in_the_bootstrapped_graph` |
| `MAX_MUTATIONS` (256) was below a full console bootstrap, and the invariant guarding it compared against `MAX_NODES` alone (256 ≥ 128, green) while the cost is dominated by **edges** — the 4-deck console ran 245 mutations into a 256-entry ring that nothing drains until the first audio block, so crossing it **dropped structural mutations silently** and the console booted with a hole in it | `MAX_MUTATIONS` = 1024 with `EngineBuilder::topology_buffer_size` derived from it; invariant is `MAX_MUTATIONS >= MAX_NODES + MAX_BUFFERS`; `test_bootstrap_fits_the_mutation_budget` measures the real console against both the budget and the ring; `async_hydration_test` is the end-to-end witness |
| Deck strip EQ: three places wrote `channel_eq_*` and only one sent a command, so the knob row and the draggable curve moved the display and left the audio alone | all writes route through `send_channel_eq`, which resolves the deck's isolator by name and ramps over 128 samples |
| `GauntletRunner` had zero call sites while `AGENTS.md` §4 required it | `conformance_gauntlet.rs:50` calls it; `every_processor_survives_the_gauntlet` is green |
| Zero-allocation claim was prose | `rt_zero_allocation_test.rs` + `guard_is_installed`, which **fails if the counting allocator is absent** |
| `BiquadFactory` shipped an arbitrary lowpass `{0.1,0.2,0.1,-0.5,0.2}`, costing ~5 dB and treble at every biquad in the graph | identity default at `factory.rs:130`, with the bug named in a comment; `probe_frequency_response` measures ±0.056 dB ripple |
| Catmull-Rom resampling measured −29.0 dB THD+N at 10 kHz | 16-tap Kaiser β=14 sinc, cubic table; `resampler_quality_test.rs` (5 tests incl. `test_the_measurement_still_detects_a_bad_kernel`); measures −129 dB |
| Crossfader `curve` was an `if curve > 0.5` two-position switch | `test_curve_is_continuous_not_a_two_position_switch` |
| Every deck carried a 1024-point vocoder (28.7 ms) for a latch defaulting OFF | pitch slot is `BYPASS`, swapped by **type**; `probe_deck_latency` measures 7.4 ms |
| `TopologyCoordinator::commit()` boxed ~373 KB on the RT thread | caught and pinned by `rt_zero_allocation_test.rs` |
| Test suite **deadlocked** rather than failed (blocking `UdpSocket::recv_from` inside `tokio::spawn`) | every `scripts/verify.sh` step runs under a wall-clock `timeout`, and `run_step` captures `$?` from the command rather than from a negation |
| Offline bounce could diverge from live render via pool dispatch | `render_is_identical_serial_and_pooled`, which asserts its own precondition so it cannot go vacuous |
| Node vs. buffer index confusion | `BufferId` newtype, `BufferSlot::from_raw`/`encode_crossfade`; `node_sentinel_test.rs`, `capacity_constants_test.rs` |
| MXCSR FTZ/DAZ leaked across test threads, breaking the golden hash | `golden_render_tests.rs` applies `FpControlGuard::apply_ftz_daz()` explicitly; hash `0x5dbc9e3eb4d51f2d` |
| Track load deep-cloned sample buffers on the RT path | `SamplerProcessor` holds `Arc`; O(1) load |
| PTP assumed a fixed 1 ms path delay | four-timestamp round trip, EMA smoothing, 100 ms plausibility filter |
| `library.redb` mutex contention on analysis saves | batched single-transaction commit in `analysis_worker.rs` |
| `std::sync::Mutex` priority inversion across UI/metrics/orchestration | `parking_lot` migration; `clippy.toml` bans the std types in the execution plane |
| WASM sidecar SHM required intermediate copies | direct guest-memory pointer mapping in `fx-runtime/src/wasm_runtime.rs` |
| GUI `unwrap()` panics | 0 `.unwrap()` in `nullherz-inspector` production code (2026-10-08 count) |

---

*Re-verify this file against the tree before citing it. An entry that has not
been checked since it was written is a claim, not a record.*
