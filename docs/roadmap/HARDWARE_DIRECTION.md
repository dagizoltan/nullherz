# Hardware Direction — standalone units, mixer, controllers

> ## NOT SCHEDULED. NOT STARTED. DESKTOP FIRST.
>
> This document exists so a long-term ambition can shape near-term decisions
> **without becoming work**. Nothing here is planned, estimated, or committed.
> The current plan of record is
> [`IMPLEMENTATION_ROADMAP_2026_07.md`](./IMPLEMENTATION_ROADMAP_2026_07.md),
> and the desktop product is finished first.
>
> **Read §4 and ignore everything else.** §4 is the only part that affects work
> happening now: a short list of things not to foreclose. §1–§3 are evidence and
> gaps, recorded so that whoever picks this up later starts from measurements
> rather than from this document's enthusiasm.
>
> This repository has retired one set of documents for describing intent as
> though it were status (see [`../archive/README.md`](../archive/README.md)).
> This one is written to not join them: every capability claim below is dated and
> names the measurement behind it, and everything unproven is in §3.

## 0. The ambition

One codebase, several bodies:

- **Desktop** — the current product.
- **Standalone player** — a CDJ-like unit running the same engine, stripped, responsible for one deck and its own output.
- **Mixer** — a unit that sums, EQs and crossfades, with no library or deck.
- **Controller** — the traditional case: dumb MIDI surface, desktop does the work.

The bet is that the Triple-Plane split already separates "what a body does" from
"what the engine is", so a body is a build profile plus a topology, not a fork.

## 1. What is already true (measured 2026-07-29)

| claim | evidence |
| :--- | :--- |
| The daemon runs headless with no UI dependency | `nullherz-conductor` binary **9.7 MB** vs `nullherz-inspector` **31 MB**; no `egui`/`eframe`/`wgpu` in the conductor's manifest |
| Audio backends impose **no link-time dependency** | `nullherz-backends` depends only on `libc`; ALSA/JACK/PipeWire are `dlopen`'d at runtime and degrade if absent |
| The engine is genuinely separable from orchestration | Triple-Plane holds in the dependency graph; `bin/survival` runs the full console headless |
| Small periods are achievable on commodity audio hardware | Onboard ALC298 negotiated **period 32 / buffer 64 frames (1.33 ms)** with 0 xruns; period 64 × 3 is comfortable at 34% of budget |
| Signal-path latency is hardware-independent | 3.33 ms RAW at block 64, identical code on any target (`examples/probe_deck_latency.rs`) |
| Clock sync, discovery and remote DSP exist in some form | `ptp_engine` (UDP :319, four-timestamp round trip, PI servo), `discovery` (UDP beacon), `distributed-sidecar` (:9002, Type 5/6) |

The `dlopen` detail is the strongest single signal: a build already links against
no audio library at all.

## 2. A CDJ-style topology needs LESS than what is built

Worth stating because it points the opposite way to the existing distributed work.

`distributed-sidecar` solves **distributed DSP** — one machine rendering audio for
another, over UDP. A standalone-player topology does not need that. Real units
render their own deck to their own physical output and feed a mixer; the network
carries only:

- clock sync,
- library and track metadata,
- transport/link state.

**No network audio.** That removes the hardest reliability problem in the current
distributed design. If this direction is ever pursued, resist the temptation to
reuse the remote-DSP path because it exists.

## 3. What is NOT true yet

| gap | detail |
| :--- | :--- |
| **No build modularity** | The headless daemon pulls **244 unique crates, 128 of them the `wasmtime` subtree** — a full WASM JIT, an unconditional dependency of `fx-runtime`, which the conductor never references. Workspace feature flags today are only `test-utils`, `kani-verify`, and three optional MIDI/cpal backends. Nothing gates the WASM host, the WebSocket gateway, `redb`, or DNA gossip. **A stripped build is not currently possible.** |
| **No Best-Master-Clock election** | Master/slave are hardcoded constructor flags (`ptp_engine.rs`). Appliances must negotiate this without a config file; IEEE 1588 BMC is the standard answer. |
| **Sync uses software timestamps** | `PtpClockProvider` implements `SO_TIMESTAMPING`, but the engine's receive path timestamps with `get_system_time_ns()`. Caps precision at scheduler jitter — adequate on a laptop, marginal for beat-accurate multi-unit sync. |
| **ARM is untested** | Rust portability is real but unproven here; no cross-compilation target is exercised in CI. |
| **No round-trip latency measurement** | `calibration_samples` is a config field with nothing behind it. Comparing hardware without an RTL measurement is guesswork. |

None of these are exotic. All are ordinary work. **None should be done now.**

## 4. What this means for desktop work happening TODAY

*The only actionable section. These cost nothing now and preserve the option.*

1. **Keep the engine free of the UI.** It is free today; it stays free. Any
   conductor→inspector dependency closes this door permanently.
2. **Keep backends `dlopen`'d.** Do not add a link-time audio dependency for
   convenience. This is what makes a build portable to a target with a different
   audio stack.
3. **Feature-gate optional subsystems as they are touched**, rather than in one
   later migration. The WASM host is the obvious first one and is worth doing on
   desktop merit alone — see §5.
4. **Keep the Protocol Plane ABI-stable.** `nullherz-traits` command and telemetry
   schemas are what would let a desktop and a standalone unit of different
   versions talk to each other. Breaking changes there are cheap now and
   expensive later.
5. **Do not assume a desktop OS** in new code: no assumption of a window, a
   filesystem layout, or a user session in the engine or conductor.

## 5. A pattern worth naming: unconditional cost for opt-in capability

Two independent instances found on the same day, in unrelated subsystems:

- **KeySync** — a phase vocoder in every deck chain costing **21.3 ms of latency
  on every block**, serving the KEY latch, which defaults to **off**.
- **wasmtime** — **128 crates** in every build, serving a WASM sidecar host the
  console **cannot reach at all**.

Both are the same defect class: *a capability nobody enabled is charged to
everybody, unconditionally*. Neither was visible in any status document, because
both are costs rather than failures — nothing breaks, so nothing reports.

This matters for the hardware direction specifically, because a stripped build is
exactly the exercise that surfaces this class. But it is worth watching for on
desktop now: when adding a capability, ask what it costs when switched off, and
make that cost zero if it can be.

---

**Entry conditions — do not start this before all of these hold:**

1. The desktop product is finished and shipping.
2. Gate 1 is green on real hardware (60 minutes, 0 xruns).
3. A real RTL measurement exists, so hardware claims can be measured.
4. A stripped build profile exists and is exercised in CI — proving modularity on
   the desktop before betting a product on it.
