# Nullherz Reverse-Engineering Evaluation

**Pass date:** 2026-08-02, against `main` at `26c9fb1`.
**Method:** read the tree, then run it. Every number below was produced on this machine by a
command quoted next to it, or by a `grep` whose output is reproducible. Nothing here is
inherited from a previous pass.
**Reference machine:** 4-core Intel Xeon @ 2.10 GHz, no `isolcpus`, no `nohz_full`, stock
`cargo build --release`. **This is not the box the July figures came from** (a 3.28 GHz
desktop), so where an older number exists it is quoted beside mine rather than replaced.

> This document is the *audit*. [`../system/ARCHITECTURE.md`](../system/ARCHITECTURE.md) is the
> *reference* — what the system is, maintained in the same PR as the code. Read that first.
> This one exists to answer a narrower question: **does the tree still match what we say about
> it?** Findings that survive get folded into the reference and disappear from here.

---

## 1. Verdict

The engine is real, coherent, and measurably good in the places it has been measured. The
execution plane is the strongest part of the codebase: the RT invariants in
[`AGENTS.md`](../../AGENTS.md) are actually enforced (by `clippy.toml`, by the type system, and
by tests that fail on violation), the graph VM does what the docs claim, and the console's
signal path has been characterised to a standard most DAWs never publish.

Five things are true at once, and the ranking matters:

0. **Nothing has gated `main` since 2026-07-26.** §2.2. The last 30 CI runs all concluded
   `failure` without ever acquiring a runner — 3-second jobs, no logs, and even the
   `continue-on-error` advisory job "failing". The workflow file is present and correct, which is
   exactly why this is invisible: a green-looking gate that never executes. Listed first because
   it changes how every other claim on this page should be read, and because it is the only item
   here that cannot be fixed from inside the repository.
1. **The code is better documented than the docs.** The load-bearing explanations live in
   doc-comments next to the constant or the branch they justify (`execution.rs`,
   `reachability_gate_test.rs`, `dj.rs`). Those comments are correct. The prose docs are where
   drift accumulates.
2. **Most of the drift is in the docs, not the code.** Seven discrepancies (§3). **Five are a
   doc quoting a superseded state and are fixed in this commit**; two (§3.1, §3.2) need code and
   are left for someone who can re-audit what they touch. Two of the five doc fixes were in
   `AGENTS.md` itself, and one of those would have led a contributor to use a sentinel value that
   is now a legal graph index.
3. **`FloatX16`'s AVX-512 and WASM arms do not compile, and never have.** §4. Both are
   `cfg`-gated behind target features no CI job selects, so nothing type-checks them; enabling
   AVX-512 — or simply `-C target-cpu=native` — fails the build outright. The default build is
   therefore SSE2 baseline while the DSP is written for 16-wide. Of the three findings here that
   need code rather than prose, this is the one with real performance at stake.
4. **Reachability remains the honest weak spot, and the arithmetic gives it away.** Of 26
   registered processors the console instantiates **12**, leaving **14** unreachable — but
   `known_unreachable()` carries **15** entries. The extra one is StereoUtility, which the
   allowlist declares unreachable and every deck instantiates (§3.1). Declaring absences with a
   reason each is the right posture; the list having one more entry than there are absences is
   what tells you nothing re-checks it. It also conflates "installed on demand" with "not yet
   wired", which makes the headline count mean less than it appears to.

---

## 2. What the system measures, right now

### 2.1 Size

| | |
| :--- | ---: |
| Rust source (`crates/` + `sidecars/`, tests included) | **59,014 lines** |
| Workspace crates | **19** |
| Sidecar binaries | **8** |
| `#[test]` / `#[tokio::test]` functions | **404** |
| Kani proof harnesses | **8** |
| `unsafe` occurrences | **450** (138 of them in `nullherz-backends` — see §6) |

`find crates sidecars -name '*.rs' | xargs wc -l | tail -1`

Test count by crate — useful for spotting where verification is thin:

| crate | tests | crate | tests |
| :--- | ---: | :--- | ---: |
| `nullherz-conductor` | 113 | `nullherz-backends` | 16 |
| `audio-dsp` | 60 | `nullherz-topology` | 8 |
| `nullherz-processors` | 59 | `nullherz-mixer` | 8 |
| `nullherz-traits` | 32 | `sidecar-sdk` | 4 |
| `nullherz-inspector` | 29 | `nullherz-ui-hal` | 3 |
| `audio-core` | 26 | `fx-runtime` | 3 |
| `ipc-layer` | 22 | `nullherz-gateway` | 1 |
| `nullherz-dna` | 20 | sidecars (all 8) | 0 |

**`fx-runtime` (3 tests for the process host, the cgroup limiter and the WASM runtime) and the
eight sidecar binaries (0) are the thinnest verification in the workspace.** That is the
out-of-process failure surface — the code path a third-party plugin runs through — and it is
the least tested. Worth stating plainly rather than leaving it implicit in a table.

`cargo check --workspace --all-targets` completes **clean, zero warnings** (verified this pass,
locally).

### 2.2 CI has not executed successfully once since 2026-07-26

**The last 30 CI runs — every run in the API's first page, back to 2026-07-26 — concluded
`failure`. Zero successes. That includes every commit on `main`, among them `26c9fb1`, the base
this branch is cut from.**

The jobs are not failing; they are not running:

| evidence | reading |
| :--- | :--- |
| `runner_id: 0`, `runner_name: ""` | no runner was ever assigned |
| `started_at` → `completed_at` = **3 seconds** | a `cargo check` of this workspace takes minutes |
| logs return **HTTP 404** | no log stream was ever produced |
| **`Clippy (advisory)` also "failed"** | it is `continue-on-error: true` and cannot fail on lint count |

That last row is the one that settles it. An advisory job whose only failure mode is disabled
still reports failure, so whatever is wrong sits **upstream of every job's contents** — runner
availability, quota, or Actions permissions on the repository. Nothing in the workspace can
cause this and nothing in the workspace can fix it.

**Why this belongs in an audit rather than a bug report:** `ARCHITECTURE.md` §5 has carried the
note that `ci.yml` and `.githooks/pre-push` were deleted in `968cd12` and restored on 2026-07-28,
with the standing instruction *"if a future audit finds the doc and the workflow disagreeing
again, trust `.github/workflows/`."* This audit is that future audit, and the instruction does
not help: **the doc and the workflow agree, both are correct, and `main` is ungated anyway** —
not because the gate was removed, but because it never executes. A workflow file that is present
and correct reads as a working gate in every review, in every doc, and on every PR page.

The repair is `scripts/verify.sh`, which runs the same checks locally and is the reason this pass
could verify anything at all. Wiring it up as the pre-push hook
(`git config core.hooksPath .githooks`) is currently the *only* gate that actually runs. Until
Actions is restored, treat a green PR page as unverified.

### 2.3 The bootstrapped 4-deck console, as actually built

Measured by instantiating `Conductor::with_library_path(":memory:")` → `setup_engine()` →
`bootstrap_4channel_mixer()` and reading `topology_manager.active_node_types` directly:

| | measured | limit | headroom |
| :--- | ---: | ---: | ---: |
| Nodes occupied | **58** | `MAX_NODES` 128 | 55% |
| Highest node index | **57** | 127 | — |
| Highest deck buffer index | **86** | `MAX_BUFFERS` 240 | 64% |
| Named nodes in telemetry map | **42** | `NODE_MAP_SLOTS` 64 | 22 slots |

Three observations fall out of this:

- **The node span is contiguous, and the count equals it.** 58 nodes occupy indices 0–57 with no
  gaps. The standing note that "the allocator leaves gaps — the practical ceiling is index-bound,
  not count-bound, so budget against 57 rather than 34" was recorded against a **34**-node
  console spanning the same 57 indices. I cannot reproduce that: today the layout accounts for
  every index — 11 nodes per deck (sampler, pitch slot, DNA slot, gain, biquad, stereo utility,
  one FX slot, isolator, two cue gains, sequencer) × 4 = 44, plus 14 at the master = 58. Whether
  the console genuinely grew by 24 nodes or the 34 was a miscount, I have not established, and
  the distinction does not change the advice: **budget against 58/128**, which is now both the
  count and the span.
- **Buffer occupancy reaches 86 of 240**, well clear of the wall that motivated raising
  `MAX_BUFFERS` from 128.
- **`NODE_MAP_SLOTS` headroom is 22, not 34.** The doc-comment on that constant says "a 4-deck
  console registers 30 names"; it registers 42. The constant is still correctly sized — a stale
  comment, not a bug — but the margin is a third smaller than stated, and overflow drops an
  *arbitrary* subset (`HashMap` iteration order), which is precisely why the comment exists.

### 2.4 Block cost

`cargo run --release -p nullherz-conductor --example bench_console_block`, 20,000 blocks,
4 decks live, 256 frames @ 44.1 kHz (budget 5805 µs):

| | µs | % budget |
| :--- | ---: | ---: |
| mean | 224.22 | 3.9% |
| p50 | 213.94 | 3.7% |
| p90 | 265.77 | 4.6% |
| p99 | 354.44 | 6.1% |
| p99.9 | 612.45 | 10.6% |
| max | 4041.35 | 69.6% |

The July figure was **117 µs mean on a 3.28 GHz box**. Do not read the difference as a
regression, and do not attribute it to the sinc-16 resampler that landed in between — **this
bench runs every deck at playback rate 1.0**, because the engine's default transport is 120 BPM
(`audio-core/src/engine/mod.rs:196`) and the bench registers its tones at `metadata.bpm = 120.0`,
so `sync_rate = 120/120 = 1.0` and every voice takes the resampler's bit-exact short circuit. At
rate 1.0 the sinc kernel is *faster* than the cubic it replaced (10.7 vs 13.3 ns/sample), so the
resampler change cannot be the cause here.

The nominal clock ratio accounts for ~1.56× of the ~1.9× gap. **The remainder is not isolated**,
and the candidates — different microarchitecture, different cache, a shared VM with unknown turbo
behaviour — are not separable without running both binaries on one machine. Recording it as
unexplained rather than guessing: this codebase has already been burned once by an
attribution that fit the shape of the data and was wrong (the THD residual that turned out to be
the tone generator, ARCHITECTURE §2).

What the number does say plainly: the console is **under 4% of block budget at the mean** on a
modest 4-core VM, and the p99.9 is the figure to watch, not the mean.

Also worth noting for anyone tuning against this bench: **at rate 1.0 it cannot see the
resampler at all**, which is the same coverage gap the golden master render has. A bench of the
sinc kernel's real cost needs a tempo-synced deck.

**The `max` of 4041 µs is scheduler jitter on a shared, non-isolated 4-core VM, not DSP.** It is
69% of budget and it would be an xrun on a busier machine. Any conclusion about live-latency
headroom from this box is unsound until it is re-measured somewhere with core isolation.

Per-node breakdown (`--example profile_console_nodes`, 8000 blocks). It reports 270 µs of summed
node time against the bench's 224 µs wall-clock mean; **the two are not directly comparable** and
the difference is not a finding — the profiler derives ns from the engine's cycle telemetry via a
calibrated `ns_per_cycle`, which its own header flags as approximate. Every node shares that
factor, so the **ranking and the % share** are the trustworthy output:

| type | share | count | note |
| :--- | ---: | ---: | :--- |
| Sampler | **83.5%** | 5 | 4 decks + preview — voice mixing, *not* interpolation (see below) |
| DjIsolator | 7.3% | 4 | 8 biquads per deck, unconditionally |
| Biquad | 3.1% | 8 | deck filter + FX slot |
| Limiter | 2.0% | 1 | master ceiling |
| *(unnamed)* | 1.3% | 12 | 8 Bypass slots + 4 StereoUtility — see §3.2 |
| MasteringEq | 1.2% | 1 | master tone |
| Gain / Summing / Sequencer / Crossfader / Capture | 1.6% | 27 | |

**The sampler is 83.5% of console DSP cost, and that is measured with the resampler
short-circuited.** Every optimization conversation about this engine should start at the sampler
and nowhere else: the isolator's "runs unconditionally on every deck" note is correct, but it is
a 7.3% item against an 84% one. Note what this profile does *not* show — with all four decks at
rate 1.0, none of that 83.5% is interpolation. It is voice mixing, envelope and gain over five
sampler nodes. A tempo-synced profile would add the sinc kernel *on top* of this, and the July
worst-case estimate for that (32 voices at +2.5%) was 1328 µs, ~25% of budget on the faster box.
**That profile has not been run.** It is the single most useful measurement missing.

---

## 3. Drift found this pass

Each item states the discrepancy, the evidence, and whether the fix is a doc edit or code.
All doc edits in this list have been applied in the same commit as this file.

### 3.1 The reachability allowlist has a hole, and a false entry is already in it — **CODE**

The console leaves 14 of 26 registered processors uninstantiated. `known_unreachable()` in
`conductor/tests/reachability_gate_test.rs` has **15** entries. The surplus one is
**StereoUtility**, listed as "available for FX chains; not in the default master chain". It is
not: `mixer/src/dj.rs:75` adds a `ProcessorTypeId(160)` node to **every deck**, and the probe
confirms `instantiated=true` for type 160.

The entry is harmless in effect but corrosive in principle, and the gate cannot catch it:

```rust
if instantiated.contains(&id) { continue; }   // reachable -> pass, allowlist never consulted
if allowed.contains(&name)    { continue; }   // declared  -> pass
```

A processor that *becomes* reachable is skipped before its allowlist entry is ever read, so the
entry sits there forever describing a state that no longer holds. The companion guard,
`test_unreachable_allowlist_has_no_stale_entries`, only checks that the named processor is still
*registered* — not that it is still *unreachable*.

This is precisely the failure the file's own header rails against ("a unit test proves a thing
WORKS, not that anything CALLS it"), one level up: an entry in the allowlist proves nothing
about the state it describes, because nothing re-checks it once written. One entry is
demonstrably false today; the other 14 are unverified rather than wrong. **Recommended fix:**
assert the complement — every name in
`known_unreachable()` must be absent from `instantiated`. That is three lines and it converts
the list from documentation into a contract. Not applied here; this pass is a doc pass, and the
change belongs with someone who can re-audit all 15 entries.

### 3.2 Three processor type ids exist only as magic integers — **CODE**

`ProcessorTypeId` carries 24 named constants in `nullherz-traits/src/commands.rs`, the ABI
crate. Three registered factories do not use them because they do not exist:

| processor | type id | declared as |
| :--- | ---: | :--- |
| StereoUtility | 160 | `ProcessorTypeId(160)` in `factory.rs:245` **and** `mixer/src/dj.rs:75` |
| Compressor | 170 | `ProcessorTypeId(170)` in `factory.rs:236` |
| Analysis | 180 | `ProcessorTypeId(180)` in `factory.rs:254` |

The consequence is already visible and is not hypothetical. `profile_console_nodes` renders
**12 of the console's 58 nodes as `?`** with a blank name, because its `type_name()` is a
hand-transcribed copy of the ABI constant list (`examples/profile_console_nodes.rs:78`). The
transcription reproduces the ABI's shape exactly, including its faults:

- **StereoUtility (160) is structurally absent** — it has no constant to copy. That is 4 of the
  12 unnamed nodes, and it is the failure this finding is about.
- **Bypass (3) was dropped by hand** even though it *does* have a constant. That is the other 8,
  and it is what hand-transcription costs independently.
- **`BiquadEq` (11) is present in the table** for a type nothing can instantiate — see below.

An ABI whose type table is incomplete forces every consumer to re-derive the integers, and the
first tool to try got it wrong in both directions at once: it omitted a live type and carried a
dead one.

That dead one: **`ProcessorTypeId::BIQUAD_EQ` (11) is a named constant with no registered
factory.** `registry.create(BIQUAD_EQ, ..)` returns `None`. It appears once in a test, in
`setup_honours_sample_rate_test.rs`'s list of "processor types the 4-deck console actually
instantiates", where a `let Some(..) else { continue }` guard silently skips it — so one of that
test's nine cases has never executed.

### 3.3 `AGENTS.md` carried two stale invariants, both from one change — **DOC, fixed**

The agent guidelines are the file that tells contributors — human and otherwise — what the
invariants *are*. Two of them were wrong, and both were collateral from raising `MAX_NODES`
from 64 to 128:

1. **The address-space sizes.** It said `MAX_NODES = 64`, `MAX_BUFFERS = 128`. The code has said
   `128` / `240` since the tap-budget increase (`execution.rs:85,100`).
2. **The logical sentinel values**, and this one is the dangerous half. It said "`NodeConventions`
   constants (PREVIEW = 111, sequencers 70–73) are LOGICAL sentinels deliberately ≥ MAX_NODES".
   Those numbers were out of range only while `MAX_NODES` was 64. **At 128 all five are legal
   graph indices**, so anything written to those constants from the guidelines would be a command
   aimed at a real node rather than a sentinel the conductor translates.

The code got this right and got it right *well*: roadmap item 0.4 moved the sentinels to
`LOGICAL_BASE = 0xFFFF_FF00`, added `is_logical()` as the preferred test, and added a
`_SENTINELS_ABOVE_MAX_NODES` const assert so any future `MAX_NODES` increase that reopens the
hole is a compile error rather than an audio bug. `AGENTS.md` simply was not swept when that
landed. Both entries now point at the mechanism (`is_logical`, and "read the constants from
`execution.rs`") instead of quoting numbers that can rot. Fixed.

**The generalisable lesson:** a constant that appears in prose as a literal is a copy with no
link back to its definition, and raising it updates one of the two. Where a doc must state a
value, it should also say where the value lives — which is what both replacements now do.

### 3.4 The deck chain was documented in its pre-insert-slot form — **DOC, fixed**

`ARCHITECTURE.md` §1.3 described the deck strip as
`Sampler → DnaMorph → KeySync → Gain → …`. That has not been the chain since the insert slots
landed; §2 of the same document describes the current one correctly, so the file contradicted
itself. Actual chain, from `mixer/src/dj.rs`:

```
Sampler → [pitch slot: BYPASS] → [DNA slot: BYPASS] → Gain → Biquad
        → StereoUtility → [FX slots] → DjIsolator → private L/R + cue send
```

Both source slots hold `ProcessorTypeId::BYPASS` until the operator engages something, at which
point `TopologyCommand::SwapProcessor` installs the real processor with routing preserved. This
is what took deck→master latency from 28.7 ms to 7.4 ms, and it is why KeySync and DnaMorph now
appear in `known_unreachable()` — correctly, and with the clearest reasons in that list.

### 3.5 Processor counts were stale — **DOC, fixed**

Registry holds **26** factories (was documented as 24), of which **12 are instantiated** by the
bootstrapped console and **14 are not**, against an allowlist of 15 (see §3.1 for the surplus
entry). Full map in §5. Note the direction: the declared-unreachable list grew. Two of the
additions (KeySync, DnaMorph) are *good* news — they moved out of the default chain to buy back
21.3 ms of latency and are now installed on demand — which is exactly why the raw count is a
poor metric and the reasons column is the part to read.

### 3.6 Two ✅ in the feature matrix fail the matrix's own rule — **DOC, fixed**

`FEATURE_MATRIX.md` opens by declaring that a ✅ must mean "a user can reach it". Two entries
did not clear that bar:

- **Library Analysis Pipeline** was credited to "`folder_monitor` auto-scan". Boot auto-scan was
  deliberately removed from both the daemon and the inspector on 2026-07-22 (it decoded every
  file into the registry at startup and froze on large libraries). `start_auto_scan` has zero
  call sites. Scanning is now on-demand via `ResourceCommand::ScanFolder`, which is a *better*
  design and a *different* claim.
- **Disk Streaming** was ✅ while `StreamingSampler` — the node that consumes the stream — sits
  in `known_unreachable()`. The feeder is wired; nothing in any live graph reads from it. The
  debt log already said so; the matrix disagreed with it.

### 3.7 The debt log's xrun entry was both stale and wrong — **DOC, fixed**

"Threaded Audio Backend Xrun Blindness" claimed the Threaded backend "cannot programmatically
detect or log hardware-level underruns … unlike the ALSA or PipeWire backends". Two errors in
one sentence:

- **Threaded does detect them.** `threaded.rs:99` compares each cycle's elapsed time against
  `period_ns(period_size, sample_rate)` and increments `xrun_counter` past a 20% overrun,
  surfaced through `AudioBackend::xruns()`. It counts *software deadline misses* — the only
  underrun a sleep-paced software clock can observe. A different measurement from ALSA's, but a
  real one.
- **PipeWire does not.** `PipewireBackend` and `JackBackend` both inherit the trait default
  `xruns() -> None`. **ALSA and Threaded are the only two that measure anything.**

Worth calling out because the trait went out of its way to make this legible: `xruns()` returns
`Option<u64>` specifically so "this backend does not count" cannot be read as "this backend
counted zero" — which is the exact confusion the debt entry then reintroduced in prose. Type
signatures decay into prose faster than prose decays into type signatures.

---

## 4. `FloatX16` has three platform arms and only one of them compiles

This is the most significant finding of the pass. It is not in any existing document, and both
`ARCHITECTURE.md` and the previous version of this file described the broken arms as working
features.

### 4.1 The claim

`audio-dsp` is written against three width tiers: `FloatX4`, `FloatX8`, and a `FloatX16`
documented in its own source as *"supporting AVX-512 or WASM SIMD128 (via 4x f32x4) where
available"*. Selection is by `#[cfg(target_feature = ...)]`, resolved at compile time — there is
no runtime dispatch anywhere in the tree (`is_x86_feature_detected!` appears zero times).

```rust
pub struct FloatX16 {
    #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
    pub(crate) val: wide::f32x16,
    #[cfg(all(not(avx512f), target_arch = "wasm32", target_feature = "simd128"))]
    pub(crate) parts: [f32x4; 4],
    #[cfg(all(not(avx512f), not(wasm simd128)))]
    pub(crate) low: f32x8,
    pub(crate) high: f32x8,
}
```

### 4.2 Neither non-default arm builds

The reference machine reports `avx512f avx512bw avx512cd avx512dq avx512vl avx2` in
`/proc/cpuinfo`, so it can select the AVX-512 arm. It does not survive it:

```
$ RUSTFLAGS="-C target-feature=+avx512f" cargo check -p audio-dsp
simd_vec.rs:63:27  error[E0425]: cannot find type `f32x16` in crate `wide`
simd_vec.rs:75:29  error[E0433]: could not find `f32x16` in `wide`
simd_vec.rs:98:29  error[E0433]: could not find `f32x16` in `wide`
simd_vec.rs:339:59 error[E0433]: could not find `f32x16` in `wide`
error: could not compile `audio-dsp` (lib) due to 4 previous errors
```

`wide 0.7.30` — the pinned version — has no `f32x16` type. **The AVX-512 path has never been
compiled by anyone.** `-C target-cpu=native` on any AVX-512 host fails the same way, which means
the obvious "just build it optimised for my machine" move breaks the build.

The wasm arm fails worse:

```
$ cargo check -p audio-dsp --target wasm32-unknown-unknown -C target-feature=+simd128
oscillators.rs:244  error[E0609]: no field `low` on type `FloatX16`
spectral.rs:205     error[E0609]: no field `low` on type `FloatX16`
... plus ~20 f32x4/v128 mismatches in simd_vec.rs
```

### 4.3 The mechanism, which is the part worth internalising

**The type has a three-way `cfg` split; its consumers have a two-way one.** `oscillators.rs:242`
and `spectral.rs:203` both branch on `#[cfg(not(target_feature = "avx512f"))]` and then reach
directly for `b_mod_phase.low` / `v_val.high` — fields that exist **only in the third arm**. On
wasm32+simd128, `not(avx512f)` is true, so consumers take the `.low`/`.high` path while the type
is `[f32x4; 4]`. The abstraction leaks its fallback representation to its callers, so the
fallback is the only representation that can ever compile.

This is why nobody noticed: on the default x86-64 target the two-way and three-way splits agree,
so `cargo check --workspace --all-targets` is clean and the suite is green — neither ever
compiles the other two arms. The reference machine in the design gate has no AVX-512, and CI
builds only `ubuntu-latest` x86-64. **`cfg`-gated code that no build configuration ever selects
is not merely unexercised — it is not type-checked.** A green workspace says nothing about it.

### 4.4 What the default build actually gets

With both alternate arms non-viable, a stock `cargo build --release` compiles for the
**x86-64 baseline, which is SSE2**. `wide`'s `f32x8` lowers to a pair of SSE2 registers, not one
AVX2 register. The DSP asks for 16-wide; the hardware is handed 4-wide. Given the resampler is
now 83.5% of console DSP cost (§2.4), the headroom here is not academic.

The rest of the release profile is cargo defaults: **no LTO, 16 codegen units, `opt-level = 3`**
— there is no `.cargo/config.toml` and no `[profile.release]` anywhere in the workspace. For an
RT audio engine, 16 codegen units leaves cross-crate inlining on the table across exactly the
boundary that matters: `audio-dsp` kernels called from `audio-core`'s graph executor.

### 4.5 Recommendation

Three separable pieces of work, cheapest first. None is applied here — this is a doc pass, and
each is a decision with a measurement attached.

1. **Add `[profile.release]` with `lto = "thin"`, `codegen-units = 1`.** Independent of the ISA
   question, mechanical, and measurable against `bench_console_block` in one afternoon.
2. **Decide what `FloatX16` is for.** Either fix both arms and add CI jobs that compile them
   (a `cfg` arm without a CI target is a liability, not a feature), or delete them and let
   `FloatX16` be honestly what it is today: two `f32x8`s. The current state — a documented
   capability that does not build — is the worst of the three.
3. **Then** ask the ISA question properly. `target-cpu=native` is not shippable (`SIGILL` on
   older hardware). The real options are runtime dispatch, a documented `RUSTFLAGS` for
   self-builds, or a measured "baseline is enough" note. Any beats the status quo, which is that
   the question has never been asked.

---

## 5. Processor reachability, in full

26 registered factories. `instantiated` is measured against the real bootstrapped console.

| id | processor | in console | status |
| ---: | :--- | :---: | :--- |
| 1 | Biquad | ✅ | deck filter + FX slot |
| 2 | Gain | ✅ | deck gain + cue sends |
| 3 | Bypass | ✅ | the insert slots, at rest |
| 10 | Sampler | ✅ | 4 decks + preview |
| 20 | Crossfader | ✅ | master, per side |
| 30 | Summing | ✅ | buses, master, cue |
| 70 | Sequencer | ✅ | one per deck (groove micro-timing) |
| 110 | Capture | ✅ | master tap |
| 120 | DjIsolator | ✅ | deck EQ |
| 160 | StereoUtility | ✅ | **declared unreachable — see §3.1** |
| 200 | Limiter | ✅ | master ceiling |
| 220 | MasteringEq | ✅ | master tone |
| 0 | Delay | ❌ | FX chains |
| 40 | Spectral | ❌ | sidecars/plugins |
| 50 | Wavetable | ❌ | synthesis source |
| 60 | Modulation | ❌ | mod matrix |
| 80 | EnvelopeFollower | ❌ | mod matrix / sidechain |
| 90 | Granular | ❌ | transfusion path |
| 100 | SpectralMorph | ❌ | transfusion path |
| 130 | SimdBiquad | ❌ | FX chains |
| 140 | KeySync | ❌ | **on demand** — KEY latch swaps it into the pitch slot |
| 150 | PersonalityInheritance | ❌ | Phase 6 |
| 170 | Compressor | ❌ | FX chains |
| 180 | Analysis | ❌ | measurement tap, on demand |
| 190 | DnaMorph | ❌ | **on demand** — same mechanism as KeySync |
| 210 | StreamingSampler | ❌ | Phase 2; see §3.6 |

"On demand" is a materially different state from "not wired", and the two are currently mixed
in one list. Splitting `known_unreachable()` into `installed_on_demand()` and
`not_yet_reachable()` would make the count mean something again.

---

## 6. Notes an architect should carry forward

Not defects. Properties of the system that are load-bearing, non-obvious, and not written down
anywhere else.

**The audio backends are hand-rolled `dlopen`/`dlsym`, not binding crates.** `nullherz-backends`
depends on `libc` and nothing else; ALSA, JACK and PipeWire are reached by opening
`libasound.so.2`, `libjack.so.0` and `libpipewire-0.3.so.0` at runtime and resolving symbols
into hand-written function-pointer tables (138 `unsafe` occurrences, the most of any crate).
This buys two real things — the workspace builds and tests with no audio system packages
installed (which is why CI needs none), and a missing backend degrades to a runtime error rather
than a build failure — at the cost of an unchecked FFI surface that no compiler is validating
against the real headers. That trade is defensible and deliberate. It should be a documented
decision rather than folklore, because the next person to add a backend will not guess it.

**`IPC_BLOCK_SIZE` (256) is a different constant from `MAX_BLOCK_SIZE` (1024), on purpose.**
`AudioBlock` is a 1088-byte protocol ABI type (`const _: () = assert!(size_of::<AudioBlock>() ==
1088)` in `ipc-layer`), allocated in fixed 16-deep SHM rings per input, output and sidechain
channel of every sidecar; tying it to the render capacity would multiply every ring by 4×.
**The consequence is a live limitation:** a render block larger than 256 frames must be split
before crossing the IPC boundary, and the bridge currently *clamps rather than chunks*. The
offline bounce path renders at `MAX_BLOCK_SIZE` — so bounce plus a sidecar is a combination the
IPC layer does not currently serve, and it asserts rather than silently truncating. Correct
behaviour; undocumented outside the constant's own doc-comment.

**Telemetry is one fixed `#[repr(C)]` POD struct of roughly 8 KB**, carrying `MAX_NODES`-wide
arrays for per-node times, peaks and levels, plus spectrum, goniometer, waveform and the
64-slot node-name map. That is what makes it safe to push through a lock-free ring with no
allocation — and it also means every field is paid for on every snapshot whether or not a
consumer reads it. Adding a field is cheap; adding an array is not.

**Network listeners bind `0.0.0.0`, and 9001 is overloaded.** The gateway is localhost-only
(`127.0.0.1:9001`, TCP). But the remote-sidecar listener binds `0.0.0.0:9000`, the PTP engine
binds `0.0.0.0:319`, and the DNA gossip overlay binds `0.0.0.0:<port>` — all interfaces, by
default. Separately, the discovery beacon broadcasts to UDP `255.255.255.255:**9001**`, the same
number the gateway serves on TCP. There is no bind conflict (different protocols) and no bug
today, but one number naming two unrelated services is a trap for whoever debugs it next.

**`clippy.toml` enforces the RT rules, and the escape hatch is honest.** `std::sync::Mutex`,
`RwLock`, `thread::spawn` and `thread::sleep` are all `disallowed`. The codebase runs 69
`parking_lot` locks against 4 `std` ones. The worker pool opens with a file-level
`#![allow(clippy::disallowed_methods, clippy::disallowed_types)]` — which is the correct place
for it (it *is* the thread-spawning primitive) and, being a single visible line, is auditable.

---

## 7. What is still not measured

Carried forward from the July pass and still true. Listed so the gaps stay visible rather than
being mistaken for clean results:

- **Phase and group delay.** Never measured. The isolator's LR crossovers phase-rotate at unity
  by construction, and the argument for not bypassing them per-deck rests on that rotation —
  an argument currently derived from the structure, not from a measurement.
- **Stereo imaging and correlation.** Not measured since the pan-law fix.
- **Intermodulation distortion.** Never measured. THD+N with a single tone does not predict it.
- **Behaviour across sample rates.** Contract tests prove coefficients *move* with the rate;
  nothing measures whether the console is transparent at 96 kHz.
- **Real-hardware jitter with core isolation.** The reference box has no `isolcpus`, so the
  4041 µs worst-case block in §2.4 measures the VM, not the engine.
- **A human listen** is owed on the crossfader default change and on a tempo-synced mix after
  the resampler change. Neither is covered by the golden render, because every deck in that
  fixture plays at rate 1.0 and takes the resampler's bit-exact short circuit.
