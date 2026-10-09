# Nullherz Technical Debt & Stubs Log

**Author:** Senior Lead Audio & Rust Systems Architect
**Status:** PRODUCTION BETA
**Date:** July 2026

This document lists the open technical debt, stubs, and prototype logic verified directly in the codebase. Identifying and cataloging these items with precise file paths allows the engineering team to address them systematically without architectural disruption.

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

### 1.7 Deck FX Rack — **RESOLVED** / Pad Subchannel Strips — **OPEN**

- **Deck FX insert rack — RESOLVED**:
  - *Location*: `crates/nullherz-mixer/src/lib.rs` (`DECK_FX_SLOT_COUNT`), `crates/nullherz-inspector/src/fx_rack.rs`, `views/mixer.rs`, `views/store.rs`, `views/library.rs`, `views/channel_detail.rs`.
  - *Detail*: The rack was `[Vec<String>; 16]` — an unbounded list of LABELS — over a graph with exactly **one** FX node per deck (`create_dj_deck(deck, &[1], bus)`). Four defects compounded:
    1. Every load past the first resolved `deck_<x>_fx<n>` for an `n` with no node, fell back to the `deck_<x>_insert` alias, and silently **replaced** the previous effect while the UI list grew.
    2. `DeckState::default()` seeded three labels (`TRIM / GAIN`, `3-BAND EQ`, `PITCH / SPEED`) that were not nodes. The renderer identified them by string-matching the label (`name_upper.contains("TRIM")`) and re-pointed their knobs at the real gain and isolator nodes — so `fx_slot_idx` started at 3 and the **first** real load already took the fallback path. `PITCH / SPEED` wrote `channel_pitch`, which nothing reads.
    3. `✕` called `Vec::remove` on the label list only: the processor stayed in the graph, still audible, with nothing left to control it. `▲`/`▼` swapped labels and left the graph untouched.
    4. The generic knob rendered with no `command_sender.send` anywhere near it, so every hot-loaded effect ran at its construction defaults for the session. `views/library.rs` was worse still — its two "LOAD TO DECK" sites pushed a label and sent **no command at all**.
  - *Resolution*: Four `BYPASS` slots per deck (`DECK_FX_SLOT_COUNT`), and the rack is now **positional** — slot `i` of deck `d` *is* node `deck_<d>_fx<i+1>`, so there is no index arithmetic to get wrong and no list that can disagree with the graph. `DeckState::deck_fx: [[Option<DeckInsert>; 4]; 16]` replaces both label vectors; trim and the 3-band isolator have their own explicit UI. Remove issues `SwapProcessor` back to `BYPASS`; reorder swaps the slots and re-installs both nodes; knobs send `MixerCommand::SetParam` with `ramp_duration_samples: 128`. The `.unwrap_or(i as u32 * 4 + 2)` fallback is deleted — on deck A it computed node 2, which is deck A's `dna_slot`.
  - *Cost*: 59 → **71** of `MAX_NODES` (128) and 96 → **120** of `MAX_BUFFERS` (240); measured with `cargo run -p nullherz-mixer --example graph_budget` and held by `test_fx_slots_fit_the_node_budget`. Twelve `BYPASS` nodes at ~0.47 µs/block each is ~5.6 µs against a 196 µs console block (`profile_console_nodes`) — a buffer copy per slot, not literally free, but under 3%.
  - *Gate*: `reachability_gate_test.rs` now requires `deck_<x>_fx1..fx4` (plus `pitch_slot`, `dna_slot`, `stem_matrix`). The missing `fx<n>` suffix is precisely how this survived a gate built to catch it: the lookup went through `format!`, so the literal scraper could not see it, and the explicit list that covers `format!` lookups did not name it.

- **`get_node_id(..).unwrap_or(..)` as a class — RESOLVED**:
  - *Location*: `crates/nullherz-conductor/tests/reachability_gate_test.rs` (`scan_defaulted_lookups`), `crates/nullherz-inspector/src/views/composer.rs`.
  - *Detail*: `test_ui_views_do_not_hardcode_node_indices` only matched `node_idx: <digit>` in a struct literal, so it could not see a hardcoded index that arrived through a variable binding and arithmetic. The gate now also rejects any `get_node_id(..)` chain ending in `unwrap_or*`. Adding that check immediately found three live instances in `composer.rs`, all `.unwrap_or(70)` — `AGENTS.md` describes sequencer ids 70–73 as logical sentinels that are safe for being **above** `MAX_NODES`, but `MAX_NODES` is 128 and the console allocates 71 nodes, so 70 is a real node and those commands were misdirected rather than dropped. All three now skip.

- **Pad subchannel strips are a mockup — OPEN**:
  - *Location*: `crates/nullherz-inspector/src/views/mixer.rs` — `render_sampler_subchannel_fx_item`, `render_pad_subchannel_strips`, `render_custom_subchannel_fx_item`.
  - *Detail*: The deck rack's fix does not transfer, because there is nothing to point these at. The graph has **one** `drum_machine_node` and no per-pad strip: every control on a pad subchannel — the FX rack, the fader, GAIN, PITCH, and the three EQ bands — writes `app.sampler.*` / `app.mixer.custom_subchannel_*` and is read by nothing. Verified: no `command_sender.send` anywhere in the pad strip, and no `sampler_pad_*` name in `MixerManager::node_names`.
  - *Why not fixed here*: 16 pads at the deck strip's shape is 16 nodes for gain alone and 64 for four insert slots each, against 57 nodes of headroom. This needs a design decision — a shared pad bus, a smaller slot count, or a sub-mixer the pads render into — not a wiring patch. The remove/reorder buttons are **disabled** in the interim so they do not imply a graph edit they cannot perform.

- **Other display-only strip controls — OPEN**:
  - `channel_balance` ("PAN") is read only by the VU meter's left/right split; the deck graph has no pan stage. Tooltipped as display-only.
  - `channel_pitch` is written by nothing now that the seeded `PITCH / SPEED` rack entry is gone, and was never read.
  - The DNA "SHAPE" checkbox (`views/dj_studio/dna.rs`) resolves `deck_<x>_dna_morph`, a name `MixerManager` **no longer registers** (the permanent DnaMorph node was replaced by the swappable `dna_slot`). It therefore does nothing. Engaging DNA should be a `SwapProcessor` on `deck_<x>_dna_slot` to `DNA_MORPH`, which is a feature decision rather than a rename; adding `dna_morph` to the gate's suffix list would turn it red today.

---

## 2. Resolved Architectural Hardenings (Kept for Context)

- **MXCSR FTZ/DAZ Test Harness State Synchronization**: Resolved thread-local floating-point control register state leakage across test runners. `golden_render_tests.rs` now explicitly applies `FpControlGuard::apply_ftz_daz()`, ensuring golden hash verification matches real-time audio thread execution state consistently (`0x5dbc9e3eb4d51f2d`).
- **O(1) Sample Deck Loading**: Resolved track-load heap clones. `SamplerProcessor` has been refactored to adopt shared `Arc` containers instead of deep-cloning sample buffers, preventing large allocations on the RT thread hot-path.
- **PTP Path-Delay Calculation**: Refactored `PtpEngine` from a fixed 1 ms assumption to an active four-timestamp round-trip measurement with EMA smoothing and a 100 ms plausibility filter.
- **Database Mutex Contention**: Migrated track analysis saves to a batched, single-transaction database commit pattern inside `AnalysisWorker` (`crates/nullherz-conductor/src/analysis_worker.rs`), reducing lock contention on `library.redb`.
- **System-Wide `parking_lot` Migration**: Replaced standard library blocking mutexes with lightweight, non-poisoning `parking_lot::Mutex` across the UI, metrics, and orchestration layers to prevent priority inversion.
