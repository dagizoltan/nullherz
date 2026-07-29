# Archive — superseded documents

**These documents are kept for history. Do not use them as a source of truth, and
do not update them.**

They are here because they described intent as though it were status. Each was
written in good faith, but the project accumulated two parallel accounts of
itself, and they disagreed about basic facts:

| These documents said | The code said (measured 2026-07-28) |
| :--- | :--- |
| "PRODUCTION BETA", subsystem matrix mostly ✅ | 14 of 24 registered processors are unreachable from the console (`known_unreachable()` in `reachability_gate_test.rs`) |
| "fully green test suite (200/200)" | ~360 tests; the documented gate `scripts/verify.sh` was **red** — it ran `cargo test` in debug, where a wall-clock budget assertion fails on build profile alone |
| CI gate in place (`ci.yml`, `kani.yml`) | `ci.yml` had been deleted in commit `968cd12`; only `kani.yml` remained, and it runs weekly, not per-PR |
| Q3 pillars: libp2p, RDMA / InfiniBand, sub-100 µs network audio | The same month's measured roadmap recorded 453–642 xruns at Gate 1, and a −5 semitone pitch shift applied to every track on load, unasked |

The last row is the reason this folder exists. A directive to prototype RDMA and
a roadmap entry saying the console transposes the user's music are not two views
of one project; only one of them can be the plan.

## What replaced them

- **What the system *is*** — [`../system/ARCHITECTURE.md`](../system/ARCHITECTURE.md).
  Reverse-engineered from the tree, and explicitly subordinate to the code: when
  it and the source disagree, the source wins and the doc gets fixed in the same PR.
- **What is *wrong* and what happens next** — [`../roadmap/IMPLEMENTATION_ROADMAP_2026_07.md`](../roadmap/IMPLEMENTATION_ROADMAP_2026_07.md)
  and [`../system/PRE_IMPLEMENTATION_DESIGN_GATE_2026_07.md`](../system/PRE_IMPLEMENTATION_DESIGN_GATE_2026_07.md).
  These are measured, specific, and honest about their own failures — including
  the note that the Gate 1 harness could not fail until 2026-07-27 because it
  graded on a counter nothing incremented.

## The habit worth keeping

The roadmap and the reachability gate share one discipline that the archived
documents lacked: **a claim is only as good as the thing that would fail if it
were false.** A `[VERIFIED]` tag next to a subsystem name is not evidence. A
failing test, a measured number, or a gate that a regression trips is.

Status claims belong in the document that also carries the way to disprove them.
