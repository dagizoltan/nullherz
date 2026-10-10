# Insert Ecosystem: Design Proposal

**Status:** PROPOSAL — not implemented. Decisions required before any code.
**Date:** 2026-10-10
**Scope:** the contract between the insert slots (deck FX rack, pad subchannels,
master chain) and the 45 registered processors that can occupy them.

---

## 0. The finding that motivates this

Before proposing anything: a `SetParam` addressed to one node reaches every
other node, and ten processors act on it.

```
reverb room_size: before=0.8 after=0.98
  (command was addressed to node 7, param 0 = cutoff 20000 Hz)
```

`ProcessorGraph::apply_command_with_context` has an addressed arm for
`SetMacro` — it resolves `target_id` to a node index and calls `set_parameter`
on that node alone. It has **no arm for `SetParam`**, which therefore falls into
the catch-all:

```rust
_ => { for node in self.nodes.iter() { unsafe { (*node.processor.get()).apply_command(command); } } }
```

So every `SetParam` is broadcast to all 128 node slots, and correctness depends
entirely on each processor's own hand-written filter. 36 processors have such a
filter. **Ten of them do not bind `target_id` at all:**

`neural_nam`, `neural_tcn`, `tube_preamp`, `hypernetwork_eq`, `neural_filter`,
`algorithmic_reverb`, `neural_ssm`, `modulation_fx`, `neural_saturator`,
`multiband_compressor`.

Parameter 0 means `room_size` (0–0.98) on the reverb, `cutoff` (20–20000 Hz) on
the filter, and `gain_db` (−24–24 dB) on the hypernetwork EQ. Because all three
are FX-rack-loadable and the rack is now wired, moving a filter cutoff writes
20000 into the reverb's room size — clamped to its maximum — and +24 dB into the
EQ's gain. Audible, and reachable from the shipped UI.

`AGENTS.md` already warns about exactly this: *"A processor arm MUST match its
own address … an untargeted `{ node_idx: _, .. }` match makes one deck's control
fire on all of them."* The warning is correct and has been ignored ten times,
which is the argument of this document: **the insert contract is enforced by
convention across 36 hand-written match arms, and convention has failed.**

---

## 1. Every insert contract is an optional default

`AudioProcessor` and `SignalProcessor` already define the right hooks. All of
them default to a no-op or a zero, so a processor that ignores one is
indistinguishable from one that has nothing to say. Adoption across the 42
processor files:

| hook | implemented | what the default means |
| :--- | ---: | :--- |
| `set_parameter` | 39 / 42 | — |
| `metadata()` → declared params | **27 / 42** | **12 processors expose 3–5 real parameters each that no host can discover** — the one figure here that survived checking |
| `setup(AudioConfig)` | 16 / 42 | 26 processors do not implement it — see the caveat below |
| `latency_samples()` | 8 / 42 | 34 do not declare it, and **zero of them are wrong to** — see §1.1 |

Each row is a defect class rather than a style preference:

* **Addressing** (§0) — ten live cross-talk bugs.
* **Declared parameters** — `ParameterMetadata` carries `id, name, min, max,
  default`, which is everything a generic editor needs. The inspector
  references it **zero times**. That is why `mixer.rs` renders one hardcoded
  `"MIX"` knob for a loaded sidecar regardless of what it actually exposes.
* **Sample rate** — the reverb shipped tuned for 44.1 kHz whatever the device
  ran at, losing 54% of its tail at 96 kHz, because `ReverbFactory` discarded
  `_sample_rate` and `process` ignored `ctx`. `NeuralFilterProcessor` had
  `let sample_rate = 48000.0f32;` inside `process()`, which measured −0.15 dB
  where it should have been −12.04 dB at 96 kHz.

  **Corrected:** an earlier draft of this document read the `setup()` adoption
  figure as a defect count and said "a class with 26 candidates". It is not.
  Most of those 26 have no need of the rate — a gain stage, a summing node, a
  stereo width control. Scanning for actual hardcoded rates finds five sites,
  four of them legitimate: three inside `#[cfg(test)]` (`compressor.rs:171`,
  `deck_stem_matrix.rs:317` and `:371`, the latter two calling `setup()`), and
  a constructor default in `hypernetwork_eq.rs:26` whose `process` reads the
  transport properly. The real remaining defect was **one**, now fixed.

  That changes this item's justification rather than removing it: the value of
  §2.3 is preventing a fourth instance, not cleaning up twenty-six. Its gate
  therefore lands with an EMPTY allowlist, which is the strongest state a
  ratchet can start from.
* **Latency — WITHDRAWN, this was not a defect.** The claim was that a
  processor which delays and reports 0 is silently uncompensated. Checked: of
  the 34 that do not declare latency, only two have any delay state at all —
  `modulation_fx` and `tape_saturator` — and both delays are **LFO-modulated**
  (chorus/flanger, and tape wow/flutter). A modulated delay is the effect, not a
  fixed group delay, and PDC neither can nor should compensate a time-varying
  one. Reporting 0 is correct for both, and for the other 32, which genuinely
  have no latency. There is no undeclared fixed latency in the tree.

### 1.1 The same twelve processors skipped every contract

Not a coincidence, and the most useful thing in this survey. The ten processors
whose `SetParam` arm never bound `target_id` are **all** in the set that
declares no parameter metadata:

`algorithmic_reverb`, `hypernetwork_eq`, `modulation_fx`, `multiband_compressor`,
`neural_filter`, `neural_nam`, `neural_saturator`, `neural_ssm`, `neural_tcn`,
`tube_preamp` — plus `sample_drum_machine` and `synth_drum_machine`, which
declare no metadata but did filter correctly.

These are not twelve independent oversights. They are the processors written
without reference to the insert contract at all, because nothing required
reference to it: every hook is an optional default, so a processor can be
registered, loaded into a rack and made audible while implementing none of them.
That is the argument for this document in one sentence, and it is why the fix is
a mechanism rather than twelve patches.

### 1.2 What survived checking

Three of the four contracts were justified by the adoption table above. Two did
not survive measurement:

| contract | claimed | actual |
| :--- | :--- | :--- |
| Addressing (§2.1) | 10 processors mis-apply `SetParam` | **confirmed, and fixed** — measured cross-talk, reverb room size 0.8 → 0.98 |
| Declared parameters (§2.2) | 15 expose nothing | **confirmed** — 12 of them have 3–5 real parameters, ~44 invisible in total |
| Rate (§2.3) | "a class with 26 candidates" | **one** defect, now fixed; the gate landed for prevention |
| Latency (§2.4) | 34 under-report to PDC | **none** — withdrawn, see §1 |

The lesson for whoever extends this document: the adoption counts are counts of
hooks implemented. Reading one as a defect count was wrong twice out of three
tries. Measure the defect before scheduling the contract.

---

## 2. Proposed design

Four contracts, each made either impossible to get wrong or mechanically
enforced. In dependency order.

### 2.1 Address parameters in the graph, not in 36 processors

Give `SetParam` the arm `SetMacro` already has: resolve `target_id` to a node
index, bounds-check it, call `set_parameter` on that node alone.

* Fixes all ten cross-talk bugs in one place.
* Lets 36 hand-written `apply_command` arms be deleted, and makes the
  mis-addressing class unrepresentable rather than discouraged.
* `apply_command` stays for genuinely broadcast commands (safe mode, transport).

**This is independently shippable and should not wait for the rest.**

### 2.2 Declared parameters become the only parameter surface

`metadata()` becomes mandatory for anything registered as insert-capable, and
the inspector renders from it: one control per declared parameter, named, with
the declared range and default, sending `SetParam` with the declared id.

Consequences: the hardcoded `"MIX"` knob goes; the 15 processors with no
declared parameters either declare them or are marked not insert-capable; a new
processor becomes usable by writing metadata rather than by editing the UI.

### 2.3 One rate contract

`setup(AudioConfig)` is how a processor learns its rate. Anything whose DSP
depends on rate implements it, and factories pass the rate they are given.

Enforcement, in the style of `scan_defaulted_lookups` in the reachability gate:
a test that scans processor sources for rate literals (`44100`, `48000`,
`/ sample_rate` against a constant) and fails when the file does not implement
`setup`. The gate catches the *form*, not a list of known offenders.

### 2.4 Latency — withdrawn

This section proposed making `latency_samples()` mandatory. Measurement
(§1) found no processor with an undeclared fixed latency, so the contract would
add ceremony without fixing anything.

What is still worth having, and is much smaller: a PDC test over the **eight**
processors that DO declare a latency, driving an impulse through each and
asserting the measured group delay matches the declaration. That catches a wrong
declaration, which is the remaining failure mode, and does not ask the other 34
to assert a zero they already correctly report.

---

## 3. What this does not propose

* **No new DSP.** Not the reverb's 4+2 → 8+4 upgrade, not oversampling.
* **No change to the RT execution model.** Slot allocation, the graph compiler,
  PDC machinery and the command ring are unchanged. This is about the contract
  at the slot boundary.
* **No plugin format.** Sidecars already exist for out-of-process effects; this
  is about in-process inserts.

---

## 4. Decisions required

These change the shape of the work and are not mine to make.

1. **Scope of "insert-capable".** All 45 registered processors, or a declared
   subset? The registry already distinguishes reachable from
   `known_unreachable`; an `Insert` marker trait or a factory flag would make
   the mandatory contracts apply to a named set rather than to everything,
   which matters because samplers and sequencers are not inserts and should not
   be forced to declare insert metadata.

2. **Migration shape.** Big-bang (one change, all 45 conform) or a ratcheting
   gate (a list of known-unconforming processors that may only shrink)? The
   ratchet is how this repo handled reachability; it is slower but never leaves
   the gate red.

3. **Pad subchannels.** The deck rack has four real nodes per deck. The pad
   subchannel rack has *none* — it is presentational because no per-pad nodes
   exist (`TECHNICAL_DEBT_AND_STUBS.md` §3.1). Does this refactor allocate them,
   or stay deck-only and leave pads explicitly out of scope?

4. **Parameter identity across versions.** If declared metadata becomes the
   contract, a saved project references `(processor_type, param_id)`.
   Renumbering a processor's parameters then silently reinterprets saved
   sessions. Do we need stable parameter ids (and a test that they never move),
   or is project-format versioning enough?

---

## 5. Suggested order, if approved

1. §2.1 addressed `SetParam` + a cross-talk test. Small, fixes a live audible
   bug, independent of every decision above.
2. Decide §4.1 and §4.2.
3. ~~§2.3 rate contract + the scanning gate~~ **DONE.** Not for the reason
   given here first: the defect count was one, not twenty-six (see §1). The
   gate landed anyway, with an empty allowlist, because it retires a bug class
   this project has shipped three times — sampler source rate, reverb delay
   constants, neural filter cutoff.
4. §2.2 declared parameters + generic editor — the largest user-visible change,
   and it depends on §4.1 and §4.4.
5. ~~§2.4 latency contract~~ **WITHDRAWN** (§1). Optionally, the much smaller
   version: verify the eight declarations that exist.

---

*A finding in this document is a claim about a commit. Every measurement here is
reproducible from the tree at the date above; re-run before relying on it. One
figure in the first draft — "26 candidates" for the rate class — did not survive
that test and is corrected in §1; treat the remaining adoption counts as what
they are, counts of hooks implemented, not counts of defects.*
