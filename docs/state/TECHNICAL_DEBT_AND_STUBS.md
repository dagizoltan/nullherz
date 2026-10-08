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

### 3.1 🔴 The deck FX rack is presentational

The most serious product-level defect in the tree. Four compounding faults; see
§5.1 of the audit report for the full trace.

* *Locations:* [`mixer.rs:160`](../../crates/nullherz-inspector/src/views/mixer.rs:160)
  (`render_channel_fx_rack_item`),
  [`store.rs:462`](../../crates/nullherz-inspector/src/views/store.rs:462) and
  `:724` (hot-load),
  [`state.rs:406`](../../crates/nullherz-inspector/src/state.rs:406) (seeded
  defaults), [`nullherz-mixer/src/lib.rs:284`](../../crates/nullherz-mixer/src/lib.rs:284)
  (`create_dj_deck(deck, &[1], bus)`).
* *(a) One engine slot, unlimited UI slots.* The bootstrap passes a single
  `fx_ids` entry, so each deck gets exactly one node, named `deck_<x>_fx1`. UI
  state is `deck_inserts: [Vec<String>; 16]`, whose own field comment says
  *"unlimited amount per channel"*.
* *(b) Three seeded inserts that are not nodes.* Every deck boots with
  `["TRIM / GAIN", "3-BAND EQ", "PITCH / SPEED"]`. No `SwapProcessor` is ever
  issued for them. The rack renders their knobs by **string-matching the label**
  and re-pointing them at the real gain and isolator nodes — so gain and the
  3-band EQ work, while `PITCH / SPEED` writes `app.mixer.channel_pitch[..]` and
  sends nothing.
* *(c) The first real FX load already takes the fallback path.* Because of those
  three entries, `fx_slot_idx = deck_inserts[i].len()` is 3 on the first
  hot-load, so it looks up `deck_a_fx4`, misses, and falls back to the
  `deck_a_insert` alias for `fx_slot_ids[0]` — right node, by luck. Every
  subsequent load resolves to the **same** alias and **silently replaces the
  previous FX** while the UI list grows.
* *(d) Remove, reorder and the generic knob emit nothing.* `✕` calls
  `deck_inserts[..].remove(fx_idx)` only — the processor stays in the graph,
  audible, label gone. `▲`/`▼` `.swap()` the label vector only. A hot-loaded
  sidecar gets one `"MIX"` knob with **no `command_sender.send`**, so it runs at
  defaults forever.
* *Operator experience:* load a reverb — audible at defaults. Load a delay — the
  reverb vanishes silently. Remove either — no change in the audio. Turn the knob
  — nothing.
* *Same pattern:* `render_sampler_subchannel_fx_item`, same file.
* *Fix:* allocate 4 `fx_ids` per deck; drop the seeded label-inserts; wire
  remove → `SwapProcessor(BYPASS)`, reorder → topology edit, knobs →
  `MixerCommand::SetParam`. This turns the **16** processors declared
  *"available for FX chains"* in `known_unreachable()` from technically reachable
  into usable, at zero DSP cost — the highest value-per-hour item in the register.

### 3.2 Hardcoded node-index fallback in a view — `AGENTS.md` §3 violation

* *Location:* [`store.rs:468`](../../crates/nullherz-inspector/src/views/store.rs:468)
  and `:730` — `.unwrap_or(i as u32 * 4 + 2)`.
* *Detail:* `AGENTS.md` §3 is explicit: *"Never hardcode a node index in a view
  and never default a failed lookup to 0."* The spirit is violated whatever the
  constant: for deck A this resolves to node 2 regardless of what node 2 is.
  Currently unreachable because the `deck_<x>_insert` alias always resolves —
  **latent, not harmless.** Delete the fallback and skip the command on an
  unresolved name, which is what every other view does.

### 3.3 The reachability gate cannot see `format!` node names

* *Location:* [`reachability_gate_test.rs:160`](../../crates/nullherz-conductor/tests/reachability_gate_test.rs:160).
* *Detail:* the gate scrapes views for **literal** `get_node_id("…")` arguments
  and covers dynamic lookups with an explicit list of suffixes —
  `["sampler","gain","filter","isolator","sequencer"]`. `fx1..fxN` is absent,
  which is exactly how §3.1(c) survived a gate built to catch this class of bug.
* *Fix:* add `fx1..fxN` to the list. Minutes of work, and it is what keeps §3.1
  fixed once fixed.

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
