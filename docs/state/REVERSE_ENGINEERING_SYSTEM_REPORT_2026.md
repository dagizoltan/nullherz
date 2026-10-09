# Nullherz System Architecture, Sound Design & Technical Debt Master Report

**Prepared by:** Chief Sound Designer & Audio Software Rust Architect
**Status:** REVERSE-ENGINEERING AUDIT — measured, not asserted
**Audit date:** 2026-10-08
**Tree audited:** `main` @ `9d4be33`, plus an uncommitted working tree (see §8)
**System version:** Nullherz 0.1.0

> **How to read this document.** Every number below was produced by running a
> probe or a test in this tree on the audit date, and each one carries the
> command that reproduces it. Numbers without a command are not in this
> document. Where a previously published figure did not reproduce, both figures
> appear and the discrepancy is named — in both directions.
>
> Per `AGENTS.md` §4: *"Do not record an invariant as verified in a document.
> Record it as a test."* Claims here are therefore either (a) a test name, (b) a
> probe command plus its output, or (c) explicitly marked as unverified prose.

---

## 1. Verdict

**The engine is the asset. The effects and the surface above them are not yet at
the engine's level.**

Three things are, on measurement, genuinely excellent and would be difficult for
a competitor to match quickly:

1. **The sampler's resampling path.** 16-tap Kaiser-windowed sinc with cubic
   table interpolation, measuring **-129 dB THD+N and flat across frequency**
   at the pitch ratios a tempo-synced deck actually runs. 100 dB better than the
   4-point polynomial it replaced. (§4.2)
2. **Real-time discipline.** Zero heap allocation across the engine's whole
   block cycle, enforced by a counting global allocator that fails if the guard
   is not installed. `clippy.toml` bans `Mutex`/`RwLock`/`thread::spawn` in the
   execution plane. 0 `panic!` and 27 `.unwrap()` in production code across
   ~102k lines. (§4.5, §6)
3. **The verification culture.** 552 tests, a reachability gate that fails when a
   registered processor is unreachable, a bit-exactness gate between serial and
   pooled render, and an executor-geometry sweep. Defect fixes carry the
   measurement that motivated them in a comment next to the code. This is rarer
   than any DSP feature in the tree.

Three things are, on measurement, below the standard the engine sets:

1. **The verification gate is RED.** `scripts/verify.sh` fails today, reproducibly
   (5/5), on a control-path budget. (§3)
2. **The deck FX rack is presentational.** The engine allocates **one** FX slot
   per deck; the UI presents an unbounded, reorderable, removable rack whose
   remove, reorder, and parameter controls emit no commands at all. (§5.1)
3. **The time-domain effects are placeholder-grade** against the resampler's
   standard: a reduced Freeverb hardcoded to 44.1 kHz with an identical impulse
   response in both channels (a stereo reverb with zero stereo width), and no
   oversampling anywhere in the tree — so no true-peak limiting and aliasing
   saturation. (§4.3, §4.4)

The strategic read: Nullherz has built a **reference-grade playback and routing
engine** and a **demo-grade effects and control surface**. The gap is not
architectural — the graph, the slot discipline, the PDC machinery and the
command path are all ready for better processors. It is a content gap, and it is
the cheapest kind to close.

---

## 2. Scale & Composition

```
cargo check --workspace --all-targets   →  clean, 0 warnings, 16.4 s
cargo test  --workspace                 →  552 passed, 0 failed, 3 ignored (128 binaries)
cargo test  --workspace --release       →  223 passed, 1 FAILED   ← see §3
```

~102,000 lines of Rust across 20 workspace crates and 25 sidecar binaries.
No `TODO`, `FIXME`, `todo!()` or `unimplemented!()` markers anywhere in the
workspace — debt is carried in prose comments and in this document, not in
markers, which is a deliberate convention and the reason this file exists.

| Crate | Files | Lines | Plane / role |
| :--- | ---: | ---: | :--- |
| `nullherz-inspector` | 45 | 23,295 | UI — egui console, 23 views, multi-window |
| `nullherz-conductor` | 74 | 20,615 | Orchestration — tick, commands, library, analysis |
| `nullherz-processors` | 69 | 14,861 | Execution — 44 processor types + factories |
| `audio-dsp` | 26 | 8,840 | Execution — kernels (filters, sinc, FFT, rhythm) |
| `audio-core` | 29 | 5,989 | Execution — engine, graph, executor, worker pool |
| `nullherz-traits` | 18 | 4,607 | Protocol — command/telemetry schema, test_kit |
| `nullherz-dna` | 15 | 3,868 | SoundDNA library, transfusion, asset db |
| `sidecar-sdk` | 4 | 3,163 | Extensibility — SHM host + store processors |
| `ipc-layer` | 8 | 2,924 | Protocol — SPSC/MPSC rings, SHM, RT hardening |
| `nullherz-backends` | 10 | 2,398 | ALSA / PipeWire / JACK / CoreAudio / Mock |
| `nullherz-topology` | 3 | 2,243 | Off-thread graph compiler (Kahn + PDC) |
| `nullherz-ui-hal` | 10 | 1,586 | UI hardware abstraction |
| `nullherz-mixer` | 6 | 982 | Declarative console/deck/bus construction |
| `fx-runtime` | 4 | 739 | Out-of-process + WASM sidecar host |
| *remaining 6 crates* | 12 | 861 | bench, gateway, setup, control-plane, macros, xtask |

**Backends are all real**, hand-rolled `dlopen` FFI with no binding crates:
ALSA 856 lines (incl. MMAP, no-period-wakeup, D-Bus device reservation),
PipeWire 429, CoreAudio 241, JACK 157, Mock 52.

> **Note:** `storage/system_config.json` currently selects `"audio_backend":
> "Mock"`. Every measurement in §4 is therefore an engine measurement, not a
> device measurement. Nothing in this report characterises real hardware I/O.

---

## 3. 🔴 Gate status: RED

### 3.1 The failure

```bash
cargo test --release -p nullherz-conductor --test long_track_control_path_test
```

```
test test_long_track_does_not_stall_the_control_path ... FAILED
tick() took 10.81ms, over the 5.805ms audio-block budget
```

Reproduced **5/5** at 10.50–11.05 ms. This is not scheduler jitter: the spread
is 0.5 ms around a 1.85× overrun. `worst_telemetry` passes; the breach is
isolated to `Conductor::tick()`.

### 3.2 Root cause — measured

`Conductor::tick()` calls `refresh_audio_devices()`
([orchestrator.rs:1338](../../crates/nullherz-conductor/src/orchestrator.rs:1338)).
When the active backend reports no devices — which is the case before a backend
is attached, and whenever the selected backend enumerates empty — it falls
through to `AlsaBackend::new().enumerate_devices()`.

Measured cost of that call on this machine:

```
MockBackend::enumerate_devices   → 2 devices in 0.0008 ms
AlsaBackend::enumerate_devices   → 25 devices in 9.1–10.0 ms   (5 consecutive calls)
```

**9–10 ms, every call, not just the first.** `AlsaLib::load()` re-`dlopen`s
`libasound.so.2` and re-resolves ~40 symbols on each invocation, then
`snd_device_name_hint(-1, "pcm", …)` walks the entire ALSA configuration tree
from disk. Nothing is cached between calls.

`refresh_audio_devices()` already caches its *result* for 5 s — the comment
above it says enumeration "must never run per frame", so the cost was known.
What is not handled is that the **cold and the 5-secondly call both land
synchronously on the tick thread**.

### 3.3 Severity — stated precisely

The failing test's message says *"something slow is holding the engine lock"*.
That mechanism is not what is happening here: `refresh_audio_devices()` takes no
engine lock, and `sync_session_rate()` scopes its lock to two statements. **A
slow tick does not directly stall rendering.**

What it does breach is `AGENTS.md` §1: *"the conductor tick/command path is
latency-critical too (it feeds the RT command ring): no blocking work — file
decode, disk I/O — inline in a command handler."* A 10 ms tick delays every
queued command — including `Play` — by up to 10 ms, and starves telemetry for
two audio blocks. On a machine with more sound cards, or a networked ALSA
config, it is worse.

**Classification:** control-path stall, P0 (it is the gate), not an audio
dropout.

### 3.4 Fix

Two independent changes, either of which clears the budget:

1. Cache the `AlsaLib` handle in a `OnceLock` so `dlopen` + 40 `dlsym` happen
   once per process instead of once per enumeration.
2. Move enumeration off the tick: run it on a background thread and publish into
   `cached_audio_devices` through a channel, the same pattern async track
   hydration already uses in this file.

(2) is the one that makes the budget robust rather than merely faster, because
it removes an unbounded foreign-library call from a latency-critical path
instead of shrinking it. Recommend both: (1) is three lines and helps every
other caller.

---

## 4. Audio architect audit: measured signal performance

### 4.1 The signal path, as built

`MixerManager::bootstrap_4channel_mixer` (`create_dj_deck(deck, &[1], bus)`)
builds this per deck — **verified against
[nullherz-mixer/src/dj.rs](../../crates/nullherz-mixer/src/dj.rs), stereo end to end**:

```
SAMPLER(10) → pitch_slot BYPASS(3) → dna_slot BYPASS(3) → GAIN(2)
            → BIQUAD(1) filter → STEREO_UTILITY → fx_slot BIQUAD(1)
            → DJ_ISOLATOR(120) → per-deck buffers
                                      ↓
            bus SUMMING(30) → CROSSFADER(20) → master sum
                            → MASTERING_EQ(220) → LIMITER(200) → out
```

Two design decisions worth recording, both correct and both non-obvious:

* **The two source slots start as `BYPASS` and are swapped by *type*, not
  engaged by parameter.** KeySync is a 1024-point phase vocoder; carrying it
  unconditionally cost every deck 21.3 ms of latency to serve a latch that
  defaults OFF. Making "engaged" a type change is also what keeps PDC honest —
  the swap triggers the commit, and the commit is where `sync_node_latencies`
  re-reads the chain.
* **`BiquadFactory` defaults to identity** (`b0=1`, rest 0), with a comment
  naming the arbitrary lowpass it replaced, which had been costing ~5 dB and
  treble at *every* biquad in the graph. The measured ±0.056 dB flatness in §4.3
  is the evidence that this is still true.

### 4.2 ✅ Resampler — reference grade, and **better than documented**

```bash
cargo run --release -p nullherz-conductor --example probe_resampler_quality
```

Analyser floor **-153.2 dB** (7-term Blackman-Harris, FFT 16384, f32).

THD+N of the **shipped default** (`InterpolationType::Sinc`), at the rates a
tempo-synced deck actually runs (`>512` distinct fractional phases):

| source | +1 semitone | +2.5% tempo | +0.8% nudge | −4 semitones |
| :--- | ---: | ---: | ---: | ---: |
| 997 Hz | −130.1 dB | **−129.4 dB** | −130.0 dB | −135.3 dB |
| 5 kHz | −130.0 dB | **−128.9 dB** | −129.8 dB | −139.1 dB |
| 10 kHz | −129.2 dB | **−129.2 dB** | −129.0 dB | −135.9 dB |

**Flat across frequency within 1 dB** — that flatness is the property being
bought, not the peak number. For contrast, the Catmull-Rom kernel it replaced,
measured in the same run: −91.8 dB at 997 Hz, −48.9 dB at 5 kHz, **−29.0 dB at
10 kHz**. The improvement at 10 kHz is **100 dB**.

> **Correction to the previous edition of this report.** It published
> "−92.8 dB, 16-tap sinc" and "Resampler Fidelity (10 kHz) 0.0023%". That is the
> *superseded* `16t, β=9.0, linear-table` configuration recorded in the tap sweep
> in [resample.rs](../../crates/audio-dsp/src/resample.rs). The shipped kernel is
> `16t, β=14, cubic table` and measures **36 dB better** than was claimed. The
> documentation was understating the system.

**Open limitation — decimation fold on pitch-up.** The kernel does scale its
anti-alias cutoff by the stretch (`h_s(u) = (1/s)·h(u/s)`), but 16 taps give a
finite transition width, so a near-Nyquist source pitched up leaves residue:

| rate | 16 kHz source should land at | loudest bin lands at | level vs source |
| ---: | ---: | ---: | ---: |
| 1.2 | 19 200 Hz | 19 201 Hz | −2.5 dB (ok) |
| 1.6 | 25 600 Hz | 22 400 Hz | **−17.7 dB (fold)** |
| 1.8 | 28 800 Hz | 19 201 Hz | −34.5 dB (fold) |
| 2.0 | 32 000 Hz | 15 999 Hz | **−63.0 dB (fold)** |
| 2.2 | 35 200 Hz | 12 800 Hz | −136.1 dB (gone) |

> **Correction.** The previous edition published "Resampler Alias Suppression
> −90.9 dB at rate 2.0". Live measurement is **−63.0 dB** — 28 dB worse than
> claimed. The honest statement: the low-pass works (the tone is attenuated
> rather than passed at full level, and is gone by rate 2.2), but a 1.5–2×
> pitch-up of content with real energy near 16 kHz leaves an audible fold.
> On music, where 16 kHz energy typically sits 40 dB below peak, the consequence
> is small. It is still a real limitation and the published figure is not
> reproducible.

**Cost.** 296 ns/sample at +2.5% vs 19.7 ns for Catmull-Rom — 15×. At 32 voices
that is 2427 µs, **45.5% of a 256-frame budget**. This is the single most
expensive node in the system and the voice count ceiling is set by it.

### 4.3 Console transparency and response

```bash
cargo run --release -p nullherz-conductor --example probe_signal_quality
cargo run --release -p nullherz-conductor --example probe_frequency_response
```

| Metric | Measured |
| :--- | :--- |
| THD+N, 997 Hz @ −20 dBFS, deck A, all unity | **0.0000049% (−146.1 dB)** |
| THD+N, 997 Hz @ −40 dBFS | 0.0000050% (−146.0 dB) |
| Response ripple about the mean, 40 Hz – 16 kHz | **±0.056 dB** |
| Response **mean level** | **−3.041 dB** |

**The −3.04 dB is correct, and the previous edition of this report hid it.** It
published "Frequency Response Flatness ±0.056 dB" — the ripple — and omitted
the offset, which invites the reader to conclude the path is unity. It is not.
`Crossfader::new()` is `position: 0.5, curve: 1.0` — **constant power**, which
gives `1/√2 = −3.01 dB` per side at centre
([audio-dsp/src/lib.rs:141](../../crates/audio-dsp/src/lib.rs:141), pinned by
`test_curve_endpoints_are_linear_and_constant_power`). A constant-power
crossfader at centre is the industry-standard DJ law, so **a lone deck being
3 dB down at centre is by design.**

Recording it matters because a reader comparing a bounce against its source will
find 3 dB and have no way to know whether it is a bug. It is not. The curve is a
continuous control (`test_curve_is_continuous_not_a_two_position_switch`); set
it to 0.0 for the linear law (−6.02 dB per side).

### 4.4 ⚠️ Effects audit — the weak half of the system

#### Reverb — three defects, one of them audible on every use

[`AlgorithmicReverbProcessor`](../../crates/nullherz-processors/src/algorithmic_reverb.rs)
is a reduced Freeverb: **4 combs + 2 allpass** per channel against Freeverb's
canonical 8 + 4, which gives a measurably more metallic, fluttery tail.

1. **No sample-rate awareness at all.** `comb_lengths = [1116, 1188, 1277,
   1356]` and `allpass_lengths = [556, 441]` are the 44.1 kHz Freeverb tunings,
   written as literals in `process()`. The struct has no `sample_rate` field;
   `process()` takes `_ctx` and never reads `transport.sample_rate`; `setup()`
   is not implemented; and `ReverbFactory::create_processor` takes
   `_sample_rate: f32` and discards it. **At 48 kHz every delay is 9% short; at
   96 kHz the decay is less than half its intended length.** Unlike
   `MultiBandCompressor`, `ModulationFx`, `HyperNetworkEq` and `Analysis` — all
   of which correctly read `ctx.transport.sample_rate` per block — this one has
   no path to the session rate.
2. **Both channels are identical.** Same delay lengths, same initial phase, no
   stereo spread (Freeverb offsets the right channel by 23 samples). Feed a mono
   source and the two outputs are **perfectly correlated**: a stereo reverb that
   produces zero stereo width. This is the one a listener notices first.
3. **384 KB inline in the struct.** `2 × 4 × 8192 + 2 × 2 × 8192` f32 =
   393,216 bytes as fixed arrays, so `Self::new()` materialises it on the stack
   before boxing. It is RT-safe (that is the point of the fixed arrays) but it
   is 384 KB of L2/L3 pressure for 1356 samples of useful delay — a 23× waste,
   and the reason the tail cannot be lengthened without re-architecting.

Also missing versus any shipping reverb: pre-delay, tail modulation, HF damping
in the allpass stage, and parameter smoothing on `room_size`.

#### No oversampling anywhere in the tree

A full-tree search for oversampling, upsampling, or polyphase anti-alias filters
in `audio-dsp` and `nullherz-processors` returns **nothing**. Consequences:

* **The limiter has no true-peak detection.** `LimiterProcessor` is a
  sample-peak brickwall. Its `ceiling` is an honest sample-peak ceiling and
  nothing more; the inter-sample peaks that matter after lossy encoding are
  invisible to it. It is **not** an ITU-R BS.1770 true-peak limiter and the docs
  should never imply otherwise.
* **Every saturator aliases.** `TapeSaturator` is `(x·drive·0.8).tanh()`,
  `TubePreamp` a Padé rational, `NeuralFilter`/`NeuralSaturator` the same Padé —
  all memoryless nonlinearities at base rate. Harmonics generated above Nyquist
  fold back in-band. The tape saturator's post "head gap filter" low-pass runs
  *after* the nonlinearity, so it removes nothing that has already folded.

In exchange, the limiter's measured behaviour is honest and unusually
well-characterised:

```bash
cargo run --release -p nullherz-processors --example probe_limiter_lookahead
```

| lookahead | latency | overshoot on +12 dB hit | THD 997 Hz (+6 dB over) | THD 60 Hz (+6 dB over) |
| ---: | ---: | ---: | ---: | ---: |
| **2.00 ms (shipped)** | 96 smp | 1.0000 | −68.3 dB | **−38.7 dB** |
| 0.50 ms | 24 smp | 1.0000 | −64.0 dB | −33.6 dB |
| 0.10 ms | 5 smp | 1.0000 | −58.3 dB | −33.0 dB |

It holds the ceiling at every setting, and below threshold it is transparent
(−150.0 dB). The 60 Hz column is the telling one: one cycle of 60 Hz is 16.7 ms,
so a gain change inside it reshapes the waveform. **1.2% THD on bass under
6 dB of limiting** is a single-stage design with no program-dependent release.
Acceptable for a performance limiter; not a mastering limiter.

#### KeySync — a known-wrong algorithm with a named replacement

```bash
cargo run --release -p nullherz-processors --example probe_keysync_quality
```

Partial suppression on an A2/C3/E3 chord, shipped config N=1024 / hop 128:
**worst −17.8 dB**, and the per-partial spread is **7.65 dB**. The probe's own
conclusion, which I confirm: the error is **timbral, not a level error**, so no
makeup gain can fix it. Integer bin-rounding redistributes energy between
partials. The replacement route is already identified — time-stretch plus
resampling, which puts the pitch change through the −129 dB resampler instead
of through bin arithmetic. Latency 21.33 ms, which is why it is out of the
default chain.

**This is the best-documented defect in the tree.** It is wrong, the probe
proves it is wrong, the probe explains *why* a cheap fix won't work, and it names
the expensive fix that will.

### 4.5 ✅ Real-time safety — enforced, not asserted

| Guard | Where | Status |
| :--- | :--- | :--- |
| Engine block cycle allocates nothing | `audio-core/tests/rt_zero_allocation_test.rs` | green, and `guard_is_installed` fails if the counting allocator is absent |
| No processor allocates on a steady block | `conformance_gauntlet.rs::no_processor_allocates_on_the_audio_path` | green |
| Every processor survives NaN + buffer-size oscillation | `conformance_gauntlet.rs::every_processor_survives_the_gauntlet` | green — `GauntletRunner` **is** called now (`conformance_gauntlet.rs:50`); the `AGENTS.md` §4 warning about zero call sites is resolved |
| Offline bounce == live render, bit for bit | `audio-core` `graph/mod.rs::render_is_identical_serial_and_pooled` | green, asserts its own precondition |
| Executor slicing over randomized `(num_samples, offset)` | `graph/verification.rs` | green |
| Registered processors are reachable or declared with a reason | `reachability_gate_test.rs` | green |
| `Mutex`/`RwLock`/`thread::spawn`/`sleep` banned | `clippy.toml` + `#![deny]` in `audio-core` | enforced at compile time |
| `panic = "unwind"` pinned in release | root `Cargo.toml`, with the reason | per-node `catch_unwind` fault isolation would silently become a no-op under `abort` |

Production panic surface, excluding `#[cfg(test)]` modules:

| crate | `.unwrap()` | `.expect()` | `panic!` |
| :--- | ---: | ---: | ---: |
| `nullherz-inspector` | **0** | 2 | 0 |
| `nullherz-conductor` | **0** | 2 | 0 |
| `audio-core` | 2 | 3 | 0 |
| `nullherz-backends` | 10 | 0 | 0 |
| all others | 15 | 3 | 0 |

The 54 `unwrap()`s a naive grep finds in `nullherz-conductor` are **all** inside
`#[cfg(test)]` modules. The two RT-adjacent ones are provably safe:
`executor.rs:126` is `pool.as_mut().unwrap()` inside a branch only reachable
from a `Some(_)` match arm.

### 4.6 Cost and the scaling ceiling

```bash
cargo run --release -p nullherz-conductor --example bench_console_block
cargo run --release -p nullherz-conductor --example bench_studio_scale
```

**4-deck DJ console, 256 frames @ 44.1 kHz (5805 µs budget), 20 000 blocks:**

| mean | p50 | p90 | p99 | p99.9 | max |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 118.7 µs (2.0%) | 112.2 | 134.3 | 147.1 | 176.8 | **193.2 µs (3.3%)** |

438 ns/sample. Comfortable, and the tail is tight — 1.6× the mean at max.

**Studio graphs — and this is where the ceiling is:**

| tracks | nodes | mean | p99 | p99.9 | **max** |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 4 | 12 | 93.0 µs (1.6%) | 126 | 148 | 163 µs (2.8%) |
| 8 | 20 | 173.4 µs (3.0%) | 263 | 295 | 300 µs (5.2%) |
| 16 | 36 | 321.3 µs (5.5%) | 427 | 558 | 704 µs (12.1%) |
| **32** | **72** | 320.9 µs (5.5%) | 547 | **1790** | **3574 µs (61.6%)** |
| 48 | 106 | 451.5 µs (7.8%) | 729 | 1363 | 2336 µs (40.2%) |

**The mean is not the problem and never was.** At 32 tracks the max is **11× the
mean** and eats 62% of the period, while the mean is a flat 5.5%. And 32 tracks
is *worse than 48* — the curve is non-monotonic, which a load-dependent cost
function can do and a fixed cost cannot. The suspect is the worker pool's
per-stage cost gate (`DEFAULT_PARALLEL_THRESHOLD_CYCLES = 150000`) sitting near
its decision boundary at that graph shape and thrashing between serial and
pooled dispatch block to block. `AGENTS.md` §4 warns about exactly this: *"the
worker pool's cost gate never fires at 34 nodes and fires routinely at 106, with
opposite conclusions about whether it helps."*

**Supportable claim today: 16 tracks.** Beyond that the tail, not the mean, is
the binding constraint, and it is not yet characterised with repeats on an
isolated machine.

### 4.7 Latency

```bash
cargo run --release -p nullherz-conductor --example probe_deck_latency
```

```
believed (worst path, drives PDC) :  96 samples (2.0 ms)
measured impulse arrival          : 353 samples (7.4 ms)
gap                               : 257 samples (5.4 ms)
```

The probe labels the gap `UNDECLARED`. **That label is probably wrong and should
be softened.** 257 = 256 + 1 — exactly one render block plus a sample, which is
the quantisation the probe's own closing note says to expect ("a residual gap of
a block or two is expected… a gap on the order of an FFT window is not
quantisation"). 257 is a block, not a window. There is no evidence here of a
processor failing to declare its latency; the verdict string just fires on any
nonzero gap.

Action-to-sound, from the same probe family: **7.33 ms at 256 frames, 3.33 ms at
64**. Detaching the KeySync vocoder from the default chain is what bought this —
it was 28.7 ms when every deck carried a 1024-point vocoder unconditionally.

---

## 5. Chief Sound Designer audit: what an operator can actually reach

### 5.1 🔴 The deck FX rack is a façade

> **Status since this audit:** resolved for the DECK rack — four positional
> slots per deck, every control wired. The pad-subchannel half is open and
> re-filed as debt §3.1, because it is a different defect (no per-pad nodes
> exist at all). The finding below is preserved as written, as the record of
> what the audit found.

This is the most serious product-level finding in the audit. Four independent
defects compound.

**(a) The engine has one FX slot per deck; the UI has unlimited.**
`bootstrap_4channel_mixer` calls `create_dj_deck(deck, &[1], bus)` — a **single**
`fx_ids` entry, so exactly one node per deck, registered as `deck_<x>_fx1`
([nullherz-mixer/src/lib.rs:260](../../crates/nullherz-mixer/src/lib.rs:260)). The UI
state is `deck_inserts: [Vec<String>; 16]` — unbounded, documented in its own
field comment as *"unlimited amount per channel"*.

**(b) `DeckState::default()` pre-seeds three inserts that are not nodes.**
Every deck boots with `["TRIM / GAIN", "3-BAND EQ", "PITCH / SPEED"]`
([state.rs:406](../../crates/nullherz-inspector/src/state.rs:406)). No `SwapProcessor`
was ever issued for them; they are labels. The rack renders their knobs by
**string-matching the label** (`name_upper.contains("TRIM")`) and re-pointing
them at the real gain and isolator nodes. Gain and the 3-band EQ therefore do
work. `PITCH / SPEED` does not: its knob writes `app.mixer.channel_pitch[..]`
and sends nothing.

**(c) The first *real* FX load already takes the fallback path.** Because of
those three seeded entries, `fx_slot_idx = deck_inserts[i].len()` is **3** on
the first hot-load, so the lookup is for `deck_a_fx4`, which does not exist. It
falls back to `deck_a_insert` — an alias the mixer registers for `fx_slot_ids[0]`
— so it lands on the right node by luck. Every subsequent load resolves to the
**same** alias and **silently replaces the previous FX** while the UI list grows.
The final fallback is `.unwrap_or(i as u32 * 4 + 2)`
([store.rs:468](../../crates/nullherz-inspector/src/views/store.rs:468)) — a
**hardcoded node index**, in direct violation of `AGENTS.md` §3 *"Never hardcode
a node index in a view."* It is currently unreachable because the alias always
resolves, which makes it latent, not harmless.

**(d) Remove, reorder, and the generic parameter knob emit no commands.**
In `render_channel_fx_rack_item`
([mixer.rs:160](../../crates/nullherz-inspector/src/views/mixer.rs:160)): the `✕`
button calls `deck_inserts[..].remove(fx_idx)` and nothing else — the processor
stays in the graph, audible, with the label gone. `▲`/`▼` call `.swap()` on the
label vector only. And for a hot-loaded sidecar, the only control rendered is
`render_knob_sized(ui, &mut params[0], … "MIX" …)` with **no
`command_sender.send`** — so a loaded FX runs at its default parameters forever
and its one knob does nothing.

**Net operator experience:** hot-load a reverb onto deck A and it is audible at
defaults. Hot-load a delay and the reverb vanishes with no indication. Remove
either and the audio does not change. Turn the knob and nothing happens.

The same pattern repeats for sampler subchannel inserts
(`render_sampler_subchannel_fx_item`, same file).

### 5.2 Dead controls, counted

Knobs/faders versus command sends, per view:

| view | controls | command sends | reading |
| :--- | ---: | ---: | :--- |
| `organism_editor.rs` | 18 | **0** | edits an in-memory organism; only outbound path is `std::fs::write` of JSON at line 207 |
| `visuals.rs` | 8 | **0** | writes `visual_preset_N.json` **into the process CWD**, not `storage/` |
| `settings/preferences.rs` | 6 | **0** | prefs file, by design |
| `mixer.rs` | 38 | 14 | includes the §5.1 façade |
| `channel_detail.rs` | 8 | 5 | |
| `sampler.rs` | 9 | 8 | healthiest ratio in the tree |
| `player.rs`, `library.rs`, `topology.rs`, `settings/audio.rs` | 0–1 | 7–12 | command-only views, correct |

The visual organism path is the clearest case: **26 live controls across two
views reach the audio engine and the visual sidecars through nothing at all.**
They mutate UI state and can be serialised to a file. There is no runtime path
from an organism gene to a running generator.

### 5.3 What *is* reachable

The reachability gate (`reachability_gate_test.rs`) is the authority, and it is
green. Of 44 registered processors, **11 are in the default console graph** and
**33 are declared unreachable with a stated reason** — 16 of them with the
reason *"available for FX chains"*. The declarations are
honest — most read "available for FX chains; not in default master chain", which
§5.1 shows is true in the sense that one slot per deck exists and
`SwapProcessor` reaches it, and misleading in the sense that an operator can
only ever have one at a time and cannot control its parameters.

**That is the single highest-leverage fix in this report**: allocating 4
`fx_ids` per deck and wiring remove/reorder/params to commands turns 20-odd
already-written, already-gauntlet-passing processors from "technically
reachable" into "usable", at no DSP cost.

The gate also scrapes the views for literal `get_node_id("…")` arguments so an
invented name fails the test rather than silently doing nothing. **Its blind
spot is `format!` lookups**, which is exactly how the `deck_<x>_fx<n>` defect in
§5.1(c) survives: the explicit name list covers suffixes
`["sampler","gain","filter","isolator","sequencer"]` and no `fx<n>`. Adding
`fx1..fxN` to that list would have caught it.

### 5.4 Sidecar ecosystem

Real, and better than it looks. The 24-line `sidecars/*/main.rs` files are not
stubs — they are argv shells (`cmd_shm`, `sig_shm`, in/out SHM lists, eventfd)
around processors implemented in `sidecar-sdk/src/store.rs`, hosted by
`SidecarHost`. Process isolation via cgroups with RSS limits, plus an optional
`wasmtime` path with fuel limits, both in `fx-runtime`.

**But the Store bypasses it for every sidecar that exists.**
`store.rs:470–490` matches descriptor ids (`"neural-saturation"`,
`"algorithmic-delay"`, `"algorithmic-reverb"`, …) to **in-process**
`ProcessorTypeId`s and issues `SwapProcessor`. Only an unmatched id falls
through to `CoreCommand::HotLoadSidecar`. So the out-of-process path — the
crash-isolation story — is exercised by nothing in the shipped catalogue.
That is a reasonable performance decision, but it means **the isolation
guarantee is untested in the product**, and no test asserts a sidecar crash is
survived end to end.

---

## 5. Comprehensive Issue & Technical Debt Inventory

### 5.1 Real-Time & Audio DSP Issues
1. **MXCSR Thread State Leakage in Test Harnesses [RESOLVED]**:
   - *Detail*: Tests invoking `setup_rt_thread` set FTZ/DAZ on CPU control registers. When `golden_render_is_bit_stable` ran on worker threads in `cargo test`, MXCSR state was normalized.
   - *Fix*: Updated `golden_render_tests.rs` to explicitly invoke `FpControlGuard::apply_ftz_daz()`, ensuring golden hash verification matches real-time audio thread execution state consistently (`0x5dbc9e3eb4d51f2d`).
2. **Disk Streaming Manager Stereo Upgrade [RESOLVED]**:
   - *Location*: `crates/nullherz-conductor/src/streaming_manager.rs` and `crates/nullherz-processors/src/streaming_sampler.rs`.
   - *Detail*: Upgraded `StreamingManager` and `StreamingSamplerProcessor` to support full stereo audio streaming. Interleaved stereo pairs ($L_i, R_i$) are pushed to the shared-memory ring buffer, and `StreamingSamplerProcessor` routes separate Left and Right outputs.
3. **PTP Hardware Timestamping Fallback**:
   - *Location*: `crates/nullherz-conductor/src/ptp_engine.rs` and `crates/nullherz-traits/src/clock.rs`.
   - *Detail*: `PtpClockProvider` implements raw socket `SO_TIMESTAMPING` timestamp extraction, but `PtpEngine` timestamps packet arrival via `get_system_time_ns()`. Integrating true hardware RX timestamps directly into the engine arrival path remains open.
4. **Non-Power-of-Two Spectral FFT Block Handling**:
   - *Location*: `crates/nullherz-processors/src/spectral.rs`.
   - *Detail*: Spectral FFT kernels assume power-of-two block sizes $\le 1024$. Arbitrary non-power-of-two buffer sizes require overlap-add buffering wrappers.
5. **Retired Sample Buffer Drops on RT Thread**:
   - *Location*: `crates/audio-core/src/engine/resource_recycler.rs`.
   - *Detail*: Replacing a sample buffer drops the original `Arc<Vec<f32>>` on the RT thread if not retained in the sample registry. A lock-free garbage collection ring should defer deallocations off-thread.
6. **Synchronous Audio-Device Enumeration on the Conductor Tick [RESOLVED 2026-10-08]**:
   - *Location*: `crates/nullherz-conductor/src/orchestrator.rs` (`refresh_audio_devices`, `scan_audio_devices`), `crates/nullherz-conductor/src/backend.rs` (`BackendManager::active_type`), `crates/nullherz-backends/src/alsa.rs` (`ALSA_LIB`).
   - *Detail*: This is the third incarnation of one bug — an expensive driver query on a latency-critical thread. It was first per telemetry frame (74.6 ms × 187/s), then moved to a 5-second result cache that was still **synchronous on `tick()`**, the thread that feeds the RT command ring. `snd_device_name_hint` re-walks ALSA's whole config tree from disk on every call (**15.0 ms** measured here, 31 hints), so one tick in ~860 cost 10.8 ms against the 5.8 ms audio-block budget: every command queued behind that tick, `Play` included, was delayed by it, and that tick's telemetry went uncollected. Caught by `test_long_track_does_not_stall_the_control_path`, not by review.
   - *Fix*: The scan runs on a named `device-scan` thread and publishes into `cached_audio_devices` through an `mpsc` channel drained with `try_iter()` on the tick — the async track-hydration pattern mandated by AGENTS.md §1. One scan in flight at a time via an `AtomicBool` released through a `Drop` guard; device list seeded with `default` so the picker is never empty. `AlsaLib` is additionally cached in a `OnceLock`, so the `dlopen`/`dlsym` resolution happens once per process for every caller.
   - *Measured correction to the filed diagnosis*: the `dlopen` + ~40 `dlsym` was **not** the per-call cost it was reported to be. Standalone measurement: `dlopen` 326 µs cold / ~1 µs warm, symbol resolution ~15 µs, versus 15.0 ms for the hint walk on every single round. The `OnceLock` alone left the test failing at 11.2 ms. Removing the call from the latency-critical path — not speeding it up — is what fixed the budget, and is the only version of the fix that stays fixed when the host has more cards.

### 5.2 UI/UX Micro-Frictions & Usability
1. **DAW Step Grid Velocity Sensitivity [RESOLVED]**:
   - *Location*: `crates/nullherz-inspector/src/views/composer.rs`.
   - *Detail*: Smoothed step velocity dragging sensitivity (`0.005`) for high-DPI mouse precision and added step hover tooltips (`STEP N: VELOCITY XX%`).
2. **Detached Visual Window 60 Hz Smoothing [RESOLVED]**:
   - *Location*: `crates/nullherz-inspector/src/main.rs`.
   - *Detail*: Locked detached viewports and main window rendering cadence to 16ms (60 Hz) when `has_detached` is true.
3. **Input Source Signal Badges**: Channel input selector dropdowns in System Mixer lack live green signal presence indicators.
4. **Organism Editor Parameter Grouping**: 64-D genome weights require high-level macro sliders (Morphology, Chaos, Reactivity, Symmetry) for live performance.
## 6. Issue inventory

Ordered by what it costs to leave alone. Every entry names a file and a way to
see it.

### P0 — the gate is red

| # | Issue | Location | Evidence |
| :--- | :--- | :--- | :--- |
| 1 | `tick()` 10.8 ms vs 5.8 ms budget: ALSA device enumeration (9–10 ms/call, uncached `dlopen` + full config-tree walk) runs synchronously on the control path | `orchestrator.rs:1338`, `backends/src/alsa.rs:642` | §3; 5/5 reproduction |

### P1 — a user can see it

| # | Issue | Location | Evidence |
| :--- | :--- | :--- | :--- |
| 2 | Deck FX rack: 1 engine slot vs unlimited UI; silent replacement; remove/reorder/params emit nothing | `mixer.rs:160`, `store.rs:462`, `state.rs:406`, `nullherz-mixer/src/lib.rs:284` | §5.1 |
| 3 | Hardcoded node-index fallback `i*4+2` in a view — latent `AGENTS.md` §3 violation | `store.rs:468`, `store.rs:730` | §5.1(c) |
| 4 | Reverb hardcoded to 44.1 kHz; `ReverbFactory` discards `_sample_rate`; no `transport` read, no `setup()` | `algorithmic_reverb.rs`, `factory.rs:443` | §4.4 |
| 5 | Reverb channels share delay lengths and phase → zero stereo width | `algorithmic_reverb.rs:52` | §4.4 |
| 6 | `NeuralFilterProcessor` hardcodes `sample_rate = 48000.0` **inside `process()`** — cutoff wrong at every other rate | `neural_filter.rs:40` | grep |
| 7 | 26 visual/organism controls with no runtime path to engine or sidecar | `organism_editor.rs`, `visuals.rs` | §5.2 |
| 8 | `visual_preset_N.json` written to process CWD, not `storage/` | `visuals.rs:1350` | §5.2 |

### P2 — a professional user will measure it

| # | Issue | Location | Evidence |
| :--- | :--- | :--- | :--- |
| 9 | No oversampling in the tree → limiter is sample-peak only (not true-peak); all saturators alias | `limiter.rs`, `tape_saturator.rs`, `tube_preamp.rs`, `neural_*.rs` | §4.4 |
| 10 | Studio tail non-monotonic: max 3574 µs (62% budget) at 32 tracks, better at 48 — pool cost gate suspected | `graph/executor.rs:109`, `bench_studio_scale` | §4.6 |
| 11 | Reverb is 4+2 Freeverb (vs 8+4); no pre-delay, tail modulation, or param smoothing; 384 KB struct for 1356 samples of delay | `algorithmic_reverb.rs` | §4.4 |
| 12 | KeySync bin-rounding remap is timbral (7.65 dB partial spread); needs time-stretch + resample | `keysync.rs`, `probe_keysync_quality` | §4.4 |
| 13 | Limiter 1.2% THD at 60 Hz under 6 dB limiting — single-stage, no program-dependent release | `limiter.rs` | §4.4 |
| 14 | Sidecar out-of-process path unexercised by the shipped catalogue; no end-to-end crash-survival test | `store.rs:470` | §5.4 |
| 15 | Reachability gate cannot see `format!` node names — `fx<n>` absent from the explicit list | `reachability_gate_test.rs:160` | §5.3 |

### P3 — correctness of the record

| # | Issue | Location |
| :--- | :--- | :--- |
| 16 | `probe_deck_latency` prints `UNDECLARED` for a 257-sample gap that is one render block of quantisation | `probe_deck_latency.rs` |
| 17 | `probe_resampler_quality` header still says *"`SamplerVoice` uses 4-point Lagrange (Catmull-Rom)"*; the default has been `Sinc` since the kernel landed | `probe_resampler_quality.rs:10` |
| 18 | `TECHNICAL_DEBT_AND_STUBS.md` §1.5 claims `StreamingManager` is "never constructed, zero callers" — it is wired at `orchestrator.rs:322` and called from `command_handler.rs:181` | that doc (corrected in this pass) |
| 19 | `TECHNICAL_DEBT_AND_STUBS.md` §1.3 claims `set_ir` allocates "on the RT thread" — its only caller is `command_handler.rs:371`, the control thread | that doc (corrected in this pass) |

### Carried forward, still open and still accurate

PTP hardware RX timestamps not in the engine arrival path
(`ptp_engine.rs`); `SystemClockProvider::synchronize_with_master` is a no-op;
no IEEE-1588 BMC election; spectral kernels assume power-of-two blocks ≤ 1024;
retired sample `Arc<Vec<f32>>` can drop on the RT thread
(`resource_recycler.rs`); threaded fallback backend cannot detect xruns;
`StreamingManager` per-stream threads cannot terminate while the registry holds
its own `Arc` (latent, now that the manager is wired).

---

## 7. Recommendations, in order

| # | Action | Why this order | Cost |
| ---: | :--- | :--- | :--- |
| 1 | Move ALSA enumeration off `tick()` (+ `OnceLock` the lib handle) | The gate is the thing that makes every other number trustworthy. Nothing else should be merged over a red gate. | hours |
| 2 | Allocate 4 `fx_ids` per deck; wire remove/reorder/`SetParam` to commands; drop the three seeded label-inserts; delete the `unwrap_or(i*4+2)` fallback | Turns the 16 "available for FX chains" processors from reachable into usable. Highest value-per-hour in the report. | days |
| 3 | Add `fx1..fxN` to the reachability gate's explicit name list | Makes (2) stay fixed. A gate that cannot see `format!` names will let it regress. | minutes |
| 4 | Give the reverb a sample rate: scale delays from `ctx.transport.sample_rate`, add stereo spread, go to 8+4 combs/allpasses | Cheapest audible quality win in the tree, and (1)+(2) make it reachable enough to matter | days |
| 5 | Add a 2× oversampling wrapper usable by the saturators and the limiter | Unlocks true-peak limiting and non-aliasing saturation in one primitive | week |
| 6 | Characterise the 32-track tail with repeats on an isolated machine before claiming any track count above 16 | `AGENTS.md` §4: *"take REPEATS: tail statistics on a machine without core isolation are not stable enough for a single A/B"* | days |
| 7 | Route organism genes to the visual sidecars, or remove the 26 controls | Either is better than a surface that pretends | week |
| 8 | Replace KeySync's bin remap with time-stretch + resampling | Already specified by its own probe; reuses the −129 dB kernel | weeks |

| Priority | Category | Task | Target Path | Status |
| :---: | :---: | :--- | :--- | :---: |
| **P0** | **DSP / Tests** | Explicit MXCSR FTZ/DAZ in Golden Render Harness | `crates/nullherz-processors/src/golden_render_tests.rs` | **COMPLETED** |
| **P0** | **UI / Graphics** | Decouple Detached Visual Viewport Frame Cadence (60Hz) | `crates/nullherz-inspector/src/main.rs` | **COMPLETED** |
| **P0** | **Conductor** | Move Audio-Device Enumeration Off the Latency-Critical Tick | `crates/nullherz-conductor/src/orchestrator.rs` | **COMPLETED** |
| **P1** | **UI / Waveform**| Sub-Frame Linear Playhead Interpolation | `crates/nullherz-inspector/src/views/dj_studio/waveform.rs` | **COMPLETED** |
| **P1** | **Backend** | 1-Click Exclusive ALSA Hardware Performance Mode | `crates/nullherz-inspector/src/views/settings/audio.rs` | **COMPLETED** |
| **P2** | **Conductor** | Disk Streaming Ring Teardown & Stereo Upgrade | `crates/nullherz-conductor/src/streaming_manager.rs` | **COMPLETED** |
| **P2** | **UI / DAW** | Step Grid Velocity Drag Exponential Smoothing | `crates/nullherz-inspector/src/views/composer.rs` | **COMPLETED** |
| **P2** | **UI / Organisms**| Organism 64-D Genome Macro Slider Groupings | `crates/nullherz-inspector/src/views/organism_editor.rs` | **OPEN** |
Note what is **not** on this list: anything architectural. The triple-plane
split, the slot discipline, the off-thread compiler, the PDC machinery and the
command path all survived this audit without a finding against them.

---

## 8. Audit conditions and caveats

* **Machine:** Linux 7.0.0-34-generic, x86_64, AVX2+FMA (`AudioEngine: DSP SIMD
  path = avx2+fma`). **No core isolation.** Load average during the §4.6 runs
  was 7–16 because of concurrent builds; the single-run tails there are
  therefore *upper bounds*, not stable statistics. The §3 failure was
  re-measured 5× at lower load and held, which is why it is reported as a
  regression and the tails are not.
* **Backend:** `Mock`. No measurement in this report characterises hardware I/O,
  xrun behaviour, or real device latency.
* **Working tree was not clean.** 11 files in `nullherz-inspector` carry a
  mechanical `1.0` → `1.0_f32` type-annotation change (egui `Stroke::new`
  call sites), and `storage/system_config.json` gained two `midi_ports`
  entries. Neither affects any measurement here; the workspace type-checks clean
  with them applied.
* **Not audited:** the `.clac` container, stem separation, the DNA network and
  consensus layers, the 9 visual engines, and the business/roadmap documents.
  Specifications exist for all of these in `docs/system/`; this report makes no
  claim about how much of each is implemented.

---

*Chief Sound Designer & Audio Software Rust Architect — 2026-10-08*
